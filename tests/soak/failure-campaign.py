#!/usr/bin/env python3
"""Bounded D1A failure/cancellation campaign; never runs the 100k resource soaks."""
import argparse
from dataclasses import dataclass
import json
import os
from pathlib import Path
import re
import sys
from types import SimpleNamespace

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tests/corpus"))
from run import Run, digest, save


@dataclass(frozen=True)
class Gate:
    name: str
    package: str
    selector: str
    scope: str
    target: str = "lib"
    features: str = ""
    ignored: bool = True
    hardware: bool = False
    directory_variable: str = ""
    receipt: str = ""


GATES = (
    Gate("core-encoder-panic", "asciiflow-core",
         "pipeline::tests::encoder_panic_cancels_source_finish_before_ordered_join",
         "Synchronized encoder/drop panic and source-finish cancellation before ordered join", ignored=False),
    Gate("core-primary-root", "asciiflow-core",
         "pipeline::tests::worker_panic_preserves_an_existing_substantive_failure",
         "Secondary worker panic retains the primary substantive root", ignored=False),
    Gate("core-slow-consumer-cancellation", "asciiflow-core",
         "pipeline::tests::cancellation_with_synchronized_slow_backend_and_encoder_joins_all_owners",
         "20 slow-backend and 20 slow-encoder cancellations with bounded queues and owned mocks; not native GPU cleanup", ignored=False),
    Gate("interop-borrowed-encoder-panic", "asciiflow-interop",
         "pipeline::tests::borrowed_encoder_panic_cancels_before_waiting_for_decoder_join",
         "Borrowed owner lifetime and panic cancellation ordering using test workers", ignored=False),
    Gate("native-avio-write-faults", "asciiflow-media",
         "ffmpeg::encoder::audio_regression_tests::d1a_native_avio_enospc_and_eio_preserve_finalization_cause_and_cleanup",
         "SimulatedOnly: actual native AVIO ENOSPC/EIO callbacks at MP4 finalization, FD cleanup and healthy replay; not full CLI/header faults",
         directory_variable="ASCIIFLOW_D1A_NATIVE_WRITE_EVIDENCE_DIR", receipt="native-write.json"),
    Gate("native-mux-cancellation", "asciiflow-media",
         "ffmpeg::encoder::mux_replay::deterministic_merge_cancellation_releases_fds_and_packets",
         "Native mux waiting-head, audio enqueue backpressure and buffered-interleaver cancellation; three scenarios, not 20 phase-targeted CLI cancellations",
         features="mux-qualification", directory_variable="ASCIIFLOW_MUX_REPLAY_DIRECTORY"),
    Gate("pq-output-fault-recovery", "asciiflow-interop",
         "pq_host_output_faults_preserve_cause_and_recover_fds",
         "SimulatedOnly checkpoints around real PQ image/import/submit/completed-fence operations; fresh surface parity and FD recovery",
         target="pq_hardware", features="hdr-pq-qualification", hardware=True),
    Gate("pq-mux-fault-recovery", "asciiflow-media",
         "ffmpeg::encoder::audio_regression_tests::pq_mux_header_packet_and_trailer_failures_preserve_cause_and_fds",
         "SimulatedOnly PQ native encoder header/early/mid packet/trailer fault checkpoints and FD recovery",
         hardware=True),
    Gate("pq-cli-sigint-followup", "asciiflow-cli",
         "pq_sigint_preserves_destination_and_fresh_followup_initializes",
         "Packet-writing-state SIGINT for HEVC/AV1 and fresh child-process followup; not proof of same-process GPU cleanup or 20 cancellation phases",
         target="pq_production", hardware=True),
    Gate("cli-repeated-jobs", "asciiflow-cli",
         "repeated_jobs_and_output_failures_preserve_transactions_and_recover_exactly",
         "100 CPU/software successes and 50 expected staging/rename/native EFBIG failures, immediate exact recovery and preserved sentinels",
         target="d1a_reliability", directory_variable="ASCIIFLOW_D1A_EVIDENCE_DIR", receipt="repeated-jobs.json"),
    Gate("native-million-packet-replay", "asciiflow-media",
         "ffmpeg::encoder::mux_replay::d1a_million_packet_native_mux_replay",
         "Two-cycle preflight then at least one million native mux packets with exact counts/PTS/DTS/payload/duration and FD restoration; independent per-stream cycles, not synchronized A/V looping or full-pipeline performance",
         features="mux-qualification", directory_variable="ASCIIFLOW_D1A_MILLION_PACKET_DIR", receipt="million-packet.json"),
)


def command(gate):
    argv = ["cargo", "test", "--release", "-p", gate.package]
    argv += ["--lib"] if gate.target == "lib" else ["--test", gate.target]
    if gate.features:
        argv += ["--features", gate.features]
    argv += [gate.selector, "--"]
    if gate.ignored:
        argv += ["--ignored"]
    return argv + ["--exact", "--test-threads=1", "--nocapture"]


def assert_one_test(text):
    """Cargo succeeding with a stale selector is not evidence of a passed test."""
    assert re.search(r"(?m)^running 1 test$", text), "selected test did not run exactly once"
    summaries = re.findall(r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;", text)
    assert summaries == [("1", "0", "0")], f"unexpected selected-test summary: {summaries}"


def canonical_identity(directory):
    # Always trust the committed source manifest, not a manifest supplied beside
    # alternate media. Full native pixel verification remains a separate gate.
    manifest = ROOT / "tests/fixtures/codecs/pq-canonical-v1-identity.json"
    identity = json.loads(manifest.read_text())
    checked = []
    for entry in identity["encoding"]["outputs"]:
        path = directory / entry["file"]
        assert path.stat().st_size == entry["bytes"], f"canonical fixture size differs: {path}"
        assert digest(path) == entry["sha256"], f"canonical fixture hash differs: {path}"
        checked.append(dict(path=str(path), sha256=entry["sha256"]))
    return dict(manifest_sha256=digest(manifest), files=checked)


def run_gate(run, gate, canonical):
    env = dict(os.environ)
    # Campaign artifacts have their own create-new directories. An inherited
    # measurement destination must not collide across these child conversions.
    env.pop("ASCIIFLOW_RELIABILITY_REPORT", None)
    env["ASCIIFLOW_PQ_CANONICAL_DIR"] = str(canonical)
    env["ASCIIFLOW_MUX_REPLAY_SOURCE"] = str(ROOT / "tests/fixtures/media/multiple.mp4")
    if gate.hardware:
        env.update(ASCIIFLOW_VULKAN_VALIDATION="1", ASCIIFLOW_REQUIRE_VULKAN_VALIDATION="1",
                   VK_INSTANCE_LAYERS="VK_LAYER_KHRONOS_validation")
    artifacts = run.out / gate.name
    if gate.directory_variable:
        env[gate.directory_variable] = str(artifacts)
    fixture_identity = canonical_identity(canonical) if gate.name == "pq-cli-sigint-followup" else None
    run.checked(gate.name, command(gate), env)
    log = run.out / run.commands[-1]["log"]
    text = log.read_text()
    assert_one_test(text)
    assert not any(token in text for token in ("Validation Error", "VUID-", "SYNC-HAZARD")), "Vulkan Validation diagnostic in test log"
    evidence = dict(scope=gate.scope, selected_test=gate.selector, log=log.name,
                    log_sha256=digest(log), validation_environment_requested=gate.hardware,
                    canonical_identity=fixture_identity)
    if gate.receipt:
        receipt = artifacts / gate.receipt
        document = json.loads(receipt.read_text())
        # Retain verbose child receipts separately; the campaign index carries
        # their identity and compact counters, not 150 copies of codec logs.
        summary = {key: value for key, value in document.items() if key != "records"}
        records = document.get("records", [])
        if len(records) <= 3:
            summary["records"] = records
        else:
            summary["record_count"] = len(records)
        evidence["receipt"] = dict(path=str(receipt), sha256=digest(receipt),
                                   summary=summary)
    return evidence


def completion_status(status, results):
    if status:
        return status
    return 0 if all(result["result"] == "PASS" for result in results) else 2


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--canonical-dir", type=Path,
                        default=Path(os.environ.get("ASCIIFLOW_PQ_CANONICAL_DIR", ROOT / "tests/fixtures/codecs")))
    args = parser.parse_args()
    run = Run(SimpleNamespace(output=args.output, binary=ROOT / "target/release/asciiflow",
              device="/dev/dri/renderD128", watchdog_seconds=1200, mode="d1a-bounded-failure-campaign", stack=None))
    run.environment()
    for gate in GATES:
        run.gate(gate.name, "hardware" if gate.hardware else "runtime",
                 lambda gate=gate: run_gate(run, gate, args.canonical_dir.resolve()), gate.hardware)
    save(run.out / "campaign-scope.json", dict(schema_version=1,
         qualification="BoundedFailureCoverageOnly",
         gates=[dict(id=gate.name, scope=gate.scope, hardware=gate.hardware) for gate in GATES],
         remaining=["20 cancellation trials at every native lifecycle phase",
                    "full CLI ENOSPC/EIO at header/packet/flush/trailer",
                    "same-process mixed-job GPU state contamination",
                    "100k resource and Validation campaigns"]))
    status = run.finish()
    return completion_status(status, run.results)


if __name__ == "__main__":
    raise SystemExit(main())
