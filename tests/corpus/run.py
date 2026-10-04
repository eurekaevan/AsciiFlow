#!/usr/bin/env python3
"""Production corpus orchestrator. Never promotes or overwrites retained evidence."""
import argparse
from collections import Counter
from fractions import Fraction
import hashlib
import importlib.util
import json
import math
import os
import signal
import shutil
from pathlib import Path
import subprocess
import sys
import time

sys.dont_write_bytecode = True
from schema import validate
ROOT = Path(__file__).resolve().parents[2]
MEDIA = ROOT / "tests/baselines/media"


def digest(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def load(path):
    return json.loads(Path(path).read_text(encoding="utf-8"))


def save(path, document):
    with Path(path).open("x", encoding="utf-8") as stream:
        json.dump(document, stream, indent=2, sort_keys=True)
        stream.write("\n")


def repository_path(value):
    path = Path(value)
    resolved = (ROOT / path).resolve()
    if path.is_absolute() or not resolved.is_relative_to(ROOT):
        raise ValueError(f"nonportable or escaping repository path: {value}")
    return resolved


def oracle(name):
    spec = importlib.util.spec_from_file_location(name, MEDIA / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def outcome(expected, success, failure_stage=None, category=None):
    """Unexpected success is as serious as unexpected rejection."""
    if expected["runtime"] == "Pass":
        return "PASS" if success else "FAILED"
    if success:
        return "FAILED"
    if failure_stage != expected["failure_stage"]:
        return "FAILED"
    if expected["root_cause"] is not None and category != expected["root_cause"]:
        return "FAILED"
    return "PASS"


def hardware_availability(device):
    # This describes the CURRENT process environment, not the host's hardware.
    path = Path(device)
    if not path.exists():
        return "render node not visible in this execution environment (host GPU not inferred)"
    if not path.is_char_device() or not os.access(path, os.R_OK | os.W_OK):
        return "render node is not accessible in this execution environment"
    return None


def assert_plan_capabilities(diagnostic):
    """Selected paths must agree with input/profile-specific runtime facts."""
    plan, caps = diagnostic["selected_plan"], diagnostic["capabilities"]
    source, output = diagnostic["input_requirements"], plan["output"]
    def supported(group, key):
        assert caps[group][key]["state"] == "Supported", (group, key, caps[group][key])
    supported("processing", plan["backend"].lower())
    color_key = {"HdrPqPreserve": "vulkan_pq", "HdrPqToSdrBt709": "vulkan_hdr_to_sdr"}.get(plan["color_processing"])
    if color_key:
        supported("processing", color_key)
    def codec_fact(codec, depth, operation):
        prefix = ("hevc_main10" if codec == "Hevc" else "av1_10bit") if depth == 10 and codec in ("Hevc", "Av1") else codec.lower()
        return prefix + f"_vaapi_{operation}"
    if plan["decode"] == "Hardware":
        supported("media", "vaapi_device")
        supported("media", codec_fact(source["codec"], source["bit_depth"], "decode"))
    else:
        supported("media", "software_decode")
    if plan["encode"] == "Hardware":
        supported("media", codec_fact(output["codec"], output["bit_depth"], "encode"))
    else:
        assert output["codec"] == "H264" and output["bit_depth"] == 8
        supported("media", "software_encode")
    if plan["hardware_input_interop"]:
        key = "p010_input" if source["bit_depth"] == 10 else {"H264": "input", "Hevc": "hevc_input", "Av1": "av1_input"}[source["codec"]]
        supported("interop", key)
    if plan["hardware_output_interop"]:
        key = ("av1_p010_output" if output["codec"] == "Av1" else "p010_output") if output["bit_depth"] == 10 else {"H264": "output", "Hevc": "hevc_output", "Av1": "av1_output"}[output["codec"]]
        supported("interop", key)


class Run:
    def __init__(self, args):
        self.args = args
        self.out = args.output.resolve()
        if self.out.is_relative_to(ROOT / "tests/baselines"):
            raise ValueError("normal runs cannot write retained baselines; promotion is a separate reviewed action")
        self.out.mkdir(parents=True, exist_ok=False)
        self.results = []
        self.commands = []
        self.fixture_cache = {}
        self.fixture_setups = []
        self.binary_source = args.binary.resolve()
        self.binary = self.binary_source
        if self.binary_source.is_file():
            # A concurrent build must not silently change the executable halfway
            # through a corpus/retained run. This is an execution artifact, not a
            # new golden or another build pipeline.
            self.binary = self.out / "production-asciiflow"
            shutil.copy2(self.binary_source, self.binary)
        self.skip_hardware = hardware_availability(args.device)

    def command(self, label, argv, env=None, timeout=None, stderr_log=None):
        index = len(self.commands)
        logfile = self.out / f"command-{index:03d}.log"
        started = time.monotonic()
        timeout = getattr(self.args, "watchdog_seconds", 600) if timeout is None else timeout
        if not math.isfinite(timeout) or timeout <= 0:
            raise ValueError("command deadline must be finite and positive")
        timed_out = False
        peak_rss_kib = 0
        from contextlib import ExitStack
        with ExitStack() as resources:
            stream = resources.enter_context(logfile.open("x"))
            errors = resources.enter_context(Path(stderr_log).open("x")) if stderr_log else subprocess.STDOUT
            process = subprocess.Popen([str(x) for x in argv], cwd=ROOT, env=env,
                                       stdout=stream, stderr=errors,
                                       start_new_session=True)
            deadline = started + timeout
            while process.poll() is None:
                try:
                    status = Path(f"/proc/{process.pid}/status").read_text()
                    for line in status.splitlines():
                        if line.startswith("VmHWM:"):
                            peak_rss_kib = max(peak_rss_kib, int(line.split()[1]))
                except (FileNotFoundError, ProcessLookupError):
                    pass
                if time.monotonic() >= deadline:
                    timed_out = True
                    try:
                        os.killpg(process.pid, signal.SIGTERM)
                    except ProcessLookupError:
                        pass  # It exited between the deadline check and signal.
                    try:
                        process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        pass
                    # The leader exiting does not imply that its descendants
                    # exited. Kill surviving group members even in that case.
                    try:
                        os.killpg(process.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                    process.wait(timeout=5)
                    break
                try:
                    process.wait(timeout=min(0.1, max(0.001, deadline-time.monotonic())))
                except subprocess.TimeoutExpired:
                    pass
        self.commands.append({"id": label, "argv": [str(x) for x in argv],
                              "exit_code": process.returncode, "log": logfile.name,
                              "watchdog": "TIMEOUT / possible hang" if timed_out else "completed",
                              "peak_process_rss_kib": peak_rss_kib,
                              "elapsed_seconds": time.monotonic() - started})
        if stderr_log:
            self.commands[-1]["stderr_log"] = Path(stderr_log).name
        if timed_out:
            raise ValueError(f"{label}: TIMEOUT / possible hang; process group terminated, not an ordinary rejection")
        return process.returncode == 0

    def gate(self, name, category, action, hardware=False):
        if hardware and self.skip_hardware:
            self.results.append({"id": name, "category": category, "result": "SKIPPED",
                                 "reason": self.skip_hardware})
            return False
        try:
            evidence = action()
            result = ("SKIPPED" if isinstance(evidence, dict) and evidence.get("skipped") else
                      "UNQUALIFIED" if isinstance(evidence, dict) and evidence.get("unqualified") else "PASS")
            self.results.append({"id": name, "category": category, "result": result,
                                 "evidence": evidence})
            return not (isinstance(evidence, dict) and evidence.get("skipped"))
        except (AssertionError, ValueError, OSError, subprocess.SubprocessError) as error:
            self.results.append({"id": name, "category": category, "result": "FAILED",
                                 "reason": str(error)})
            return False

    def checked(self, name, argv, env=None, timeout=None):
        if not self.command(name, argv, env, **({"timeout": timeout} if timeout is not None else {})):
            raise ValueError(f"{name}: nonzero exit; inspect command log")
        return {"command": name}

    def environment(self):
        def query(argv):
            try:
                result = subprocess.run(argv, cwd=ROOT, capture_output=True, text=True)
                return {"exit_code": result.returncode, "stdout": result.stdout,
                        "stderr": result.stderr}
            except OSError as error:
                return {"unavailable": str(error)}
        dirty = subprocess.run(["git", "diff", "--binary", "HEAD"], cwd=ROOT,
                               capture_output=True, check=True).stdout
        untracked = subprocess.run(["git", "ls-files", "--others", "--exclude-standard"],
                                   cwd=ROOT, capture_output=True, text=True, check=True).stdout.splitlines()
        document = {"head": query(["git", "rev-parse", "HEAD"]),
                    "dirty_diff_sha256": hashlib.sha256(dirty).hexdigest(),
                    "untracked_sha256": {p: digest(ROOT / p) for p in sorted(untracked) if (ROOT / p).is_file()},
                    "cargo_lock_sha256": digest(ROOT / "Cargo.lock"),
                    "binary_sha256": digest(self.binary) if self.binary.is_file() else None,
                    "binary_source": str(getattr(self, "binary_source", self.binary)),
                    "ffmpeg": query(["ffmpeg", "-version"]),
                    "ffprobe": query(["ffprobe", "-version"]),
                    "kernel": query(["uname", "-a"]),
                    "gpu": query(["lspci", "-nn", "-s", "00:02.0"]),
                    "vaapi": query(["vainfo", "--display", "drm", "--device", self.args.device]),
                    "packages": query(["rpm", "-q", "ffmpeg", "ffmpeg-libs", "mesa-vulkan-drivers", "intel-media-driver"]),
                    "hardware_skip": self.skip_hardware}
        save(self.out / "environment.json", document)

    def validate_source_recipe(self, fixture):
        source = fixture["source"]
        if source.get("generator"):
            generator = repository_path(source["generator"])
            if digest(generator) != source["generator_sha256"]:
                raise ValueError(f"{fixture['id']}: generator identity mismatch")
        if source.get("reference_manifest"):
            ref = source["reference_manifest"]
            if digest(repository_path(ref["path"])) != ref["sha256"]:
                raise ValueError(f"{fixture['id']}: reference manifest identity mismatch")

    def fixture_path(self, fixture):
        source = fixture["source"]
        self.validate_source_recipe(fixture)
        if source["kind"] == "generated":
            if self.args.generated_inputs is not None:
                cached = self.args.generated_inputs.resolve() / Path(source["path"]).name
                if cached.is_file():
                    if cached.stat().st_size != source["byte_size"] or digest(cached) != source["sha256"]:
                        raise ValueError(f"{fixture['id']}: cached fixture setup identity mismatch")
                    self.fixture_setups.append({"fixture": fixture["id"], "mode": "identity-checked generated input reuse",
                                                "path": str(cached), "sha256": digest(cached)})
                    return cached
            path = self.out / "generated" / Path(source["path"]).name
            path.parent.mkdir(exist_ok=True)
            argv = [str(path) if item == "{output}" else item for item in source["exact_command"]]
            if str(path) not in self.fixture_cache:
                self.checked(f"generate-{fixture['id']}", argv,
                             timeout=getattr(self.args, "generation_watchdog_seconds", 2400))
                self.fixture_cache[str(path)] = source["sha256"]
            elif self.fixture_cache[str(path)] != source["sha256"]:
                raise ValueError("conflicting generated fixture identities")
        elif source["kind"] == "retained_binary":
            path = repository_path(source["path"])
        else:
            raise ValueError("external references require an explicit local artifact; no automatic download")
        if path.stat().st_size != source["byte_size"] or digest(path) != source["sha256"]:
            raise ValueError(f"{fixture['id']}: fixture setup identity mismatch")
        return path

    def fixture(self, fixture):
        # Reject cases run even without hardware: first-frame classification and
        # policy validation occur before GPU selection. Positive hardware cases
        # are visibly skipped, never treated as expected failures.
        expected = fixture["expected"]
        positive = expected["runtime"] == "Pass"
        execution_rejection = expected["runtime"] in ("RejectAtInitialization", "RuntimeFailureExpected")
        request = fixture["request"]
        portable_sdr = (positive and fixture["media"]["bit_depth"] == 8 and
                        expected["classification"] == "Sdr" and request["codec"] == "h264" and request["bit_depth"] == 8)
        if (execution_rejection or (positive and not portable_sdr)) and self.skip_hardware:
            if fixture["source"]["kind"] == "generated":
                self.validate_source_recipe(fixture)
                return {"skipped": True, "reason": self.skip_hardware,
                        "expected_input_sha256": fixture["source"]["sha256"],
                        "input_identity_verified": False,
                        "setup": "generator identity checked; native generation deferred with hardware gate"}
            path = self.fixture_path(fixture)
            return {"skipped": True, "reason": self.skip_hardware, "input_sha256": digest(path)}
        path = self.fixture_path(fixture)
        report_path = self.out / f"{fixture['id']}-plan.json"
        options = ["--audio", "none", "--output-codec", request["codec"],
                   "--output-bit-depth", str(request["bit_depth"]),
                   "--output-dynamic-range", request["dynamic_range"], "--hw-device", self.args.device]
        if portable_sdr and self.skip_hardware:
            options += ["--backend", "cpu", "--decode", "software", "--encode", "software",
                        "--vaapi-vulkan-input-interop", "off", "--vaapi-vulkan-output-interop", "off"]
        argv = [self.binary, path, "--explain-plan", "--capabilities", *options,
                "--diagnostic-report", report_path]
        success = self.command(f"plan-{fixture['id']}", argv)
        diagnostic = load(report_path)
        failure = diagnostic.get("failure") or {}
        if execution_rejection and not success:
            raise ValueError(f"{fixture['id']}: rejected before expected execution failure: {failure}")
        if not execution_rejection and outcome(expected, success, failure.get("stage"), failure.get("category")) != "PASS":
            raise ValueError(f"{fixture['id']}: unexpected pass/reject or failure category/stage: {failure}")
        classification = (diagnostic.get("input_requirements") or {}).get("dynamic_range")
        # A conflict/unknown qualification failure supersedes a stream-only
        # snapshot, which cannot represent the rejected first frame's resolution.
        if failure.get("code") in ("Conflicting", "Unknown"):
            classification = failure["code"]
        if classification is not None:
            assert classification == expected["classification"], (fixture["id"], classification)
        elif success:
            raise ValueError(f"{fixture['id']}: successful inspection omitted input classification")
        classification_evidence = {"observed": classification,
                                   "scope": "resolved first-frame semantics" if classification is not None else
                                   "not observed: policy may reject before input probing, or first-frame color rejection may leave semantics unresolved"}
        evidence = {"input_sha256": digest(path), "diagnostic": report_path.name,
                    "classification": classification_evidence,
                    "expected_rejection": not positive, "selected_plan": diagnostic.get("selected_plan")}
        if positive or execution_rejection:
            assert diagnostic["selected_plan"] is not None
            assert diagnostic["capabilities"]["scope"] == "runtime_probe_not_global_qualification"
            assert_plan_capabilities(diagnostic)
            output = diagnostic["selected_plan"]["output"]
            assert output["bit_depth"] == request["bit_depth"]
            assert output["codec"].lower() == request["codec"]
            output_path = self.out / f"{fixture['id']}-output.mp4"
            runtime_report = self.out / f"{fixture['id']}-runtime.json"
            runtime_success = self.command(f"runtime-{fixture['id']}",
                                           [self.binary, path, output_path, *options, "--no-progress",
                                            "--diagnostic-report", runtime_report])
            actual = load(runtime_report)
            failure = actual.get("failure") or {}
            if outcome(expected, runtime_success, failure.get("stage"), failure.get("category")) != "PASS":
                raise ValueError(f"{fixture['id']}: unexpected runtime pass/reject or failure category/stage: {failure}")
            evidence["runtime_diagnostic"] = runtime_report.name
            evidence["actual_plan"] = actual.get("selected_plan")
            if execution_rejection:
                assert not output_path.exists(), "expected failure committed an output artifact"
            else:
                assert actual["plan_scope"] == "initialized_execution"
                assert_plan_capabilities(actual)
                evidence["decode_back"] = self.decode_back(fixture, output_path)
                evidence["output_sha256"] = digest(output_path)
        return evidence

    def decode_back(self, fixture, output):
        """Check fixture output usability; retained regression comparators stay separate."""
        name = fixture["id"]
        self.checked(f"decode-back-probe-{name}", ["ffprobe", "-v", "error", "-show_streams",
                                                  "-show_frames", "-of", "json", output])
        document = load(self.out / self.commands[-1]["log"])
        streams = document["streams"]
        assert len(streams) == 1 and streams[0]["codec_type"] == "video"
        stream = streams[0]
        request, media = fixture["request"], fixture["media"]
        frames = document["frames"]
        count = media["timing"]["frame_count"]
        assert count is not None, "runtime fixture requires an explicit frame-count oracle"
        assert len(frames) == count, (name, len(frames), count)
        assert stream["codec_name"] == request["codec"]
        if request["codec"] in ("hevc", "av1"):
            assert stream["profile"] == ("Main 10" if request["codec"] == "hevc" and request["bit_depth"] == 10 else "Main")
        rate = Fraction(media["timing"]["frame_rate"])
        assert Fraction(stream["avg_frame_rate"]) == rate
        if media["timing"].get("model", "CFR") == "CFR" and media["timing"]["start_pts"] == 0:
            time_base = Fraction(stream["time_base"])
            for index, frame in enumerate(frames):
                assert int(frame["best_effort_timestamp"]) * time_base == index / rate, (name, index, frame)
        pixel_format = "yuv420p10le" if request["bit_depth"] == 10 else "yuv420p"
        pq = fixture["expected"]["classification"] == "HdrPq" and request["dynamic_range"] == "preserve"
        color = {"color_primaries": "bt2020" if pq else "bt709",
                 "color_transfer": "smpte2084" if pq else "bt709",
                 "color_space": "bt2020nc" if pq else "bt709", "color_range": "tv"}
        for value in [stream, *frames]:
            assert (value["width"], value["height"], value["pix_fmt"]) == (media["width"], media["height"], pixel_format)
            assert all(value.get(key) == expected for key, expected in color.items()), (name, value)
            assert value.get("chroma_location") == "left", (name, value.get("chroma_location"))
            if not pq:
                for side in value.get("side_data_list", []):
                    kind = side.get("side_data_type", "").lower()
                    assert not any(marker in kind for marker in ("mastering", "content light", "hdr", "dovi", "dolby vision")), side
        self.checked(f"decode-back-frames-{name}", ["ffmpeg", "-v", "error", "-xerror", "-nostdin",
                                                   "-i", output, "-map", "0:v:0", "-an", "-pix_fmt", pixel_format,
                                                   "-f", "framehash", "-hash", "sha256", "-"])
        decoded = [line for line in (self.out / self.commands[-1]["log"]).read_text().splitlines()
                   if line.strip() and not line.startswith("#")]
        assert len(decoded) == count, (name, len(decoded), count)
        return {"frames": count, "codec": request["codec"], "pixel_format": pixel_format,
                "scope": "full decode, geometry, depth and color metadata; no lossy pixel equivalence claim"}

    def smoke(self, name, source, codec, depth, dynamic_range):
        output = self.out / f"{name}.mp4"
        report = self.out / f"{name}.json"
        argv = [self.binary, source, output, "--width", "80", "--charset", "standard",
                "--font", "builtin-8x8", "--color", "true", "--audio", "none", "--max-frames", "300",
                "--decode", "vaapi", "--backend", "vulkan", "--vulkan-mapping", "gpu",
                "--encode", "vaapi", "--hw-device", self.args.device,
                "--vaapi-vulkan-input-interop", "on", "--vaapi-vulkan-output-interop", "on",
                "--output-codec", codec, "--output-bit-depth", str(depth),
                "--output-dynamic-range", dynamic_range, "--no-progress", "--diagnostic-report", report]
        inspection_report = self.out / f"{name}-plan.json"
        inspection = argv[:-2] + ["--diagnostic-report", inspection_report, "--explain-plan", "--capabilities"]
        self.checked(f"{name}-plan", inspection)
        inspected_plan = load(inspection_report)
        assert inspected_plan["plan_scope"] == "selected_by_planner"
        assert_plan_capabilities(inspected_plan)
        self.checked(name, argv)
        diagnostic = load(report)
        assert diagnostic["plan_scope"] == "initialized_execution"
        assert_plan_capabilities(diagnostic)
        plan = diagnostic["selected_plan"]
        assert plan["hardware_input_interop"] and plan["hardware_output_interop"]
        pq_mode = name == "smoke-pq-preserve"
        inspected = oracle("verify-pq-production").inspect(output, codec) if pq_mode else oracle("verify-hdr-to-sdr-production").inspect(output, codec, depth)
        save(self.out / f"{name}-oracle.json", inspected)
        return {"output_sha256": digest(output), "actual_path": plan, "decoded_frames": 300,
                "oracle": f"{name}-oracle.json"}

    def retained(self, input8):
        env = dict(os.environ, ASCIIFLOW_BASELINE_BINARY=str(self.binary))
        fixtures = ROOT / "tests/fixtures/codecs"
        sdr_dir, pq_dir, c_dir = (self.out / name for name in ("retained-sdr", "retained-pq", "retained-c"))
        for name, argv in (
            ("generate-retained-sdr", ["bash", MEDIA / "generate-post-polarity-v2.sh", input8, fixtures / "hevc-main10-canonical-v1.mp4", sdr_dir]),
            ("generate-retained-pq", ["bash", MEDIA / "generate-pq-production-v1.sh", fixtures, pq_dir]),
            ("generate-retained-c", ["bash", MEDIA / "generate-hdr-to-sdr-production-v1.sh", fixtures, c_dir]),
        ):
            self.checked(name, argv, env)
        self.checked("retained-pq-oracle", ["python3", MEDIA / "verify-pq-production.py", pq_dir,
                     self.out / "retained-pq-oracle.json", "--input-directory", fixtures,
                     "--check-baseline", MEDIA / "pq-production-v1.json", "--binary", self.binary])
        self.checked("retained-c-oracle", ["python3", MEDIA / "verify-hdr-to-sdr-production.py", "verify", c_dir,
                     self.out / "retained-c-oracle.json", "--input-directory", fixtures])
        # Reuse the established comparator; don't invent a second lossy oracle.
        # New CLI diagnostics change build identity. Equality of actual bytes is
        # recorded separately from the historical exact-build attestation.
        sdr_baseline = load(MEDIA / "post-polarity-v2.json")
        sdr_oracle = oracle("verify-hdr-to-sdr-production")
        for case in sdr_baseline["cases"]:
            codec, depth = case["name"].split("-")
            records = [sdr_oracle.inspect(sdr_dir / f"{case['name']}-run{i}.mp4", codec, int(depth)) for i in (1, 2, 3)]
            assert all(sdr_oracle.compare_runs(records).values())
            assert all(record["identity"]["sha256"] == case["output_sha256_runs_1_2_3"][0] for record in records)
        current = load(self.out / "retained-c-oracle.json")
        golden = load(MEDIA / "hdr-to-sdr-production-v1.json")
        # Reuse all five retained tiers. Exact-build attestation remains a
        # separate, strict historical claim; the golden provenance is immutable.
        sdr_oracle.check_retained_artifacts(current, golden)
        # Existing Rust H.264 semantic oracle, including its narrow allowlist,
        # remains mandatory even when raw hashes happen to match.
        self.checked("h264-semantic-oracle", ["cargo", "test", "-p", "asciiflow-media", "--test", "media_regression"])
        pair_env = dict(env, ASCIIFLOW_REGRESSION_REFERENCE=str(sdr_dir / "h264-8-run1.mp4"),
                        ASCIIFLOW_REGRESSION_CANDIDATE=str(sdr_dir / "h264-8-run2.mp4"),
                        ASCIIFLOW_REGRESSION_EXACT_BUILD="attested")
        self.checked("h264-production-pair", ["cargo", "test", "-p", "asciiflow-media", "--test", "media_regression",
                     "compare_pair_from_env", "--", "--ignored", "--nocapture"], pair_env)
        return {"sdr_paths": 5, "pq_paths": 2, "hdr_to_sdr_paths": 10, "runs_each": 3,
                "retained_artifact_identity": "identical", "historical_build_attestation": "not transferred to new build"}

    def finish(self):
        self.results.sort(key=lambda item: item["id"])
        summary = {category: dict(sorted(Counter(r["result"] for r in self.results if r["category"] == category).items()))
                   for category in ("classification", "planner", "runtime", "decode-back", "media oracle", "hardware", "static")}
        # A fixture gate may verify more than one surface. Counts are explicitly
        # overlapping, not an inflated claim about unique tests or fixtures.
        surfaces = []
        for record in self.results:
            if record["result"] != "PASS":
                continue
            evidence = record.get("evidence") or {}
            if evidence.get("selected_plan"):
                surfaces.append({"fixture": record["id"], "surface": "planner", "result": "PASS"})
            if evidence.get("runtime_diagnostic"):
                surfaces.append({"fixture": record["id"], "surface": "runtime", "result": "PASS"})
            if evidence.get("decode_back"):
                surfaces.append({"fixture": record["id"], "surface": "decode-back", "result": "PASS"})
        for surface in surfaces:
            category = surface["surface"]
            summary[category]["PASS"] = summary[category].get("PASS", 0) + 1
        save(self.out / "results.json", {"schema_version": 1, "mode": self.args.mode,
                                       "results": self.results, "summary": summary, "verified_surfaces": surfaces,
                                       "summary_semantics": "overlapping verification surfaces; not unique-test counts",
                                       "fixture_setups": sorted(self.fixture_setups, key=lambda item: item["fixture"]),
                                       "commands": self.commands})
        lines = ["# Compatibility corpus results", "", f"Mode: {self.args.mode}", "",
                 "Support contract: tests/support/production-support-v1.json", "", "| Gate | Category | Result |", "|---|---|---|"]
        lines.extend(f"| {r['id']} | {r['category']} | {r['result']} |" for r in self.results)
        for title, status in (("Unexpected failures / passes / rejects", "FAILED"), ("Skipped hardware", "SKIPPED")):
            lines += ["", f"## {title}", ""]
            matches = [r for r in self.results if r["result"] == status]
            lines += [f"- {r['id']}: {r.get('reason') or (r.get('evidence') or {}).get('reason', '')}" for r in matches] or ["None."]
        with (self.out / "results.md").open("x") as stream:
            stream.write("\n".join(lines) + "\n")
        print(json.dumps(summary, sort_keys=True))
        return 1 if any(r["result"] == "FAILED" for r in self.results) else 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("quick", "full", "hardware"))
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/release/asciiflow")
    parser.add_argument("--device", default="/dev/dri/renderD128")
    parser.add_argument("--generated-inputs", type=Path, help="read-only reuse of already generated inputs, still checked against exact size/SHA and generator identity")
    parser.add_argument("--retained", action="store_true", help="all 17 retained production paths, three runs each; hardware required")
    parser.add_argument("--manifest", type=Path, default=ROOT / "tests/corpus/representative-v1.json")
    parser.add_argument("--watchdog-seconds", type=float, default=600, help="per-command deadline; timeouts never count as expected rejection")
    parser.add_argument("--generation-watchdog-seconds", type=float, default=2400, help="bounded setup deadline for expensive retained lossless generation")
    args = parser.parse_args()
    if args.mode == "quick" and args.retained:
        parser.error("--retained requires full or hardware mode so the C-1/C-2B prerequisites run")
    if args.retained and args.device != "/dev/dri/renderD128":
        parser.error("retained canonical commands are pinned to /dev/dri/renderD128; other nodes may run hardware smokes without --retained")
    if any(not math.isfinite(value) or value <= 0 for value in
           (args.watchdog_seconds, args.generation_watchdog_seconds)):
        parser.error("watchdog deadlines must be positive")
    if args.manifest.name == "real-media-v1.json":
        from real_media import RealMediaMixin
        class RealRun(RealMediaMixin, Run):
            pass
        run = RealRun(args)
    else:
        run = Run(args)
    run.environment()
    manifest = load(args.manifest)
    run.gate("corpus-schema", "static", lambda: validate(manifest, load(ROOT / "tests/corpus/manifest.schema.json")))
    if run.results[-1]["result"] == "FAILED":
        return run.finish()
    if len({fixture["id"] for fixture in manifest["fixtures"]}) != len(manifest["fixtures"]):
        run.results.append({"id": "corpus-unique-identities", "category": "static",
                            "result": "FAILED", "reason": "duplicate fixture identities"})
        return run.finish()
    run.gate("support-planner-consistency", "planner", lambda: run.checked("support-planner", ["cargo", "test", "-p", "asciiflow-core", "--test", "support_contract"]))
    run.gate("support-document-consistency", "static", lambda: run.checked("support-document", ["python3", "tests/support/matrix.py", "check"]))
    coverage_spec = importlib.util.spec_from_file_location("support_matrix", ROOT / "tests/support/matrix.py")
    matrix = importlib.util.module_from_spec(coverage_spec)
    coverage_spec.loader.exec_module(matrix)
    save(run.out / "pairwise-coverage.json", matrix.coverage(load(ROOT / "tests/support/production-support-v1.json")))
    for fixture in sorted(manifest["fixtures"], key=lambda x: x["id"]):
        run.gate(fixture["id"], "classification", lambda f=fixture: run.fixture(f))
    run.gate("media-oracle-controls", "media oracle", lambda: run.checked("media-oracle-controls", ["cargo", "test", "-p", "asciiflow-media", "--test", "media_regression"]))
    if args.mode != "quick":
        run.gate("c1-cpu-reference", "runtime", lambda: run.checked("c1", ["bash", "scripts/qualify-tone-map-cpu.sh", run.out / "c1"]))
        run.gate("c2b-cpu-reference", "runtime", lambda: run.checked("c2b", ["bash", "scripts/qualify-target-volume-cpu.sh", run.out / "c1/method-a-run1.bin", run.out / "c2b"]))
    if args.mode == "full":
        run.gate("workspace-tests", "static", lambda: run.checked("workspace-tests", ["cargo", "test", "--workspace"]))
    if args.mode == "hardware" or args.retained:
        fixtures = ROOT / "tests/fixtures/codecs"
        # Hardware smokes and retained hashes require the 300-frame canonical
        # source, not whichever short real-media H.264 case sorts first.
        canonical_manifest = load(ROOT / "tests/corpus/representative-v1.json")
        canonical_input = next(f for f in canonical_manifest["fixtures"] if f["id"] == "canonical-h2648")
        prepared_inputs = {}
        def prepare_sdr_input():
            path = run.fixture_path(canonical_input)
            prepared_inputs["sdr"] = path
            return {"input_sha256": digest(path)}
        input_ready = run.gate("hardware-sdr-input-identity", "static", prepare_sdr_input, True)
        input8 = prepared_inputs.get("sdr")
        for name, path, codec, depth, dr in (
            ("smoke-sdr", input8, "h264", 8, "preserve"),
            ("smoke-pq-preserve", fixtures / "hevc-main10-pq-canonical-v1.mp4", "hevc", 10, "preserve"),
            ("smoke-pq-to-sdr", fixtures / "hevc-main10-pq-c3-legal-v1.mp4", "hevc", 10, "sdr"),
            ("smoke-pq-to-sdr8", fixtures / "hevc-main10-pq-c3-legal-v1.mp4", "h264", 8, "sdr"),
        ):
            if name == "smoke-sdr" and not input_ready and not run.skip_hardware:
                run.results.append({"id": name, "category": "hardware", "result": "SKIPPED",
                                    "reason": "blocked by failed hardware-sdr-input-identity setup"})
                continue
            run.gate(name, "hardware", lambda n=name,p=path,c=codec,d=depth,r=dr: run.smoke(n,p,c,d,r), True)
        if args.retained:
            if not input_ready and not run.skip_hardware:
                run.results.append({"id": "all-retained-production", "category": "media oracle", "result": "SKIPPED",
                                    "reason": "blocked by failed hardware-sdr-input-identity setup"})
            else:
                run.gate("all-retained-production", "media oracle", lambda: run.retained(input8), True)
        for name, command in (
            ("c3-retained-reference", ["cargo", "test", "-p", "asciiflow-vulkan", "--release", "--features", "hdr-to-sdr-qualification", "--test", "c3_qualification", "canonical_f64_and_f32_input_oracles", "--", "--ignored", "--test-threads=1"]),
            ("c4a-retained-reference", ["cargo", "test", "-p", "asciiflow-vulkan", "--release", "--features", "hdr-to-sdr-qualification", "--test", "c4a_quantization", "--", "--ignored", "--test-threads=1"]),
        ):
            env = dict(os.environ, C1_INPUT=str(run.out / "c1/linear-bt2020-1000-v1.bin"),
                       C1_OUTPUT=str(run.out / "c1/method-a-run1.bin"), ASCIIFLOW_VULKAN_VALIDATION="1",
                       C3_REPORT=str(run.out / "c3-retained.json"), C4A_BOUNDARY_REPORT=str(run.out / "c4a-retained.json"))
            run.gate(name, "hardware", lambda n=name,c=command,e=env: run.checked(n,c,e), True)
    return run.finish()


if __name__ == "__main__":
    sys.exit(main())
