#!/usr/bin/env python3
"""Canonical-centered C-2 orchestration; existing corpus/oracles stay authoritative.

Every invocation owns a new evidence directory. Source changes invalidate the
whole invocation. A successful subset is not qualification or a seal decision.
"""
import argparse
import json
import os
from pathlib import Path
from statistics import median
import sys
from types import SimpleNamespace

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "corpus"))
from portability import capability_diff, digest, source_identity
from run import ROOT, Run, load, save
from stack_environment import environment

VALIDATION = {"ASCIIFLOW_VULKAN_VALIDATION": "1",
              "ASCIIFLOW_REQUIRE_VULKAN_VALIDATION": "1",
              "VK_INSTANCE_LAYERS": "VK_LAYER_KHRONOS_validation"}


def h264_10bit_rejection(run, directory, binary, inputs, env):
    destination = directory / "rejected-h26410.mp4"
    sentinel = b"C-2 policy rejection must preserve the destination.\n"
    destination.write_bytes(sentinel)
    report = directory / "rejected-h26410.json"
    success = run.command("h26410-policy-rejection", [binary, inputs / "canonical-h2648.mp4", destination,
                          "--output-codec", "h264", "--output-bit-depth", "10", "--no-progress",
                          "--diagnostic-report", report], env)
    diagnostic = load(report)
    assert not success
    assert diagnostic["failure"]["stage"] == "Planning"
    assert diagnostic["failure"]["category"] == "InvalidConfig"
    assert destination.read_bytes() == sentinel
    assert not list(directory.glob("*asciiflow-part*"))
    return {"stage": "Planning", "category": "InvalidConfig", "sentinel_preserved": True}


def selectors(matrix, stack):
    selected = dict(matrix["defaults"])
    for field, key in (("icd", "VK_ICD_FILENAMES"), ("driver_directory", "LIBVA_DRIVERS_PATH")):
        if field in stack:
            selected[key] = str((ROOT / stack[field]).resolve(strict=True))
    return selected


def attest_edge(reference, candidate, variable):
    for key in ("source", "kernel", "rustc", "cargo", "gpu", "render_node"):
        assert reference[key] == candidate[key], f"Star edge changed {key}"
    assert reference["binary"]["sha256"] == candidate["binary"]["sha256"]
    if variable != "ffmpeg":
        assert reference["libraries"] == candidate["libraries"], "Non-libav edge changed libav"
        for tool in ("ffmpeg", "ffprobe"):
            assert reference[tool] == candidate[tool], f"Non-libav edge changed {tool}"
    for driver in ("anv", "ihd"):
        if variable != driver:
            assert reference["driver_files"][driver] == candidate["driver_files"][driver], f"Edge changed undeclared {driver}"
    closures = [stack["native_dependencies"] for stack in (reference, candidate)]
    if variable in {"canonical", "ffmpeg"}:
        assert closures[0] == closures[1], "Non-graphics edge changed the graphics dependency closure"
    else:
        selected_paths = {s["driver_files"][variable]["path"] for s in (reference, candidate)}
        closures = [{p:r for p,r in closure.items() if p not in selected_paths} for closure in closures]
        for path in closures[0].keys() & closures[1].keys():
            assert closures[0][path] == closures[1][path], f"Graphics edge replaced an unrelated dependency: {path}"
    return {"major_variable": variable, "unchanged_components_attested": True,
            "selected_driver_dependency_delta": capability_diff(*closures)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--binary", type=Path, default=Path("target/release/asciiflow"))
    parser.add_argument("--inputs", type=Path, default=Path("target/stage54c1-evidence/inputs"))
    parser.add_argument("--gates", nargs="+", choices=("core", "mux", "hardware", "performance", "retained", "lifecycle", "static"),
                        default=["core", "mux", "hardware", "performance", "retained", "lifecycle", "static"])
    parser.add_argument("--repetitions", type=int, default=3)
    args = parser.parse_args()
    if args.repetitions < 3:
        parser.error("qualification requires at least three independent per-stack runs")
    run = Run(SimpleNamespace(output=args.output, binary=args.binary, device="/dev/dri/renderD128",
                              watchdog_seconds=2400, mode="expanded-portability", stack=None))
    matrix = load(ROOT / "tests/portability/c2-matrix.json")
    source = source_identity()
    save(run.out / "source.json", source)
    stacks = []
    performance = []
    canonical = None
    for entry in matrix["stacks"]:
        name = entry["id"]
        directory = run.out / name
        directory.mkdir()
        selection = selectors(matrix, entry)
        env = environment(entry.get("prefix"), selection)
        env.update(VALIDATION, ASCIIFLOW_ATTEST_NATIVE_MAPS="1")
        selected = directory / "selectors.json"
        save(selected, selection)
        capture = ["python3", "-B", "tests/portability/stack.py", "capture", "--stack-id", name,
                   "--binary", str(run.binary), "--selectors", str(selected),
                   "--provenance", "tests/portability/c2-provenance.json"]
        if entry.get("prefix"):
            capture += ["--prefix", entry["prefix"]]
        ready = run.gate(name + "-capture", "hardware", lambda: run.checked(name + "-capture", capture, env), True)
        row = {"stack_id": name, "variable": entry["variable"], "gates": [], "status": "SetupFailed"}
        stacks.append(row)
        if not ready:
            continue
        stack = json.loads((run.out / run.commands[-1]["log"]).read_text())
        manifest = directory / "stack.json"
        save(manifest, stack)
        row["manifest"] = str(manifest.relative_to(run.out))
        row["manifest_sha256"] = digest(manifest)
        row["status"] = "IncompleteQualification"
        if canonical is None:
            if name != matrix["canonical"]:
                row["reason"] = "Canonical setup failed; cross-stack qualification cannot run"
                continue
            canonical = stack
        if not run.gate(name + "-star-edge", "runtime", lambda: attest_edge(canonical, stack, entry["variable"])):
            row["status"] = "SetupFailed"
            row["reason"] = "An undeclared major component changed"
            continue
        save(directory / "capability-diff.json", {
            "vaapi_profiles": capability_diff(canonical["vaapi_profiles"], stack["vaapi_profiles"]),
            "vulkan_profiles": capability_diff(canonical["vulkan_profile"]["profiles"], stack["vulkan_profile"]["profiles"])})
        if "core" in args.gates:
            row["gates"].append(run.gate(name + "-h26410-rejection", "runtime",
                lambda: h264_10bit_rejection(run, directory, run.binary, args.inputs.resolve(), env)))
            for repeat in range(args.repetitions):
                out = directory / f"core-{repeat + 1}"
                command = ["python3", "-B", "tests/corpus/run.py", "quick", "--stack", str(manifest),
                           "--manifest", "tests/portability/core-set.json", "--binary", str(run.binary),
                           "--generated-inputs", str(args.inputs.resolve()), "--output", str(out)]
                reference = (directory / f"core-{repeat}") if repeat else run.out / matrix["canonical"] / "core-1"
                if repeat or name != matrix["canonical"]:
                    command += ["--reference-run", str(reference)]
                row["gates"].append(run.gate(f"{name}-core-{repeat + 1}", "hardware",
                    lambda c=command: run.checked("core", c, env), True))
        if "performance" in args.gates:
            core = directory / "core-1"
            if not (core / "results.json").is_file():
                row["gates"].append(False)
                row["performance_reason"] = "Requires a completed core run; no guessed command or frame count"
                run.results.append({"id": name + "-performance", "category": "hardware", "result": "FAILED", "reason": row["performance_reason"]})
            else:
                core_results = load(core / "results.json")
                commands = {c["id"]: c for c in core_results["commands"]}
                fixture_results = {r["id"]: r for r in core_results["results"] if r["category"] == "classification"}
                perf_env = dict(env)
                for flag in VALIDATION:
                    perf_env.pop(flag, None)
                for case in ("canonical-h2648", "hevc-pq10", "hevc-pq-to-sdr10"):
                    if fixture_results.get(case, {}).get("result") != "PASS":
                        row["gates"].append(False)
                        run.results.append({"id": name + "-performance-" + case, "category": "hardware", "result": "FAILED", "reason": "Core fixture did not pass; no performance inference"})
                        continue
                    durations = []
                    record = {"stack_id": name, "fixture": case, "validation": "off", "runs": []}
                    performance.append(record)
                    for repeat in range(3):
                        argv = list(commands["runtime-" + case]["argv"])
                        argv[0] = str(run.binary)
                        argv[2] = str(directory / f"performance-{case}-{repeat}.mp4")
                        report = directory / f"performance-{case}-{repeat}.json"
                        argv[argv.index("--diagnostic-report") + 1] = str(report)
                        passed = run.gate(name + f"-performance-{case}-{repeat}", "hardware",
                                          lambda a=argv: run.checked("performance", a, perf_env), True)
                        row["gates"].append(passed)
                        command = run.commands[-1]
                        record["runs"].append(command)
                        if passed:
                            diagnostic = load(report)
                            plan = diagnostic["selected_plan"]
                            assert diagnostic["plan_scope"] == "initialized_execution"
                            assert plan == load(core / f"{case}-runtime.json")["selected_plan"]
                            assert diagnostic["input_requirements"]["width"] == 1920
                            assert diagnostic["input_requirements"]["height"] == 1080
                            record.update(actual_plan=plan, slot_count=plan["buffer_capacity"],
                                          frames=fixture_results[case]["evidence"]["decode_back"]["frames"])
                            durations.append(command["elapsed_seconds"])
                    if len(durations) == 3:
                        record.update(median_elapsed_seconds=median(durations),
                                      median_end_to_end_fps=record["frames"] / median(durations))
        if "mux" in args.gates and entry["variable"] in {"canonical", "ffmpeg"}:
            captures = directory / "mux-captures"
            row["gates"].append(run.gate(name + "-mux-capture", "runtime", lambda: run.checked("mux-capture", [
                "python3", "-B", "tests/portability/mux-determinism.py", "materialize-captures", "--output", str(captures)], env)))
            replay = directory / "mux-replay"
            replay_env = dict(env, ASCIIFLOW_MUX_REPLAY_SOURCE=str(captures / "variant-a.mp4"),
                              ASCIIFLOW_MUX_REPLAY_DIRECTORY=str(replay))
            for test in ("deterministic_producer_merge_replay", "deterministic_merge_cancellation_releases_fds_and_packets"):
                test_env = dict(replay_env)
                if test == "deterministic_merge_cancellation_releases_fds_and_packets":
                    test_env["ASCIIFLOW_MUX_REPLAY_DIRECTORY"] = str(directory / "mux-cancellation")
                row["gates"].append(run.gate(name + "-" + test, "runtime", lambda t=test: run.checked(t, [
                    "cargo", "test", "--offline", "--release", "-p", "asciiflow-media", "--features", "mux-qualification",
                    "--lib", t, "--", "--ignored", "--test-threads=1", "--nocapture"], test_env)))
            for operation, output in (("replay-summary", "mux-summary.json"), ("strict-oracles", "mux-oracles.json")):
                row["gates"].append(run.gate(name + "-" + operation, "runtime", lambda op=operation,o=output: run.checked(op, [
                    "python3", "-B", "tests/portability/mux-determinism.py", op, "--directory", str(replay),
                    "--output", str(directory / o)], env)))
            if entry["variable"] == "ffmpeg":
                pair_env = dict(env, ASCIIFLOW_REGRESSION_REFERENCE=str(run.out / matrix["canonical"] / "mux-replay/fixed-000.mp4"),
                                ASCIIFLOW_REGRESSION_CANDIDATE=str(replay / "fixed-000.mp4"))
                pair_env.pop("ASCIIFLOW_REGRESSION_EXACT_BUILD", None)
                row["gates"].append(run.gate(name + "-cross-stack-fixed-mux", "runtime", lambda: run.checked("cross-stack-fixed-mux", [
                    "cargo", "test", "--offline", "--release", "-p", "asciiflow-media", "--test", "media_regression",
                    "compare_portability_pair_from_env", "--", "--ignored", "--exact", "--nocapture"], pair_env)))
        if "hardware" in args.gates:
            native = dict(env, ASCIIFLOW_TEST_VIDEO=str(args.inputs.resolve() / "canonical-h2648.mp4"),
                          ASCIIFLOW_DESCRIPTOR_REPORT=str(directory / "descriptors.json"),
                          ASCIIFLOW_C4B_STRESS="1", ASCIIFLOW_C4B_STRESS_DIR=str(directory / "production-stress"))
            for test, function in (("portability_descriptors", "actual_input_and_encoder_output_descriptors"),
                                   ("c4b_production_stress", "c4b_full_production_3000_frame_stress")):
                if test == "c4b_production_stress" and entry["variable"] not in {"canonical", "anv", "ihd"}:
                    continue  # Same graphics bytes; descriptor capture above still runs under each libav.
                row["gates"].append(run.gate(name + "-" + test, "hardware", lambda t=test,f=function: run.checked(t, [
                    "cargo", "test", "--offline", "--release", "-p", "asciiflow-interop", "--features", "hdr-to-sdr-production",
                    "--test", t, f, "--", "--ignored", "--exact", "--test-threads=1", "--nocapture"], native), True))
        if row["gates"] and not all(row["gates"]):
            row["status"] = "FailedQualification"
        assert source_identity() == source, "source changed; discard qualification and rerun all stacks"
        print(json.dumps({"stack_id": name, "status": row["status"]}), flush=True)
    canonical_env = environment(None, selectors(matrix, matrix["stacks"][0]))
    canonical_env.update(VALIDATION)
    for gate in ("retained", "lifecycle", "static"):
        if gate in args.gates:
            run.gate("canonical-" + gate, "static" if gate == "static" else "hardware", lambda g=gate: run.checked(g, [
                "python3", "-B", "tests/portability/run-mux-closure.py", g, "--binary", str(run.binary),
                "--inputs", str(args.inputs.resolve()), "--output", str(run.out / ("canonical-" + g))], canonical_env), gate != "static")
    assert source_identity() == source, "source changed during final checks"
    reference_perf = {r["fixture"]:r for r in performance if r["stack_id"] == matrix["canonical"]}
    for record in performance:
        reference = reference_perf.get(record["fixture"], {})
        if "median_elapsed_seconds" in record and "median_elapsed_seconds" in reference:
            ratio = record["median_elapsed_seconds"] / reference["median_elapsed_seconds"]
            record.update(elapsed_ratio_to_canonical=ratio, investigation_required=ratio > 2)
    save(run.out / "performance-table.json", {"scope": "serialized end-to-end conversion, including probe/init; three runs, validation off", "records": sorted(performance, key=lambda r:(r["stack_id"],r["fixture"]))})
    save(run.out / "matrix-summary.json", {"schema_version": 1, "source": source, "selected_gates": args.gates,
         "stacks": stacks, "kernel_edge": matrix["kernel_edge"], "status": "NOT SEALED",
         "seal_policy": "Explicit review additionally requires two clean builds/SPIR-V, performance-drift review and a resolved difference ledger; subset success is not qualification."})
    result = run.finish()
    return 1 if result or any(r["status"] in {"SetupFailed", "FailedQualification"} for r in stacks) else 0


if __name__ == "__main__":
    raise SystemExit(main())
