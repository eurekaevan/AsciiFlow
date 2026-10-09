#!/usr/bin/env python3
"""Collect the two bounded D1A closure investigations, without declaring a seal.

No allocator tuning, baseline promotion, corpus edits, or generic SEI exemption.
The RSS observer streams job records; encoder capture is qualification-only.
"""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import shutil
import sys

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tests/corpus"))
from portability import digest, source_identity
from run import Run, load, save

H264_CASES = (
    "real-aac-48000-stereo-mp4-hardware",
    "real-dual-aac-mp4-hardware",
    "real-h264-bframes-2-mp4-hardware",
)


def module(path):
    spec = importlib.util.spec_from_file_location("h264_capture_helpers", path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


def capture_record(directory):
    """Validate actual captured geometry, not the old 1080p fixture shape."""
    rows = [json.loads(line) for line in (directory / "frames.jsonl").read_text().splitlines()]
    if not rows or [row["index"] for row in rows] != list(range(len(rows))):
        raise ValueError("missing or unordered encoder surfaces")
    geometry = rows[0]["width"], rows[0]["height"]
    if any((row["width"], row["height"]) != geometry or row["software_format"] != 23
           for row in rows):
        raise ValueError("changed geometry or non-NV12 encoder input")
    raw = directory / "frames.nv12"
    expected = geometry[0] * geometry[1] * 3 // 2 * len(rows)
    if raw.stat().st_size != expected:
        raise ValueError("captured NV12 plane length mismatch")
    return {
        "frames": rows, "raw_nv12_sha256": digest(raw), "raw_nv12_bytes": expected,
        "contexts": {phase: load(directory / f"context-{phase}.json")
                     for phase in ("before", "after")},
        "avoptions": {phase: (directory / f"avoptions-{phase}.txt").read_text().strip()
                      for phase in ("before", "after")},
    }


def rss(run, test_binary):
    binary = run.out / "rss-observer-test"
    shutil.copy2(test_binary, binary)
    argv = [str(binary), "--ignored", "--exact",
            "native_reliability_tests::default_allocator_fixed_mixed_cycles_observation",
            "--test-threads=1"]
    records = []
    for label, mode in [("cycles-50", "repeated-cycles")] + [
            (f"fresh-{index:02d}", "fresh-process-control") for index in range(10)]:
        directory = run.out / label
        env = dict(os.environ, ASCIIFLOW_D1A_NATIVE_GPU="1", ASCIIFLOW_C4B_PRODUCTION="1",
                   ASCIIFLOW_NATIVE_RSS_MODE=mode,
                   ASCIIFLOW_NATIVE_RELIABILITY_EVIDENCE_DIR=str(directory),
                   ASCIIFLOW_ATTEST_NATIVE_MAPS="1",
                   ASCIIFLOW_VULKAN_VALIDATION="1", ASCIIFLOW_REQUIRE_VULKAN_VALIDATION="1",
                   VK_INSTANCE_LAYERS="VK_LAYER_KHRONOS_validation")
        run.checked(label, argv, env=env, timeout=900)
        data = load(directory / "rss-cycles.json")
        jobs = [json.loads(line) for line in (directory / "jobs.jsonl").read_text().splitlines()]
        if len(jobs) != data["jobs"]:
            raise ValueError("job record count differs from executed count")
        for job in jobs:
            resources = job["resource_samples"][-1]["resources"]
            if not resources or any(row["active_count"] != 0 for row in resources.values()):
                raise ValueError("unobserved or outstanding job-owned resources")
            if (job["fd_count_before_session"] != job["fd_count_after_session"]
                    or job["thread_count_before"] != job["thread_count_after"]):
                raise ValueError("job FD/thread baseline changed")
        records.append({"id": label, "data": data,
                        "jobs_sha256": digest(directory / "jobs.jsonl")})
        save(run.out / f"rss-observations-{label}.json", {
            "classification": "UnresolvedPendingAnalysis", "test_binary_sha256": digest(binary),
            "runs": records,
        })
    save(run.out / "rss-observations.json", {
        "classification": "UnresolvedPendingAnalysis", "test_binary_sha256": digest(binary),
        "runs": records,
    })


def h264(run, historical, alternate_driver):
    helpers = module(ROOT / "tests/portability/h264-ihd-forensic.py")
    old = load(historical / "canonical-audio/results.json")
    commands = {row["id"]: row["argv"] for row in old["commands"]}
    drivers = {
        "canonical": "/usr/lib64/dri-nonfree",
        "alternate": str(alternate_driver.resolve(strict=True)),
    }
    captures = {}
    for stack, driver in drivers.items():
        for case in H264_CASES:
            directory = run.out / stack / case
            directory.mkdir(parents=True)
            capture = directory / "capture"
            argv = list(commands["runtime-" + case])
            argv[0], argv[2] = str(run.binary), str(directory / "output.mp4")
            argv[argv.index("--diagnostic-report") + 1] = str(directory / "diagnostic.json")
            env = dict(os.environ, LIBVA_DRIVER_NAME="iHD", LIBVA_DRIVERS_PATH=driver,
                       VK_ICD_FILENAMES="/usr/share/vulkan/icd.d/intel_icd.x86_64.json",
                       ASCIIFLOW_ENCODER_CAPTURE_DIRECTORY=str(capture),
                       ASCIIFLOW_VULKAN_VALIDATION="1", ASCIIFLOW_REQUIRE_VULKAN_VALIDATION="1",
                       VK_INSTANCE_LAYERS="VK_LAYER_KHRONOS_validation", LD_DEBUG="libs")
            run.checked(stack + "-" + case, argv, env=env, timeout=300)
            captures[stack, case] = capture_record(capture)
            captures[stack, case]["loaded_driver_files"] = helpers.modules_from_trace(
                run.out / run.commands[-1]["log"])
    comparisons = {}
    for case in H264_CASES:
        comparison = helpers.compare_captures(captures["canonical", case], captures["alternate", case])
        comparisons[case] = comparison
        save(run.out / f"pre-encode-{case}.json", comparison)
        if not (comparison["frames_metadata_equal"] and comparison["raw_nv12_sha256_equal"]
                and all(comparison[f"context_{phase}"]["equal"]
                        and comparison[f"avoptions_{phase}"]["raw_equal"]
                        for phase in ("before", "after"))):
            raise ValueError(f"{case}: encoder input/context/options identity failed")
    save(run.out / "pre-encode-comparison.json", comparisons)
    save(run.out / "encoder-inputs.json", {
        stack: {case: captures[stack, case] for case in H264_CASES} for stack in drivers
    })


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--test-binary", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--historical-root", type=Path)
    parser.add_argument("--alternate-driver", type=Path)
    parser.add_argument("--mode", choices=("rss", "h264"), required=True)
    args = parser.parse_args()
    run = Run(type("Args", (), {"output": args.output, "binary": args.binary,
              "device": "/dev/dri/renderD128", "watchdog_seconds": 900,
              "mode": "d1a-ab-" + args.mode, "stack": None})())
    run.environment()
    before = source_identity()
    save(run.out / "source-before.json", before)
    try:
        if args.mode == "rss":
            if not args.test_binary:
                parser.error("--test-binary required for rss")
            rss(run, args.test_binary.resolve(strict=True))
        else:
            if not args.historical_root or not args.alternate_driver:
                parser.error("--historical-root and --alternate-driver required for h264")
            h264(run, args.historical_root.resolve(strict=True), args.alternate_driver)
    finally:
        after = source_identity()
        save(run.out / "source-after.json", after)
        save(run.out / "execution.json", {"commands": run.commands,
             "source_unchanged": before == after, "qualification": "EvidenceOnlyNotASeal"})
    if before != after:
        raise ValueError("source identity changed during campaign")


if __name__ == "__main__":
    main()
