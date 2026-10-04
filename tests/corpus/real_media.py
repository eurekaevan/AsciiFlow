"""Real-media execution surfaces layered on the corpus runner's durable commands."""
from copy import deepcopy
from fractions import Fraction
import hashlib
import importlib.util
import json
import os
from pathlib import Path

from run import assert_plan_capabilities, digest, load, outcome, save
import timing


_spec = importlib.util.spec_from_file_location("real_manifest", Path(__file__).with_name("build-real-manifest.py"))
_manifest = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_manifest)


def validate_media_facts(fixture, probe):
    """Use the manifest builder's single fact model before production admission."""
    frames = entries(probe, "frame") if probe is not None else []
    decoded_video = any(frame.get("media_type") == "video" for frame in frames)
    if fixture["media"] is None:
        if decoded_video:
            raise ValueError("null media facts conceal decoded video")
        return
    if not decoded_video:
        raise ValueError("manifest media facts have no decoded video evidence")
    actual, _, _ = _manifest.describe(probe, fixture["source"]["path"])
    if actual != fixture["media"]:
        differing = sorted(key for key in actual if actual[key] != fixture["media"].get(key))
        raise ValueError(f"actual media facts mismatch: {differing}")


def normalized_probe(document):
    """Normalize only the location-dependent filename; keep every observed field."""
    value = deepcopy(document)
    if value.get("format", {}).get("filename"):
        value["format"]["filename"] = Path(value["format"]["filename"]).name
    return value


def probe_digest(document):
    payload = json.dumps(normalized_probe(document), sort_keys=True, separators=(",", ":"), ensure_ascii=False)
    return hashlib.sha256(payload.encode("utf-8")).hexdigest()


def entries(document, kind):
    return document.get(kind + "s", [entry for entry in document.get("packets_and_frames", [])
                                     if entry.get("type") == kind])


def video_only(document):
    value = deepcopy(document)
    indices = {stream["index"] for stream in value["streams"] if stream["codec_type"] == "video"}
    value["streams"] = [stream for stream in value["streams"] if stream["index"] in indices]
    for key in ("frames", "packets", "packets_and_frames"):
        if key in value:
            value[key] = [entry for entry in value[key] if entry.get("stream_index") in indices]
    return value


def characterize_audio_limits(source, output, limits):
    """Explain registered unqualified observations; never replace the strict oracle."""
    reports = []
    before = [s for s in source["streams"] if s["codec_type"] == "audio"]
    after = [s for s in output["streams"] if s["codec_type"] == "audio"]
    assert len(before) == len(after) and before
    for original, copied in zip(before, after):
        for key in ("codec_name", "sample_rate", "channels", "channel_layout"):
            assert original.get(key) == copied.get(key), (key, original.get(key), copied.get(key))
        language = original.get("tags", {}).get("language")
        out_language = copied.get("tags", {}).get("language")
        if language != out_language:
            assert "undefined_language" in limits and language is None and out_language == "und"
        default = original.get("disposition", {}).get("default", 0)
        out_default = copied.get("disposition", {}).get("default", 0)
        if default != out_default:
            assert "default_disposition" in limits and default == 0 and out_default == 1
        src = [p for p in entries(source, "packet") if p["stream_index"] == original["index"]]
        dst = [p for p in entries(output, "packet") if p["stream_index"] == copied["index"]]
        assert src and len(src) == len(dst)
        src_tick, dst_tick = Fraction(original["time_base"]), Fraction(copied["time_base"])
        differences, missing = [], []
        for index, (left, right) in enumerate(zip(src, dst)):
            assert left.get("data_hash", "").startswith("SHA256:")
            assert left["data_hash"] == right["data_hash"] and left["size"] == right["size"]
            for field in ("pts", "dts"):
                assert field in left and field in right
                assert int(left[field]) * src_tick == int(right[field]) * dst_tick
            assert "duration" in right
            actual_duration = int(right["duration"]) * dst_tick
            if "duration" not in left:
                assert "missing_first_duration" in limits and index == 0 and len(src) > 1
                assert actual_duration == (int(src[1]["pts"]) - int(left["pts"])) * src_tick
                missing.append(index)
                continue
            error = abs(int(left["duration"]) * src_tick - actual_duration)
            bound = src_tick + dst_tick if "millisecond_duration" in limits else dst_tick
            assert error <= bound, (index, error, bound)
            if error > dst_tick:
                differences.append({"packet": index, "difference_seconds": str(error)})
        reports.append({"packets": len(src), "payload_sha256_size_order": "identical",
                        "pts_dts": "exactly identical rational timestamps",
                        "language": [language, out_language], "default": [default, out_default],
                        "source_tags": original.get("tags", {}), "output_tags": copied.get("tags", {}),
                        "strict_duration_mismatches": differences, "missing_source_duration_packets": missing,
                        "observation_bound_seconds": str(src_tick + dst_tick),
                        "qualification": "Unqualified; original strict oracle failure retained"})
    return reports


def audio_titles(document):
    """Container spelling differs; the actual display label must not be lost."""
    result = []
    for stream in document["streams"]:
        if stream["codec_type"] == "audio":
            tags = {key.lower(): value for key, value in stream.get("tags", {}).items()}
            result.append(tags.get("title", tags.get("name")))
    return result


class RealMediaMixin:
    def fixture_path(self, fixture):
        source = fixture["source"]
        group = source.get("generation_group")
        if source["kind"] != "generated" or not group:
            return super().fixture_path(fixture)
        self.validate_source_recipe(fixture)
        basename = Path(source["path"]).name
        cached_dir = getattr(self.args, "generated_inputs", None)
        if cached_dir is not None and (Path(cached_dir).resolve() / basename).is_file():
            path = Path(cached_dir).resolve() / basename
            mode = "identity-checked generated input reuse"
        else:
            if Path(group).name != group or group in (".", ".."):
                raise ValueError(f"unsafe generation group: {group}")
            directory = self.out / "generated" / group
            directory.parent.mkdir(exist_ok=True)
            recipe = tuple(source["exact_command"])
            groups = self.__dict__.setdefault("real_generation_groups", {})
            if group not in groups:
                if "{output_directory}" not in recipe:
                    raise ValueError("generation group requires {output_directory} recipe")
                argv = [str(directory) if item == "{output_directory}" else item for item in recipe]
                self.checked(f"generate-group-{group}", argv,
                             timeout=getattr(self.args, "generation_watchdog_seconds", 2400))
                groups[group] = recipe
            elif groups[group] != recipe:
                raise ValueError(f"conflicting recipes for generation group {group}")
            path = directory / basename
            mode = "generated once per identity-checked recipe group"
        if not path.is_file() or path.stat().st_size != source["byte_size"] or digest(path) != source["sha256"]:
            raise ValueError(f"{fixture['id']}: generated input identity mismatch")
        self.fixture_setups.append({"fixture": fixture["id"], "mode": mode,
                                    "path": str(path), "sha256": digest(path), "generation_group": group})
        return path

    def real_probe(self, name, path, allow_failure=False):
        stderr = self.out / f"{name}-stderr.log"
        success = self.command(name, ["ffprobe", "-v", "error", "-show_format", "-show_streams",
                                      "-show_frames", "-show_packets", "-show_data_hash", "sha256",
                                      "-of", "json", path], stderr_log=stderr)
        log = self.out / self.commands[-1]["log"]
        logs = {"log": log.name, "log_sha256": digest(log),
                "stderr": stderr.name, "stderr_sha256": digest(stderr)}
        if not success:
            if allow_failure:
                return None, {**logs, "probe": "failed"}
            raise ValueError(f"{name}: full ffprobe failed; see {log.name}")
        document = load(log)
        normalized = normalized_probe(document)
        retained = self.out / f"{name}.json"
        save(retained, normalized)
        return document, {**logs, "path": retained.name, "sha256": digest(retained),
                          "normalized_probe_sha256": probe_digest(document)}

    def real_options(self, fixture):
        request = fixture["request"]
        route = request.get("route", "hardware" if request["bit_depth"] == 10 else "portable")
        options = ["--audio", request.get("audio", "none"), "--output-codec", request["codec"],
                   "--output-bit-depth", str(request["bit_depth"]),
                   "--output-dynamic-range", request["dynamic_range"], "--hw-device", self.args.device,
                   "--width", "16" if route == "portable" else "80", "--charset", "standard",
                   "--font", "builtin-8x8", "--color", "true", "--max-frames", "0"]
        if route == "portable":
            options += ["--backend", "cpu", "--decode", "software", "--encode", "software",
                        "--vaapi-vulkan-input-interop", "off", "--vaapi-vulkan-output-interop", "off"]
        else:
            options += ["--backend", "vulkan", "--decode", "vaapi", "--encode", "vaapi",
                        "--vulkan-mapping", "gpu", "--vaapi-vulkan-input-interop", "on",
                        "--vaapi-vulkan-output-interop", "on"]
        return route, options

    @staticmethod
    def real_classification(diagnostic, expected):
        failure = diagnostic.get("failure") or {}
        value = (diagnostic.get("input_requirements") or {}).get("dynamic_range")
        if failure.get("code") in ("Conflicting", "Unknown"):
            value = failure["code"]
        if value is None and not failure:
            raise ValueError("successful structured diagnostics omitted input classification")
        if value is not None and value != expected["classification"]:
            raise ValueError(f"unexpected resolved classification: {value}, expected {expected['classification']}")
        return value

    def fixture(self, fixture):
        # Keep established retained fixtures on their existing execution surface.
        if not fixture["source"].get("generation_group"):
            return super().fixture(fixture)
        name, expected = fixture["id"], fixture["expected"]
        command_start = len(self.commands)
        path = self.fixture_path(fixture)
        route, options = self.real_options(fixture)
        positive = expected["runtime"] == "Pass"
        execution_failure = expected["runtime"] in ("RejectAtInitialization", "RuntimeFailureExpected")
        source_probe, source_identity = self.real_probe(f"{name}-input-probe", path, allow_failure=not positive)
        expected_probe = fixture["source"].get("probe_sha256")
        if expected_probe and (source_probe is None or probe_digest(source_probe) != expected_probe):
            raise ValueError(f"{name}: actual full probe identity mismatch")
        validate_media_facts(fixture, source_probe)
        evidence = {"input_sha256": digest(path), "input_probe": source_identity,
                    "candidate_status": expected["status"], "blocker": expected.get("blocker"),
                    "expected_rejection": not positive, "route": route}
        if route == "hardware" and self.skip_hardware and (positive or execution_failure or expected["failure_stage"] != "InputProbe"):
            return {**evidence, "skipped": True, "reason": self.skip_hardware,
                    "actual_compatibility_outcome": "SKIPPED",
                    "commands": self.commands[command_start:]}
        plan_path = self.out / f"{name}-plan.json"
        plan_success = self.command(f"plan-{name}", [self.binary, path, "--explain-plan", "--capabilities",
                                                      *options, "--diagnostic-report", plan_path])
        diagnostic = load(plan_path)
        failure = diagnostic.get("failure") or {}
        plan_error = None
        if positive or execution_failure:
            if not plan_success:
                plan_error = f"{name}: plan rejected before execution: {failure}"
            else:
                assert_plan_capabilities(diagnostic)
        elif outcome(expected, plan_success, failure.get("stage"), failure.get("category")) != "PASS":
            plan_error = f"{name}: unexpected planning outcome: {failure}"
        if positive and plan_error:
            raise ValueError(plan_error)
        evidence.update(diagnostic=plan_path.name, diagnostic_sha256=digest(plan_path),
                        classification={"observed": (diagnostic.get("input_requirements") or {}).get("dynamic_range"),
                                        "scope": "structured first-frame diagnostics"},
                        selected_plan=diagnostic.get("selected_plan"))
        destination = self.out / f"{name}-output.mp4"
        sentinel = b"AsciiFlow corpus failure must preserve this destination.\n"
        if not positive:
            with destination.open("xb") as stream:
                stream.write(sentinel)
        runtime_path = self.out / f"{name}-runtime.json"
        runtime_success = self.command(f"runtime-{name}", [self.binary, path, destination, *options,
                                                           "--no-progress", "--diagnostic-report", runtime_path])
        if not positive:
            preserved = destination.read_bytes() == sentinel
            staging = sorted(item.name for item in self.out.iterdir() if "asciiflow-part" in item.name)
            transaction = {"sentinel_preserved": preserved, "staging_files": staging,
                           "sentinel_sha256": digest(destination)}
            save(self.out / f"{name}-transaction.json", transaction)
            evidence["transaction"] = transaction
            assert preserved, "fatal conversion changed existing destination"
            assert not staging, "fatal conversion left staging files"
        actual = load(runtime_path)
        failure = actual.get("failure") or {}
        evidence.update(runtime_diagnostic=runtime_path.name, runtime_diagnostic_sha256=digest(runtime_path),
                        actual_plan=actual.get("selected_plan"))
        if plan_error:
            raise ValueError(plan_error)
        evidence["classification"]["observed"] = self.real_classification(diagnostic, expected)
        if outcome(expected, runtime_success, failure.get("stage"), failure.get("category")) != "PASS":
            raise ValueError(f"{name}: unexpected conversion outcome: {failure}")
        evidence["runtime_classification"] = self.real_classification(actual, expected)
        if positive:
            assert actual["plan_scope"] == "initialized_execution"
            assert_plan_capabilities(actual)
            actual_rate = actual["input_requirements"]["frame_rate"]
            if isinstance(actual_rate, dict):
                actual_rate = f"{actual_rate['numerator']}/{actual_rate['denominator']}"
            if not actual_rate or Fraction(actual_rate) <= 0:
                raise ValueError("initialized execution omitted valid frame rate")
            evidence["initialized_frame_rate"] = actual_rate
            output_probe, output_identity = self.real_probe(f"{name}-output-probe", destination)
            if fixture["request"].get("audio", "none") == "none":
                assert not any(stream["codec_type"] == "audio" for stream in output_probe["streams"]), "audio=none retained audio"
            evidence["output_probe"] = output_identity
            evidence["output_sha256"] = digest(destination)
            assert source_probe is not None
            audio_expected = fixture["request"].get("audio", "none") != "none"
            source_timing = source_probe if audio_expected else video_only(source_probe)
            output_timing = output_probe if audio_expected else video_only(output_probe)
            timing_result = timing.compare(source_timing, output_timing, actual_rate,
                                           expected.get("video_timing", "cfr_zero"))
            timing_path = self.out / f"{name}-timing.json"
            save(timing_path, timing_result)
            evidence["timing"] = {"path": timing_path.name, "sha256": digest(timing_path)}
            evidence["decode_back"] = self.real_decode_back(fixture, destination, output_probe, actual_rate)
            if audio_expected and any(stream["codec_type"] == "audio" for stream in source_probe["streams"]):
                assert audio_titles(source_probe) == audio_titles(output_probe), "audio display titles changed or were lost"
                evidence["audio_titles"] = audio_titles(output_probe)
                limits = expected.get("audio_limits")
                env = dict(os.environ, ASCIIFLOW_AUDIO_REFERENCE=str(path), ASCIIFLOW_AUDIO_CANDIDATE=str(destination))
                oracle_report = self.out / f"{name}-strict-audio-failure.json"
                if limits:
                    env["ASCIIFLOW_AUDIO_ORACLE_REPORT"] = str(oracle_report)
                passed = self.command(f"audio-oracle-{name}", ["cargo", "test", "-p", "asciiflow-cli", "--test", "audio_regression",
                                                       "retained_audio_pair_from_env", "--", "--ignored", "--exact", "--nocapture"], env)
                evidence["audio_oracle"] = {"command": f"audio-oracle-{name}", "scope": "unchanged strict retained audio oracle",
                                           "result": "PASS" if passed else "FAILED",
                                           "log_sha256": digest(self.out / self.commands[-1]["log"]),
                                           "environment": {"ASCIIFLOW_AUDIO_REFERENCE": str(path),
                                                           "ASCIIFLOW_AUDIO_CANDIDATE": str(destination)}}
                if limits:
                    assert expected["status"] == "Unqualified" and expected.get("blocker")
                    assert not passed, "registered limitation unexpectedly passed; review before changing qualification"
                    report = load(oracle_report)
                    expected_code = ("MetadataMismatch" if any(limit in limits for limit in
                                     ("undefined_language", "default_disposition")) else "TimestampMismatch")
                    assert report == {"schema_version": 1, "test": "retained_audio_pair_from_env",
                                      "outcome": "STRICT_FAILURE", "code": expected_code,
                                      "reference": str(path), "candidate": str(destination)}, report
                    evidence["audio_oracle"]["structured_failure"] = {"path": oracle_report.name,
                                                                     "sha256": digest(oracle_report), "code": expected_code}
                    evidence["audio_limitation"] = characterize_audio_limits(source_probe, output_probe, limits)
                    for label, media_path in (("source", path), ("output", destination)):
                        self.checked(f"full-audio-decode-{name}-{label}",
                                     ["ffmpeg", "-v", "error", "-xerror", "-err_detect", "explode", "-nostdin",
                                      "-i", media_path, "-map", "0:a", "-vn", "-sn", "-f", "null", "-"])
                    evidence["audio_limitation_full_decode"] = "source and output passed separately"
                    evidence["unqualified"] = True
                elif not passed:
                    raise ValueError(f"audio-oracle-{name}: nonzero exit; inspect command log")
        if expected["status"] == "Unqualified":
            assert expected.get("blocker"), "unqualified case requires an explicit blocker"
            evidence["unqualified"] = True
        evidence["commands"] = self.commands[command_start:]
        evidence["actual_compatibility_outcome"] = ("UNQUALIFIED" if evidence.get("unqualified") else
                                                    "EXPECTED REJECT" if not positive else "PASS")
        evidence["qualification_claim"] = "candidate evidence; unqualified" if expected["status"] == "Unqualified" else expected["status"]
        return evidence

    def real_decode_back(self, fixture, output, probe, actual_rate):
        name, media, request = fixture["id"], fixture["media"], fixture["request"]
        streams = [stream for stream in probe["streams"] if stream["codec_type"] == "video"]
        frames = [frame for frame in entries(probe, "frame") if frame.get("media_type") == "video"]
        assert len(streams) == 1
        stream = streams[0]
        assert stream["codec_name"] == request["codec"]
        if request["codec"] in ("hevc", "av1"):
            assert stream["profile"] == ("Main 10" if request["codec"] == "hevc" and request["bit_depth"] == 10 else "Main")
        assert len(frames) == media["timing"]["frame_count"]
        pixel = "yuv420p10le" if request["bit_depth"] == 10 else "yuv420p"
        pq = fixture["expected"]["classification"] == "HdrPq" and request["dynamic_range"] == "preserve"
        colors = {"color_primaries": "bt2020" if pq else "bt709", "color_transfer": "smpte2084" if pq else "bt709",
                  "color_space": "bt2020nc" if pq else "bt709", "color_range": "tv"}
        for value in [stream, *frames]:
            assert (value["width"], value["height"], value["pix_fmt"]) == (media["width"], media["height"], pixel)
            assert all(value.get(key) == wanted for key, wanted in colors.items()), (name, value)
            assert value.get("chroma_location") == "left"
            # The existing canonical PQ output also excludes propagation or
            # recomputation of source mastering/MaxCLL/dynamic HDR side data.
            assert not any(any(marker in side["side_data_type"].lower() for marker in
                               ("mastering", "content light", "hdr", "dovi", "dolby vision"))
                           for side in value.get("side_data_list", []))
        assert Fraction(stream["avg_frame_rate"]) == Fraction(actual_rate)
        self.checked(f"decode-back-full-{name}", ["ffmpeg", "-v", "error", "-xerror", "-nostdin", "-i", output,
                                                  "-map", "0:v:0", "-an", "-pix_fmt", pixel,
                                                  "-f", "framehash", "-hash", "sha256", "-"])
        log = self.out / self.commands[-1]["log"]
        decoded = [line for line in log.read_text().splitlines() if line.strip() and not line.startswith("#")]
        assert len(decoded) == len(frames)
        return {"frames": len(frames), "codec": request["codec"], "pixel_format": pixel,
                "decoded_hashes": {"path": log.name, "sha256": digest(log)},
                "scope": "full video decode, geometry, depth, color; audio checked by retained oracle"}
