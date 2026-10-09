#!/usr/bin/env python3
"""Default-allocator finite-workload observations. Never seals automatically."""
import argparse
from collections import Counter
import json
import os
from pathlib import Path
import sys

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "corpus"))
from run import Run, digest, load, save
from portability import source_identity

TEST = "native_reliability_tests::memory_plateau::default_memory_plateau"


def records(path):
    with path.open(encoding="utf-8") as stream:
        for line in stream:
            yield json.loads(line)


def summarize(directory):
    result = load(directory / "plateau-result.json")
    samples = list(records(directory / "memory.jsonl"))
    cycles = [s for s in samples if s["boundary"] == "cycle-end"]
    assert [s["cycle"] for s in cycles] == list(range(1, result["cycles"] + 1))
    assert 200 <= result["cycles"] <= 500
    if result["extended"]:
        assert result["cycles"] == 500
    baseline = samples[0]
    assert all(s["fd_count"] == baseline["fd_count"] and
               s["thread_count"] == baseline["thread_count"] for s in samples)
    assert result["reference_configurations"] == 4
    assert result["retained_reference_bytes"] == 928880
    outcomes = Counter()
    configurations = {}
    for job in records(directory / "jobs.jsonl"):
        outcomes[job["outcome"]] += 1
        assert job["post_cleanup_resources_zero"]
        assert job["job_entry_returned_after_worker_join_boundary"]
        assert job["fd_count_before_session"] == job["fd_count_after_session"] == baseline["fd_count"]
        assert job["thread_count_before"] == job["thread_count_after"] == baseline["thread_count"]
        resources = job["resource_samples"][-1]["resources"]
        assert resources and all(v["active_count"] == 0 and v["active_bytes"] in (None, 0)
                                 for v in resources.values())
        assert resources["vulkan_buffer_bindings"]["peak_count"] > 0
        if job["outcome"] == "Success":
            assert job["byte_exact_with_first_output_for_configuration"]
            assert job["oracle"]["decoded_frames"] == 3
            identity = {k: job[k] for k in ("input_sha256", "output_sha256", "oracle", "plan")}
            if job["case"] not in configurations:
                configurations[job["case"]] = identity
            assert configurations[job["case"]] == identity
    assert sum(outcomes.values()) == result["jobs"]
    assert outcomes["Success"] == result["cycles"] * 4
    assert len(configurations) == 4
    events = list(records(directory / "hwm.jsonl"))
    high_water = 0
    last_cycle = 0
    for sample in samples:
        if sample["private_anonymous_kib"] > high_water:
            high_water = sample["private_anonymous_kib"]
            last_cycle = sample["cycle"]
        assert sample["last_hwm_cycle"] == last_cycle
        completed = sample["cycle"] if sample["boundary"] == "cycle-end" else max(0, sample["cycle"] - 1)
        assert sample["complete_cycles_without_new_hwm"] == max(0, completed - last_cycle)
    assert result["last_hwm_cycle"] == last_cycle
    assert result["observed_plateau"] == (result["cycles"] - last_cycle >= 100)
    result.update({"job_outcomes": dict(outcomes), "configurations": configurations,
                   "cycle_records": len(cycles), "job_boundary_records": len(samples) - len(cycles) - 1,
                   "hwm_events": len(events), "all_lifecycle_checks_pass": True,
                   "initial": baseline, "final": cycles[-1],
                   "late_100_cycle_live_range_bytes": [min(s["allocator"]["in_use_including_mmap_bytes"] for s in cycles[-100:]),
                                                       max(s["allocator"]["in_use_including_mmap_bytes"] for s in cycles[-100:])],
                   "raw_sha256": {name: digest(directory / name) for name in
                                  ("jobs.jsonl", "memory.jsonl", "hwm.jsonl", "plateau-result.json")}})
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True, help="Release native-reliability test executable")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--device", default="/dev/dri/renderD128")
    args = parser.parse_args()
    args.watchdog_seconds = 3600
    args.stack = None
    run = Run(args)
    if run.skip_hardware:
        raise RuntimeError(run.skip_hardware)
    run.environment()
    before = source_identity()
    save(run.out / "source-before.json", before)
    protocol = {"processes": 10, "minimum_cycles": 200, "maximum_cycles": 500,
                "no_new_hwm_complete_cycles": 100, "forced_500_process": "process-00",
                "selection": "All ten consecutive fresh processes retained; no result-dependent replacement runs",
                "fault_schedule": "Repeat original50-cycle macro: cancel at10/30, injected encoder failure+cancel20/40, none at50",
                "policy": "Default allocator and Release; no validation layers, tuning, trim or instrumented allocator",
                "metric": "smaps_rollup.Anonymous conservative resident-anonymous HWM, raw shared/private VMA fields retained; not inherently exclusive private-page census",
                "decision": "ObservationOnly; all-process plateau plus allocator/live/mapping review required",
                "historical_cycle48_652kib": "Unresolved retained; no historical full smaps/allocator snapshot exists"}
    save(run.out / "protocol-before-runs.json", protocol)
    results = []
    try:
        for index in range(10):
            label = f"process-{index:02d}"
            directory = run.out / label
            env = dict(os.environ, ASCIIFLOW_D1A_NATIVE_GPU="1", ASCIIFLOW_C4B_PRODUCTION="1",
                       ASCIIFLOW_MEMORY_PLATEAU_MODE="extended-500" if index == 0 else "adaptive",
                       ASCIIFLOW_NATIVE_RELIABILITY_EVIDENCE_DIR=str(directory), ASCIIFLOW_ATTEST_NATIVE_MAPS="1")
            for name in ("ASCIIFLOW_VULKAN_VALIDATION", "ASCIIFLOW_REQUIRE_VULKAN_VALIDATION", "VK_INSTANCE_LAYERS"):
                env.pop(name, None)
            # The normal CLI streams stdout. libtest's default capture retains
            # every job's printed summary until test exit and is not production
            # memory behavior. Stream it to Run's on-disk command log instead.
            run.checked(label, [str(run.binary), "--ignored", "--exact", TEST, "--test-threads=1", "--nocapture"], env=env)
            result = summarize(directory)
            if results:
                assert result["configurations"] == results[0]["configurations"]
            results.append({"id": label, **result})
            save(run.out / f"{label}-checkpoint.json", result)
            print(f"{label}: cycles={result['cycles']} last_hwm={result['last_hwm_cycle']} plateau={result['observed_plateau']}", flush=True)
        save(run.out / "observations.json", {"protocol": protocol, "processes": results,
             "all_observed_plateau": all(p["observed_plateau"] for p in results), "classification": "PendingReviewNotAnAutomaticPass"})
    finally:
        after = source_identity()
        save(run.out / "source-after.json", after)
        save(run.out / "execution.json", {"commands": run.commands, "source_unchanged": before == after,
             "processes_completed": len(results), "binary_sha256": digest(run.binary)})
    if before != after:
        raise ValueError("source identity changed during qualification")


if __name__ == "__main__":
    main()
