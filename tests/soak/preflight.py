#!/usr/bin/env python3
"""Short D-1 path/Validation preflight using the existing corpus watchdog."""
import argparse
from fractions import Fraction
import json
import os
from pathlib import Path
import sys
from types import SimpleNamespace

from generate import generate

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tests/corpus"))
from run import Run, assert_plan_capabilities, digest, save


def verify_source_path(kind, identity, diagnostic):
    assert identity["kind"] == kind
    codec, depth, dynamic, processing = {
        "sdr": ("H264", 8, "Sdr", "Sdr"),
        "pq-preserve": ("Hevc", 10, "HdrPq", "HdrPqPreserve"),
        "pq-to-sdr": ("Av1", 10, "HdrPq", "HdrPqToSdrBt709"),
    }[kind]
    source = diagnostic["input_requirements"]
    assert (source["codec"], source["bit_depth"], source["dynamic_range"]) == (codec, depth, dynamic)
    assert (source["width"], source["height"]) == (1920, 1080)
    assert source["frame_rate"] == {"numerator": 50, "denominator": 1}
    assert diagnostic["selected_plan"]["color_processing"] == processing


def qualify(run, kind, frames, measurement=False, *, validation=True,
            qualification="ShortPreflightOnly", audio_tracks=0, input_identity=None):
    identity = input_identity or generate(kind, frames, run.out / ("source-" + kind), audio_tracks)
    assert identity["kind"] == kind
    assert identity["frames"] == frames and identity.get("audio_tracks", 0) == audio_tracks
    assert digest(Path(identity["output"]["path"])) == identity["output"]["sha256"]
    output = run.out / (kind + ".mp4")
    diagnostic = run.out / (kind + "-diagnostic.json")
    codec, depth = ("h264", "8") if kind == "sdr" else ("hevc", "10")
    dynamic = "sdr" if kind == "pq-to-sdr" else "preserve"
    env = dict(os.environ)
    if validation:
        env.update(ASCIIFLOW_VULKAN_VALIDATION="1",
                   ASCIIFLOW_REQUIRE_VULKAN_VALIDATION="1",
                   VK_INSTANCE_LAYERS="VK_LAYER_KHRONOS_validation")
    else:
        # Long production observations must not inherit Validation residency.
        for name in ("ASCIIFLOW_VULKAN_VALIDATION", "ASCIIFLOW_REQUIRE_VULKAN_VALIDATION",
                     "VK_INSTANCE_LAYERS", "ASCIIFLOW_RELIABILITY_REPORT"):
            env.pop(name, None)
    env["ASCIIFLOW_ATTEST_NATIVE_MAPS"] = "1"
    resources = run.out / (kind + "-resources.jsonl")
    if measurement:
        env["ASCIIFLOW_RELIABILITY_REPORT"] = str(resources)
    argv = [run.binary, identity["output"]["path"], output,
            "--width", "80", "--charset", "standard", "--font", "builtin-8x8",
            "--color", "true", "--audio", "copy" if audio_tracks else "none", "--decode", "vaapi",
            "--backend", "vulkan", "--vulkan-mapping", "gpu", "--encode", "vaapi",
            "--hw-device", "/dev/dri/renderD128", "--vaapi-vulkan-input-interop", "on",
            "--vaapi-vulkan-output-interop", "on", "--output-codec", codec,
            "--output-bit-depth", depth, "--output-dynamic-range", dynamic,
            "--no-progress", "--diagnostic-report", diagnostic]
    run.checked(kind + "-production", argv, env)
    production_command = dict(run.commands[-1])
    plan = json.loads(diagnostic.read_text())
    verify_source_path(kind, identity, plan)
    assert_plan_capabilities(plan)
    selected = plan["selected_plan"]
    assert selected["hardware_input_interop"] and selected["hardware_output_interop"]
    assert selected["decode"] == selected["encode"] == "Hardware"
    assert selected["backend"] == "Vulkan"
    log = run.out / run.commands[-1]["log"]
    text = log.read_text()
    assert f"完成：{frames} 帧" in text
    assert not any(token in text for token in ("Validation Error", "VUID-", "SYNC-HAZARD"))
    run.checked(kind + "-probe", ["ffprobe", "-v", "error", "-select_streams", "v:0",
                "-count_packets", "-show_streams", "-show_packets", "-of", "json", output])
    probe = json.loads((run.out / run.commands[-1]["log"]).read_text())
    stream = probe["streams"][0]
    assert int(stream["nb_read_packets"]) == len(probe["packets"]) == frames
    assert (stream["width"], stream["height"], stream["avg_frame_rate"]) == (1920, 1080, "50/1")
    assert stream["pix_fmt"] == ("yuv420p" if depth == "8" else "yuv420p10le")
    colors = ("bt2020", "smpte2084", "bt2020nc") if kind == "pq-preserve" else ("bt709", "bt709", "bt709")
    assert tuple(stream[key] for key in ("color_primaries", "color_transfer", "color_space")) == colors
    assert stream["color_range"] == "tv"
    tb = Fraction(stream["time_base"])
    assert int(stream["duration_ts"]) * tb == Fraction(frames, 50)
    for index, packet in enumerate(probe["packets"]):
        assert int(packet["pts"]) * tb == Fraction(index, 50)
        assert int(packet["dts"]) * tb == Fraction(index, 50)
        assert int(packet["duration"]) * tb == Fraction(1, 50)
    run.checked(kind + "-full-decode", ["ffmpeg", "-v", "error", "-nostdin", "-xerror",
                "-i", output, "-map", "0", "-progress", "pipe:1", "-nostats", "-f", "null", "-"])
    decode_log = (run.out / run.commands[-1]["log"]).read_text()
    progress = dict(line.split("=", 1) for line in decode_log.splitlines() if "=" in line)
    assert int(progress["frame"]) == frames and progress["progress"] == "end"
    audio_result = None
    if audio_tracks:
        from long_audio import verify_audio
        audio_result = verify_audio(run, kind, Path(identity["output"]["path"]), output, audio_tracks)
    assert not list(run.out.glob("*asciiflow-part*"))
    resource_record = None
    if measurement:
        analysis = run.out / (kind + "-resource-analysis.json")
        run.checked(kind + "-resources", [sys.executable,
                    ROOT / "tests/soak/analyze-resources.py", "--input", resources,
                    "--output", analysis])
        rows = [json.loads(line) for line in resources.read_text().splitlines()]
        assert rows[0]["phase"] == "initial" and rows[-1]["phase"] == "post-cleanup"
        # The mux counter includes copied audio, unlike the video frame count.
        from long_summary import verify_mux_counts
        verify_mux_counts(rows[-1], frames, audio_result["packet_counts"] if audio_result else [])
        assert not rows[-1]["accounting_errors"]
        assert all(resource["active_count"] == 0 for resource in rows[-1]["resources"].values())
        assert rows[0]["fd_count"] == rows[-1]["fd_count"]
        resource_record = dict(samples=str(resources), samples_sha256=digest(resources),
                               analysis=json.loads(analysis.read_text()),
                               fd_initial=rows[0]["fd_count"], fd_final=rows[-1]["fd_count"])
        if frames >= 100000:
            from long_summary import from_files
            resource_record["long_job_summary"] = from_files(
                resources, run.out / production_command["process_samples"], production_command["elapsed_seconds"])
    save(run.out / (kind + "-record.json"), dict(kind=kind, frames=frames,
         input=identity, output=dict(path=str(output), sha256=digest(output)),
         audio=audio_result,
         production_command=production_command,
         resources=resource_record,
         resource_classification="Unresolved", qualification=qualification,
         validation=validation, measurement=measurement))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/release/asciiflow")
    parser.add_argument("--frames", type=int, default=1000)
    parser.add_argument("--measurement", action="store_true",
                        help="requires a reliability-measurement feature binary")
    args = parser.parse_args()
    if not 1 <= args.frames <= 10000:
        parser.error("preflight supports 1..10000 frames, not a primary 100k resource soak")
    run = Run(SimpleNamespace(output=args.output, binary=args.binary,
              device="/dev/dri/renderD128", watchdog_seconds=1200, mode="d1-preflight", stack=None))
    run.environment()
    for kind in ("sdr", "pq-preserve", "pq-to-sdr"):
        run.gate(kind, "hardware", lambda kind=kind: qualify(run, kind, args.frames, args.measurement), True)
    status = run.finish()
    if status:
        return status
    # The shared corpus runner permits skipped hardware. A hardware preflight
    # must not signal success when no production path was actually exercised.
    return 0 if all(result["result"] == "PASS" for result in run.results) else 2


if __name__ == "__main__":
    raise SystemExit(main())
