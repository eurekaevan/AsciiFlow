#!/usr/bin/env python3
"""Deterministic mux gates, using the existing corpus and retained oracles unchanged."""
import argparse
import json
import os
from pathlib import Path
import sys
from types import SimpleNamespace

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "corpus"))
from run import Run
from portability import digest, source_identity


def capture_manifest(run, stack_id, name, prefix=None):
    env = dict(os.environ)
    argv = ["python3", "-B", "tests/portability/stack.py", "capture", "--stack-id", stack_id, "--binary", str(run.binary)]
    if prefix:
        prefix = prefix.resolve(strict=True)
        env["PATH"] = str(prefix / "bin") + os.pathsep + env["PATH"]
        env["LD_LIBRARY_PATH"] = str(prefix / "lib")
        argv += ["--prefix", str(prefix)]
    # A fresh process is essential: already loaded SONAMEs must not leak from
    # the canonical ctypes probe into the alternate capture.
    run.checked("capture-" + name, argv, env)
    stack = json.loads((run.out / run.commands[-1]["log"]).read_text())
    manifest = run.out / (name + "-stack.json")
    manifest.write_text(json.dumps(stack, indent=2) + "\n")
    return manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("gate", choices=("retained", "static", "lifecycle", "portability", "timing"))
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--binary", type=Path, default=Path("target/release/asciiflow"))
    parser.add_argument("--inputs", type=Path, default=Path("target/portability-inputs"))
    args = parser.parse_args()
    context = SimpleNamespace(output=args.output, binary=args.binary, device="/dev/dri/renderD128",
                              watchdog_seconds=1200, mode=args.gate, stack=None)
    run = Run(context)
    run.environment()
    evidence = json.loads(Path("tests/portability/baseline.json").read_text())
    source = source_identity()
    (run.out / "source.json").write_text(json.dumps(source, indent=2) + "\n")
    if args.gate == "retained":
        input8 = args.inputs.resolve() / "canonical-h2648.mp4"
        run.gate("all-retained-production", "hardware", lambda: run.retained(input8), True)
        run.gate("c1-cpu-reference", "runtime", lambda: run.checked("c1-cpu-reference", ["bash", "scripts/qualify-tone-map-cpu.sh", str(run.out / "c1")]))
        run.gate("c2b-cpu-reference", "runtime", lambda: run.checked("c2b-cpu-reference", ["bash", "scripts/qualify-target-volume-cpu.sh", str(run.out / "c1/method-a-run1.bin"), str(run.out / "c2b")]))
        env = dict(os.environ, ASCIIFLOW_VULKAN_VALIDATION="1", VK_INSTANCE_LAYERS="VK_LAYER_KHRONOS_validation", ASCIIFLOW_REQUIRE_VULKAN_VALIDATION="1",
                   C1_INPUT=str(run.out / "c1/linear-bt2020-1000-v1.bin"), C1_OUTPUT=str(run.out / "c1/method-a-run1.bin"),
                   C3_REPORT=str(run.out / "c3-report.json"), C4A_BOUNDARY_REPORT=str(run.out / "c4a-report.json"))
        (run.out / "qualification-environment.json").write_text(json.dumps({k: env[k] for k in ("ASCIIFLOW_VULKAN_VALIDATION", "VK_INSTANCE_LAYERS", "ASCIIFLOW_REQUIRE_VULKAN_VALIDATION", "C1_INPUT", "C1_OUTPUT", "C3_REPORT", "C4A_BOUNDARY_REPORT")}, indent=2) + "\n")
        for command in evidence["canonical_retained"]["commands"]:
            if command["id"] in {"c3-retained-reference", "c4a-retained-reference"}:
                run.gate(command["id"], "hardware", lambda c=command: run.checked(c["id"], c["argv"], env), True)
    elif args.gate == "static":
        commands = evidence["static"]["commands"] + [
            {"id": "mux-qualification-tests", "argv": ["cargo", "test", "--workspace", "--features", "asciiflow-cli/mux-qualification"]},
            {"id": "mux-qualification-clippy", "argv": ["cargo", "clippy", "--workspace", "--all-targets", "--features", "asciiflow-cli/mux-qualification", "--", "-D", "warnings"]}]
        for command in commands:
            run.gate(command["id"], "static", lambda c=command: run.checked(c["id"], c["argv"]))
    elif args.gate == "lifecycle":
        env = dict(os.environ, ASCIIFLOW_VULKAN_VALIDATION="1", VK_INSTANCE_LAYERS="VK_LAYER_KHRONOS_validation", ASCIIFLOW_REQUIRE_VULKAN_VALIDATION="1",
                   ASCIIFLOW_TEST_VIDEO=str(args.inputs.resolve() / "canonical-h2648.mp4"), ASCIIFLOW_VAAPI_DEVICE="/dev/dri/renderD128")
        (run.out / "qualification-environment.json").write_text(json.dumps({k: env[k] for k in ("ASCIIFLOW_VULKAN_VALIDATION", "VK_INSTANCE_LAYERS", "ASCIIFLOW_REQUIRE_VULKAN_VALIDATION", "ASCIIFLOW_TEST_VIDEO", "ASCIIFLOW_VAAPI_DEVICE")}, indent=2) + "\n")
        commands = evidence["native_lifecycle"]["commands"] + [
            {"id": "sdr-native-mux-faults", "argv": ["cargo", "test", "--release", "-p", "asciiflow-media", "--lib", "sdr_hardware_output_matrix_encode_and_mux_failures_preserve_roots", "--", "--ignored", "--test-threads=1", "--nocapture"]},
            {"id": "audio-hardware-validation-cancel", "argv": ["cargo", "test", "--release", "-p", "asciiflow-cli", "--test", "audio_regression", "intel_audio_parity_validation_and_cancellation", "--", "--ignored", "--exact", "--test-threads=1", "--nocapture"]},
            {"id": "audio-longer-stress", "argv": ["cargo", "test", "--release", "-p", "asciiflow-cli", "--test", "audio_regression", "long_audio_stream_completes_without_deadlock_or_packet_loss", "--", "--ignored", "--exact", "--nocapture"]}]
        for command in commands:
            run.gate(command["id"], "hardware", lambda c=command: run.checked(c["id"], c["argv"], env), True)
    elif args.gate == "portability":
        for name, prefix in [("canonical", None), ("alternate", Path("target/portability-toolchains/ffmpeg-8.1.2-prefix"))]:
            stack_id = json.loads(Path(f"tests/portability/stacks/{'canonical' if name == 'canonical' else 'alternate-ffmpeg'}.json").read_text())["stack_id"]
            manifest = capture_manifest(run, stack_id, name, prefix)
            argv = ["python3", "-B", "tests/corpus/run.py", "quick", "--stack", str(manifest), "--manifest", "tests/portability/core-set.json",
                    "--binary", str(run.binary), "--generated-inputs", str(args.inputs.resolve()), "--output", str(run.out / name)]
            if name == "alternate": argv += ["--reference-run", str(run.out / "canonical")]
            run.gate(name + "-core-24", "hardware", lambda a=argv,n=name: run.checked(n + "-core-24", a), True)
    else:
        manifest = json.loads(Path("tests/corpus/real-media-v1.json").read_text())
        ids = {"real-hevc-bframes-2-mp4", "real-h264-bframes-4-mp4", "real-mp4-fragmented-mp4", "real-fragmented-aac-mp4", "real-cfr-24000-1001-mp4"}
        manifest["fixtures"] = [f for f in manifest["fixtures"] if f["id"] in ids or "pts" in f["id"]]
        selected = run.out / "timing-manifest.json"
        selected.write_text(json.dumps(manifest, indent=2) + "\n")
        # --stack selects the established real-media execution surface.
        stack = capture_manifest(run, "fedora44-ffmpeg8.1.3-anv26.2.3-ihd26.1.5", "timing")
        run.gate("additional-timing-audio", "hardware", lambda: run.checked("additional-timing-audio", ["python3", "-B", "tests/corpus/run.py", "quick", "--stack", str(stack), "--manifest", str(selected), "--binary", str(run.binary), "--generated-inputs", str(args.inputs.resolve()), "--output", str(run.out / "cases")]), True)
    assert source_identity() == source, "source changed during qualification; rerun the gate"
    return run.finish()


if __name__ == "__main__":
    raise SystemExit(main())
