#!/usr/bin/env python3
"""Summarize dedicated production runs without treating overlapping times as additive."""
import argparse
import importlib.util
import json
from pathlib import Path
import re
import statistics
import sys

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("oracle", Path(__file__).with_name("verify-hdr-to-sdr-production.py"))
oracle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(oracle)


def observed(pattern, text):
    match = re.search(pattern, text)
    assert match, f"missing measured scope: {pattern}"
    return float(match.group(1))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("directory", type=Path)
    parser.add_argument("baseline", type=Path)
    parser.add_argument("report", type=Path)
    args = parser.parse_args()
    baseline = json.loads(args.baseline.read_text(encoding="utf-8"))
    report = {"scope": "dedicated three-run production measurements; Validation off; not additive",
              "measurement_feature": "asciiflow-cli/encode-characterization",
              "generator": oracle.file_identity(Path(__file__).with_name("measure-hdr-to-sdr-production.sh")),
              "summarizer": oracle.file_identity(Path(__file__)), "cases": {}}
    for case in ("hevc-h264-8", "hevc-hevc-8", "hevc-av1-8", "hevc-hevc-10", "hevc-av1-10", "av1-hevc-10"):
        source, profile = case.split("-", 1)
        codec, depth = profile.split("-")
        golden = baseline["profiles"][f"{source}-to-{profile}"]["runs"][0]
        records = []
        for index in (1, 2, 3):
            stem = args.directory / f"{case}-run{index}"
            command = json.loads(stem.with_suffix(".command.json").read_text(encoding="utf-8"))
            argv = command["argv"]
            assert argv[3:] == oracle.COMMON + ["--output-codec", codec, "--output-bit-depth", depth, "--no-progress"]
            assert command["input"] == oracle.file_identity(Path(argv[1]))
            assert command["input"] == golden["production_command"]["input"], "measurement input differs from the canonical case"
            assert command["binary"] == oracle.file_identity(Path(argv[0]))
            build = command["build_identity"]
            assert build == oracle.build_identity(Path(argv[0]))
            for field in ("source_sha256", "kernel", "rpm"):
                assert build[field] == baseline["build_identity"][field], f"changed measured {field}"
            output = oracle.file_identity(stem.with_suffix(".mp4"))
            assert output["sha256"] == golden["identity"]["sha256"], "measurement changed output pixels/packets"
            success = json.loads(stem.with_suffix(".success.json").read_text(encoding="utf-8"))
            assert success == {"exit_code": 0, "command_sha256": oracle.file_hash(stem.with_suffix(".command.json")), "output": output}
            log = stem.with_suffix(".log").read_text(encoding="utf-8")
            assert "submitted frames 300 · received packets 300" in log
            assert "hw download 0.000" in log and "hw upload 0.000" in log
            raw_time = stem.with_suffix(".time").read_text(encoding="utf-8").strip()
            values = dict(item.split("=", 1) for item in raw_time.split())
            wall = float(values["wall_seconds"])
            metrics = {
                "whole_wall_seconds": wall, "whole_wall_fps": 300 / wall,
                "cpu_percent": float(values["cpu_percent"].rstrip("%")), "peak_kib": int(values["peak_kib"]),
                "pipeline_fps": observed(r"完成：300 帧，[0-9.]+ 秒，([0-9.]+) FPS", log),
                "decode_wall_ms_per_frame": observed(r"decode ([0-9.]+)", log),
                "input_drm_map_wall_ms_per_frame": observed(r"VAAPI/Vulkan interop CPU wall: DRM map ([0-9.]+)", log),
                "gpu_hdr_map_ms_per_frame": observed(r"Vulkan GPU timestamp:.*mapping ([0-9.]+)", log),
                "gpu_linear_method_limiter_pack_sum_ms_per_frame": observed(r"Vulkan GPU timestamp:.*render ([0-9.]+)", log),
                "output_surface_acquire_wall_ms_per_frame": observed(r"surface acquire ([0-9.]+)", log),
                "gpu_output_copy_ms_per_frame": observed(r"buffer→external image ([0-9.]+)", log),
                "encode_submit_receive_wall_ms_per_frame": observed(r"submit/receive ([0-9.]+)", log),
                "mux_video_write_wall_ms_per_frame": observed(r"video packet write ([0-9.]+)", log),
            }
            records.append({"command": command, "output": output, "successful_run": success,
                            "time": raw_time, "stage_log": log.splitlines(), "metrics": metrics})
        report["cases"][case] = {"runs": records, "mean": {
            field: statistics.mean(record["metrics"][field] for record in records)
            for field in records[0]["metrics"]}}
    oracle.write_json(args.report, report)


if __name__ == "__main__":
    main()
