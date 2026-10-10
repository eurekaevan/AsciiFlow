#!/usr/bin/env python3
"""Run a narrow D1A media/fault subset on two already-captured stacks.

This runner does not capture stacks or edit corpus/oracle policy. Capture both
stack manifests only after source freeze, against the same binary and tree.
"""
import argparse
import importlib.util
import json
from pathlib import Path
import subprocess
import sys

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tests/corpus"))
from portability import activate, digest
from run import Run, load, save

CORE_FIXTURES = {"canonical-h2648", "hevc-pq10", "hevc-pq-to-sdr10"}
REAL_FIXTURES = {
    "real-aac-48000-stereo-mp4-hardware",
    "real-dual-aac-mp4-hardware",
    "real-h264-bframes-2-mp4-hardware",
    "real-pq-no-static-mp4",
    "real-pq-static-mp4",
    "real-pq-static-64x64-mp4",
}
FAULT_GATES = {"native-avio-write-faults", "native-mux-cancellation",
               "pq-mux-fault-recovery"}


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


def selected_manifest(path, identifiers, output):
    source = load(path)
    fixtures = [fixture for fixture in source["fixtures"] if fixture["id"] in identifiers]
    found = {fixture["id"] for fixture in fixtures}
    if found != identifiers:
        raise ValueError(f"manifest fixture selection mismatch: missing {sorted(identifiers - found)}")
    save(output, {key: value for key, value in source.items() if key != "fixtures"} |
         {"fixtures": fixtures})


def run_corpus(run, stack, manifest, binary, inputs, output, reference=None):
    argv = [sys.executable, "-B", "tests/corpus/run.py", "quick",
            "--stack", str(stack), "--manifest", str(manifest),
            "--binary", str(binary), "--generated-inputs", str(inputs),
            "--output", str(output)]
    if reference:
        argv += ["--reference-run", str(reference)]
    run.checked(f"corpus-{stack.stem}", argv, timeout=3600)
    return load(output / "results.json")


def run_faults(run, campaign, binary, canonical_dir):
    selected = [gate for gate in campaign.GATES if gate.name in FAULT_GATES]
    if {gate.name for gate in selected} != FAULT_GATES:
        raise ValueError("failure campaign no longer contains the exact selected native gates")
    results = []
    for gate in selected:
        results.append({"id": gate.name, "scope": gate.scope,
                        "evidence": campaign.run_gate(run, gate, canonical_dir)})
    return results


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/release/asciiflow")
    parser.add_argument("--inputs", type=Path, default=ROOT / "target/portability-inputs")
    parser.add_argument("--canonical-stack", type=Path, required=True)
    parser.add_argument("--alternate-stack", type=Path, required=True)
    parser.add_argument("--pq-canonical-dir", type=Path, default=ROOT / "tests/fixtures/codecs")
    args = parser.parse_args()

    binary = args.binary.resolve(strict=True)
    canonical_manifest = args.canonical_stack.resolve(strict=True)
    alternate_manifest = args.alternate_stack.resolve(strict=True)
    inputs = args.inputs.resolve(strict=True)
    campaign = module("d1a_failure_campaign", ROOT / "tests/soak/failure-campaign.py")
    run = Run(type("Args", (), {"output": args.output, "binary": binary,
              "device": "/dev/dri/renderD128", "watchdog_seconds": 3600,
              "mode": "native-portability-subset", "stack": None})())
    run.environment()

    core_path = run.out / "core-subset.json"
    real_path = run.out / "real-media-subset.json"
    selected_manifest(ROOT / "tests/portability/core-set.json", CORE_FIXTURES, core_path)
    selected_manifest(ROOT / "tests/corpus/real-media-v1.json", REAL_FIXTURES, real_path)

    # `activate` is fail-closed and verifies source, binary, loaded libraries,
    # tools, native dependencies, and the actually initialized Intel drivers.
    canonical = activate(canonical_manifest, binary)
    canonical_dir = args.pq_canonical_dir.resolve(strict=True)
    canonical_core = run_corpus(run, canonical_manifest, core_path, binary, inputs,
                                run.out / "canonical-core")
    canonical_real = run_corpus(run, canonical_manifest, real_path, binary, inputs,
                                run.out / "canonical-audio")
    canonical_faults = run_faults(run, campaign, binary, canonical_dir)
    save(run.out / "canonical-stack-identity.json", canonical)

    alternate = activate(alternate_manifest, binary)
    if canonical["source"] != alternate["source"] or canonical["binary"] != alternate["binary"]:
        raise ValueError("stack pair does not share the exact source and binary identity")
    alternate_core = run_corpus(run, alternate_manifest, core_path, binary, inputs,
                                run.out / "alternate-core", run.out / "canonical-core")
    alternate_real = run_corpus(run, alternate_manifest, real_path, binary, inputs,
                                run.out / "alternate-audio", run.out / "canonical-audio")
    alternate_faults = run_faults(run, campaign, binary, canonical_dir)
    save(run.out / "alternate-stack-identity.json", alternate)

    save(run.out / "subset-results.json", {
        "schema_version": 1,
        "qualification": "BoundedCompatibilityEvidenceOnly",
        "binary_sha256": digest(binary),
        "source_identity": canonical["source"],
        "fixtures": sorted(CORE_FIXTURES | REAL_FIXTURES),
        "canonical": {"core": canonical_core, "audio": canonical_real,
                      "native_fault_gates": canonical_faults},
        "alternate": {"core": alternate_core, "audio": alternate_real,
                       "native_fault_gates": alternate_faults},
        "limitations": ["No broad codec/container claim", "No corpus/oracle policy changes",
                        "Native fault gates are their declared simulated/bounded scopes",
                        "Not a seal or full retained-regression run"]})
    return run.finish()


if __name__ == "__main__":
    raise SystemExit(main())
