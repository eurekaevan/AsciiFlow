#!/usr/bin/env python3
"""Bounded H.264 VAAPI encoder capture across the two Stage 5.4C2 iHD builds.

This runner records evidence only. It does not alter the host environment, reuse
an output directory, or turn partial results into a qualification decision.
"""
import argparse
import json
import os
from pathlib import Path
import re
import signal
import shutil
import subprocess
import sys
import time

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tests/corpus"))
from portability import digest, identity, loaded_libraries, source_identity  # noqa: E402
from stack_environment import environment  # noqa: E402

DEVICE = "/dev/dri/renderD128"
CASES = ("canonical-h2648", "canonical-h2648-auto", "hevc-pq-to-sdr10")
DRIVERS = {
    "ihd-26.1.5-host": "/usr/lib64/dri-nonfree",
    "ihd-25.4.6-isolated": "target/stage54c2-toolchains/intel-media-driver-25.4.6-1.fc44/usr/lib64/dri-nonfree",
}
HISTORICAL_ORACLE_SHA256 = {
    "tests/baselines/media/verify-hdr-to-sdr-production.py":
        "af905b5284d0dbd193b5a5c728cd4c52dab137d190ae36efa7c94042c83f2332",
    "tests/baselines/media/verify-pq-production.py":
        "010d42ae84a10fce10bf50cbfb2aafbbd69bddbf7f57c6e1023040184abbf2c2",
}


def load(path):
    return json.loads(Path(path).read_text(encoding="utf-8"))


def save(path, value):
    with Path(path).open("x", encoding="utf-8") as stream:
        json.dump(value, stream, indent=2, sort_keys=True)
        stream.write("\n")


def log_event(path, value):
    with Path(path).open("a", encoding="utf-8") as stream:
        stream.write(json.dumps(value, sort_keys=True) + "\n")
        stream.flush()
        os.fsync(stream.fileno())


def query(argv, env=None):
    result = subprocess.run(argv, cwd=ROOT, env=env, text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                            timeout=60, check=False)
    return {"argv": [str(part) for part in argv], "exit_code": result.returncode,
            "stdout": result.stdout, "stderr": result.stderr}


def fixture_input(fixture, inputs):
    source = fixture["source"]
    if source["kind"] == "generated":
        path = inputs / Path(source["path"]).name
        generator = ROOT / source["generator"]
        if digest(generator) != source["generator_sha256"]:
            raise ValueError(f"{fixture['id']}: generated-input recipe changed")
    elif source["kind"] == "retained_binary":
        path = ROOT / source["path"]
    else:
        raise ValueError(f"{fixture['id']}: unsupported fixture source {source['kind']}")
    path = path.resolve(strict=True)
    if path.stat().st_size != source["byte_size"] or digest(path) != source["sha256"]:
        raise ValueError(f"{fixture['id']}: fixture identity mismatch at {path}")
    return path


def runtime_argv(binary, fixture, source, output, report):
    request = fixture["request"]
    argv = [str(binary), str(source), str(output), "--audio", "none",
            "--output-codec", request["codec"], "--output-bit-depth",
            str(request["bit_depth"]), "--output-dynamic-range",
            request["dynamic_range"], "--hw-device", DEVICE,
            "--width", "80", "--charset", "standard", "--font", "builtin-8x8",
            "--color", "true", "--max-frames", "0"]
    if request["route"] == "auto":
        argv += ["--backend", "auto", "--decode", "auto", "--encode", "auto",
                 "--vulkan-mapping", "gpu", "--vaapi-vulkan-input-interop", "auto",
                 "--vaapi-vulkan-output-interop", "auto"]
    else:
        argv += ["--backend", "vulkan", "--decode", "vaapi", "--encode", "vaapi",
                 "--vulkan-mapping", "gpu", "--vaapi-vulkan-input-interop", "on",
                 "--vaapi-vulkan-output-interop", "on"]
    return argv + ["--no-progress", "--diagnostic-report", str(report)]


def run_command(argv, env, directory, timeout, capture, trace_vaapi):
    directory.mkdir(parents=True, exist_ok=False)
    stdout_path, stderr_path = directory / "stdout.log", directory / "stderr.log"
    child_env = dict(env)
    if capture:
        child_env["LD_DEBUG"] = "libs"
        if trace_vaapi:
            child_env["LIBVA_TRACE"] = str(directory / "libva.trace")
    started = time.monotonic()
    timed_out = False
    with stdout_path.open("x", encoding="utf-8") as stdout, \
            stderr_path.open("x", encoding="utf-8") as stderr:
        process = subprocess.Popen(argv, cwd=ROOT, env=child_env,
                                   stdout=stdout, stderr=stderr,
                                   start_new_session=True)
        try:
            return_code = process.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            timed_out = True
            try:
                os.killpg(process.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                pass
            # The leader may exit on SIGTERM while a child remains alive.
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait()
            return_code = process.returncode
    record = {"argv": argv, "exit_code": return_code,
              "timed_out": timed_out,
              "elapsed_seconds": time.monotonic() - started,
              "stdout": {"path": str(stdout_path), "sha256": digest(stdout_path)},
              "stderr": {"path": str(stderr_path), "sha256": digest(stderr_path)},
              "capture_enabled": capture, "vaapi_trace_requested": bool(capture and trace_vaapi)}
    return record


def modules_from_trace(path):
    text = Path(path).read_text(encoding="utf-8", errors="replace")
    paths = set()
    for candidate in re.findall(r"calling init:\s*(/[^\n]+)", text):
        module = Path(candidate.strip())
        if module.is_file():
            paths.add(str(module.resolve(strict=True)))
    return {str(Path(path).name): identity(path) for path in sorted(paths)
            if Path(path).name in {"iHD_drv_video.so", "libvulkan_intel.so"}}


def avoptions(path):
    return Path(path).read_text(encoding="utf-8").strip()


def json_differences(left, right, prefix=""):
    differences = []
    if isinstance(left, dict) and isinstance(right, dict):
        for key in sorted(left.keys() | right.keys()):
            child = f"{prefix}.{key}" if prefix else key
            if key not in left or key not in right:
                differences.append({"field": child, "left": left.get(key), "right": right.get(key)})
            else:
                differences.extend(json_differences(left[key], right[key], child))
    elif isinstance(left, list) and isinstance(right, list):
        if len(left) != len(right):
            differences.append({"field": prefix + ".length", "left": len(left), "right": len(right)})
        for index, (a, b) in enumerate(zip(left, right)):
            differences.extend(json_differences(a, b, f"{prefix}[{index}]"))
    elif left != right:
        differences.append({"field": prefix, "left": left, "right": right})
    return differences


def capture_record(capture_dir):
    frames_path = capture_dir / "frames.jsonl"
    raw_path = capture_dir / "frames.nv12"
    rows = [json.loads(line) for line in frames_path.read_text(encoding="utf-8").splitlines()]
    if len(rows) != 300 or [row["index"] for row in rows] != list(range(300)):
        raise ValueError(f"expected 300 ordered encoder surfaces in {frames_path}")
    if any((row["width"], row["height"], row["software_format"]) != (1920, 1080, 23)
           for row in rows):
        raise ValueError(f"unexpected downloaded encoder surface geometry/format in {frames_path}")
    expected_bytes = 1920 * 1080 * 3 // 2 * 300
    if raw_path.stat().st_size != expected_bytes:
        raise ValueError(f"raw NV12 length mismatch: {raw_path.stat().st_size} != {expected_bytes}")
    contexts = {phase: load(capture_dir / f"context-{phase}.json") for phase in ("before", "after")}
    options = {phase: avoptions(capture_dir / f"avoptions-{phase}.txt") for phase in contexts}
    return {"frames": rows, "frames_sha256": digest(frames_path),
            "raw_nv12_sha256": digest(raw_path), "raw_nv12_bytes": raw_path.stat().st_size,
            "contexts": contexts,
            "context_sha256": {phase: digest(capture_dir / f"context-{phase}.json") for phase in contexts},
            "avoptions": options,
            "avoptions_sha256": {phase: digest(capture_dir / f"avoptions-{phase}.txt") for phase in options}}


def compare_captures(left, right):
    comparisons = {}
    for phase in ("before", "after"):
        a, b = left["contexts"][phase], right["contexts"][phase]
        raw_a, raw_b = left["avoptions"][phase], right["avoptions"][phase]
        comparisons[f"context_{phase}"] = {
            "equal": a == b,
            "raw_differences": json_differences(a, b),
        }
        comparisons[f"avoptions_{phase}"] = {
            "raw_equal": raw_a == raw_b,
            "raw_differences": [] if raw_a == raw_b else [{"left": raw_a, "right": raw_b}],
        }
    comparisons["frames_metadata_equal"] = left["frames"] == right["frames"]
    comparisons["frames_metadata_differences"] = json_differences(left["frames"], right["frames"])
    comparisons["raw_nv12_sha256_equal"] = left["raw_nv12_sha256"] == right["raw_nv12_sha256"]
    return comparisons


def save_progress(path, value):
    temporary = path.with_suffix(".json.tmp")
    with temporary.open("w", encoding="utf-8") as stream:
        json.dump(value, stream, indent=2, sort_keys=True)
        stream.write("\n")
        stream.flush()
        os.fsync(stream.fileno())
    os.replace(temporary, path)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--inputs", type=Path, default=Path("target/stage54c1-evidence/inputs"))
    parser.add_argument("--device", default=DEVICE)
    parser.add_argument("--timeout-seconds", type=float, default=2400)
    parser.add_argument("--va-trace", action="store_true",
                        help="request libva tracing for each case's first captured run")
    args = parser.parse_args()
    if args.device != DEVICE:
        parser.error(f"this frozen runner is bound to {DEVICE}")
    if args.timeout_seconds <= 0:
        parser.error("--timeout-seconds must be positive")

    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    events = output / "events.jsonl"
    status = {"schema_version": 1, "status": "Running", "output": str(output),
              "started_unix_seconds": time.time(), "runs": [], "comparisons": []}
    save(output / "run.json", status)
    try:
        binary = args.binary.resolve(strict=True)
        inputs = args.inputs.resolve(strict=True)
        matrix = load(ROOT / "tests/portability/c2-matrix.json")
        core = load(ROOT / "tests/portability/core-set.json")
        fixtures = {fixture["id"]: fixture for fixture in core["fixtures"] if fixture["id"] in CASES}
        if set(fixtures) != set(CASES):
            raise ValueError("core-set.json is missing a required frozen production case")
        host_stack = next(stack for stack in matrix["stacks"] if stack["id"] == matrix["canonical"])
        source = source_identity()
        binary_identity = identity(binary)
        oracle_identities = {path: digest(ROOT / path)
                             for path in HISTORICAL_ORACLE_SHA256}
        for path, expected_hash in HISTORICAL_ORACLE_SHA256.items():
            if oracle_identities[path] != expected_hash:
                raise ValueError(f"historical oracle changed: {path}")
        runtime_tools = {}
        for name in ("ffmpeg", "ffprobe", "rustc", "cargo"):
            executable = shutil.which(name)
            if executable is None:
                raise ValueError(f"required tool is unavailable: {name}")
            runtime_tools[name] = {"executable": identity(executable)}
        runtime_tools["kernel"] = query(["uname", "-a"])
        runtime_tools["rustc"]["version"] = query(["rustc", "--version", "--verbose"])
        runtime_tools["cargo"]["version"] = query(["cargo", "--version"])
        if any(runtime_tools[name]["version"]["exit_code"] for name in ("rustc", "cargo")):
            raise ValueError("could not identify the Rust toolchain")
        save(output / "runtime-tools.json", runtime_tools)
        save(output / "identity.json", {"head": source["head"],
              "dirty_diff_sha256": source["dirty_diff_sha256"],
              "source_files_sha256": source["files_sha256"],
              "cargo_lock_sha256": source["cargo_lock_sha256"],
              "binary": binary_identity, "host_stack_id": host_stack["id"],
              "historical_oracles": oracle_identities,
              "historical_oracle_expected_sha256": HISTORICAL_ORACLE_SHA256,
              "expected_toolchain": {"ffmpeg": "8.1.3", "anv": "26.2.3",
                                     "host_ihd": "26.1.5", "isolated_ihd": "25.4.6"}})

        all_captures = {}
        all_outputs = {}
        all_inputs = {}
        all_modules = {}
        linked_libraries_by_driver = {}
        for driver_id, driver_directory in DRIVERS.items():
            selector_path = (ROOT / driver_directory).resolve(strict=True)
            selected_driver = selector_path / "iHD_drv_video.so"
            if not selected_driver.is_file():
                raise ValueError(f"selected iHD driver not found: {selected_driver}")
            icd = Path(matrix["defaults"]["VK_ICD_FILENAMES"]).resolve(strict=True)
            env = environment(None, {"LIBVA_DRIVER_NAME": "iHD",
                                     "LIBVA_DRIVERS_PATH": str(selector_path),
                                     "VK_ICD_FILENAMES": str(icd)})
            # Only this runner owns instrumentation paths. Inherited shell
            # settings must never turn ordinary repeats into extra captures.
            env.pop("ASCIIFLOW_ENCODER_CAPTURE_DIRECTORY", None)
            env.pop("LIBVA_TRACE", None)
            ffmpeg = query(["ffmpeg", "-version"], env)
            ffprobe = query(["ffprobe", "-version"], env)
            libraries = loaded_libraries(binary, env)
            ffmpeg_packages = query(["rpm", "-q", "ffmpeg", "ffmpeg-libs"], env)
            if ffmpeg["exit_code"] or not ffmpeg["stdout"].startswith("ffmpeg version 8.1.3"):
                raise ValueError(f"required host FFmpeg 8.1.3 not selected: {ffmpeg['stdout'][:120]}")
            if ffprobe["exit_code"] or not ffprobe["stdout"].startswith("ffprobe version 8.1.3"):
                raise ValueError(f"required host ffprobe 8.1.3 not selected: {ffprobe['stdout'][:120]}")
            if ffmpeg_packages["exit_code"] or "8.1.3" not in ffmpeg_packages["stdout"]:
                raise ValueError("host FFmpeg and linked-library packages are not identified as 8.1.3")
            linked_libraries_by_driver[driver_id] = libraries
            tools = {"ffmpeg": ffmpeg, "ffprobe": ffprobe,
                     "ffmpeg_packages": ffmpeg_packages,
                     "linked_ffmpeg_libraries": libraries,
                     "selected_iHD": identity(selected_driver),
                     "mesa_vulkan_package": query(["rpm", "-q", "mesa-vulkan-drivers"], env)}
            if driver_id == "ihd-26.1.5-host":
                tools["host_iHD_package"] = query(["rpm", "-q", "intel-media-driver"], env)
                if "26.1.5" not in tools["host_iHD_package"]["stdout"]:
                    raise ValueError("host Intel media driver package does not identify as 26.1.5")
            if tools["mesa_vulkan_package"]["exit_code"] or "26.2.3" not in tools["mesa_vulkan_package"]["stdout"]:
                raise ValueError("host Mesa Vulkan driver package does not identify ANV 26.2.3")
            save(output / f"{driver_id}-tools.json", tools)
            for case in CASES:
                fixture = fixtures[case]
                source_path = fixture_input(fixture, inputs)
                fixture_sha256 = digest(source_path)
                case_directory = output / driver_id / case
                case_directory.mkdir(parents=True, exist_ok=False)
                runs = []
                capture_evidence = None
                for index in range(10):
                    if digest(binary) != binary_identity["sha256"]:
                        raise ValueError("CLI binary changed during the frozen measurement")
                    if digest(source_path) != fixture_sha256:
                        raise ValueError(f"{case}: fixture changed during the frozen measurement")
                    run_dir = case_directory / f"run-{index + 1:02d}"
                    destination = run_dir / "output.mp4"
                    report = run_dir / "diagnostic.json"
                    capture_dir = run_dir / "encoder-capture"
                    argv = runtime_argv(binary, fixture, source_path, destination, report)
                    run_env = dict(env)
                    if index == 0:
                        run_env["ASCIIFLOW_ENCODER_CAPTURE_DIRECTORY"] = str(capture_dir)
                    record = run_command(argv, run_env, run_dir, args.timeout_seconds,
                                         capture=index == 0, trace_vaapi=args.va_trace)
                    record.update({"driver": driver_id, "case": case, "repeat": index + 1,
                                   "input": {"path": str(source_path), "sha256": digest(source_path)},
                                   "binary_sha256": binary_identity["sha256"],
                                   "output": ({"path": str(destination), "sha256": digest(destination),
                                               "bytes": destination.stat().st_size}
                                              if destination.is_file() else None),
                                   "diagnostic": ({"path": str(report), "sha256": digest(report)}
                                                  if report.is_file() else None)})
                    runs.append(record)
                    status["runs"].append(record)
                    log_event(events, {"event": "run-complete", "record": record})
                    save_progress(output / "run.json", status)
                    if record["timed_out"] or record["exit_code"] != 0:
                        raise ValueError(f"{driver_id}/{case}/run-{index + 1}: command failed; inspect stderr.log")
                    if record["output"] is None or record["diagnostic"] is None:
                        raise ValueError(f"{driver_id}/{case}/run-{index + 1}: required output/report missing")
                    diagnostic = load(report)
                    if diagnostic.get("plan_scope") != "initialized_execution":
                        raise ValueError(f"{driver_id}/{case}/run-{index + 1}: execution plan not attested")
                    plan = diagnostic.get("selected_plan") or {}
                    if plan.get("encode") != "Hardware" or plan.get("output", {}).get("codec") != "H264" \
                            or plan.get("output", {}).get("bit_depth") != 8:
                        raise ValueError(f"{driver_id}/{case}/run-{index + 1}: expected initialized hardware H.264 8-bit output")
                    if index == 0:
                        if not (capture_dir / "frames.jsonl").is_file():
                            raise ValueError(f"{driver_id}/{case}: encoder surface capture missing")
                        capture_evidence = capture_record(capture_dir)
                        modules = modules_from_trace(run_dir / "stderr.log")
                        save(run_dir / "loaded-drivers.json", modules)
                        names = set(modules)
                        if names != {"iHD_drv_video.so", "libvulkan_intel.so"}:
                            raise ValueError(f"{driver_id}/{case}: actual iHD and ANV module initializers not attested: {modules}")
                        actual_ihd = modules["iHD_drv_video.so"]
                        if actual_ihd["sha256"] != identity(selected_driver)["sha256"]:
                            raise ValueError(f"{driver_id}/{case}: loaded iHD differs from selected file")
                        all_modules.setdefault("anv", []).append(modules["libvulkan_intel.so"])
                        all_modules.setdefault(driver_id, []).append(actual_ihd)
                    status["status"] = "Running"
                    save_progress(output / "run.json", status)
                hashes = [run["output"]["sha256"] for run in runs]
                if len(set(hashes)) != 1:
                    raise ValueError(f"{driver_id}/{case}: ten output files are not byte-identical: {hashes}")
                all_captures[(driver_id, case)] = capture_evidence
                all_outputs[(driver_id, case)] = hashes
                all_inputs[case] = {"path": str(source_path), "sha256": fixture_sha256}
                save(case_directory / "summary.json", {"driver": driver_id, "case": case,
                      "run_count": len(runs), "output_hashes": hashes,
                      "same_driver_outputs_byte_identical": True,
                      "captured_run_matches_ordinary_runs": True,
                      "input_sha256": digest(source_path), "capture": capture_evidence})

        anv_hashes = {record["sha256"] for record in all_modules["anv"]}
        anv_paths = {record["path"] for record in all_modules["anv"]}
        if len(anv_hashes) != 1 or len(anv_paths) != 1:
            raise ValueError(f"ANV changed across runs: paths={anv_paths}, hashes={anv_hashes}")
        for driver_id in DRIVERS:
            records = all_modules[driver_id]
            if len({record["sha256"] for record in records}) != 1:
                raise ValueError(f"iHD module changed within {driver_id}")
        linked_identities = list(linked_libraries_by_driver.values())
        if linked_identities[0] != linked_identities[1]:
            raise ValueError("FFmpeg shared-library closure changed between iHD driver runs")

        comparisons = {}
        for driver_id in DRIVERS:
            canonical = all_captures[(driver_id, "canonical-h2648")]
            for case in CASES[1:]:
                result = compare_captures(canonical, all_captures[(driver_id, case)])
                comparisons[f"{driver_id}/canonical-h2648-vs-{case}"] = result
                if case == "canonical-h2648-auto":
                    for key in ("frames_metadata_equal", "raw_nv12_sha256_equal"):
                        if not result[key]:
                            raise ValueError(f"{driver_id}: canonical and automatic-route {key} differs")
                    for phase in ("before", "after"):
                        if not result[f"context_{phase}"]["equal"]:
                            raise ValueError(f"{driver_id}: canonical and automatic-route context-{phase} differs")
                        if not result[f"avoptions_{phase}"]["raw_equal"]:
                            raise ValueError(f"{driver_id}: canonical and automatic-route AVOptions-{phase} differs")
            save(output / f"{driver_id}-capture-comparisons.json",
                 {key: value for key, value in comparisons.items()
                  if key.startswith(driver_id + "/")})
        host_id, isolated_id = tuple(DRIVERS)
        for case in CASES:
            key = f"{host_id}-vs-{isolated_id}/{case}"
            result = compare_captures(all_captures[(host_id, case)],
                                      all_captures[(isolated_id, case)])
            result["outputs_byte_identical"] = all_outputs[(host_id, case)] == all_outputs[(isolated_id, case)]
            result["host_output_sha256_by_repeat"] = all_outputs[(host_id, case)]
            result["isolated_output_sha256_by_repeat"] = all_outputs[(isolated_id, case)]
            result["input"] = all_inputs[case]
            comparisons[key] = result
            if not result["frames_metadata_equal"] or not result["raw_nv12_sha256_equal"]:
                raise ValueError(f"cross-driver encoder capture differs for {case}")
            for phase in ("before", "after"):
                if not result[f"context_{phase}"]["equal"]:
                    raise ValueError(f"cross-driver encoder context-{phase} differs for {case}")
                if not result[f"avoptions_{phase}"]["raw_equal"]:
                    raise ValueError(f"cross-driver encoder AVOptions-{phase} differs for {case}")
        save(output / "cross-driver-capture-comparisons.json",
             {key: value for key, value in comparisons.items()
              if key.startswith(host_id + "-vs-" + isolated_id + "/")})
        if source_identity() != source:
            raise ValueError("source, Cargo.lock, HEAD or dirty diff changed during the measurement")
        if identity(binary) != binary_identity:
            raise ValueError("CLI binary changed during the frozen measurement")
        for case, item in all_inputs.items():
            if digest(item["path"]) != item["sha256"]:
                raise ValueError(f"fixture changed during the frozen measurement: {case}")
        status["comparisons"] = comparisons
        status["status"] = "Passed"
        status["completed_unix_seconds"] = time.time()
        save_progress(output / "run.json", status)
        log_event(events, {"event": "runner-finished", "status": status["status"]})
        return 0
    except BaseException as error:
        status["status"] = "Failed"
        status["failure"] = f"{type(error).__name__}: {error}"
        status["completed_unix_seconds"] = time.time()
        save_progress(output / "run.json", status)
        log_event(events, {"event": "runner-failed", "failure": status["failure"]})
        print(status["failure"], file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
