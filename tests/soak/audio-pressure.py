#!/usr/bin/env python3
"""One bounded full-GPU dual-AAC resource observation, not a long soak."""
import argparse
from fractions import Fraction
import json
import os
from pathlib import Path
import sys
from types import SimpleNamespace

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tests/corpus"))
from run import Run, digest, save, assert_plan_capabilities


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--source-video", type=Path, required=True,
                        help="preserved 10k SDR finite source from generate.py")
    parser.add_argument("--binary", type=Path, required=True)
    args = parser.parse_args()
    run = Run(SimpleNamespace(output=args.output, binary=args.binary,
              device="/dev/dri/renderD128", watchdog_seconds=1200,
              mode="native-dual-aac-resource-observation", stack=None))
    run.environment()
    source = args.source_video.resolve(strict=True)
    run.checked("fixed-generator-version", ["ffmpeg", "-version"])
    if not (run.out / run.commands[-1]["log"]).read_text().startswith("ffmpeg version 8.1.3 "):
        raise ValueError("requires fixed FFmpeg 8.1.3")
    fixture = run.out / "dual-aac-input.mp4"
    run.checked("dual-aac-generation", ["ffmpeg", "-v", "error", "-nostdin", "-n",
        "-i", source, "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000:duration=200",
        "-f", "lavfi", "-i", "sine=frequency=660:sample_rate=48000:duration=200",
        "-map", "0:v:0", "-map", "1:a:0", "-map", "2:a:0", "-c:v", "copy",
        "-c:a", "aac", "-b:a", "128k", "-ar", "48000", "-ac", "2", "-threads", "1",
        "-map_metadata", "-1", "-fflags", "+bitexact", "-flags:a", "+bitexact",
        "-metadata:s:a:0", "language=eng", "-metadata:s:a:1", "language=jpn",
        "-disposition:a:0", "default", "-disposition:a:1", "0",
        "-video_track_timescale", "90000", fixture])
    output = run.out / "output.mp4"
    diagnostic = run.out / "diagnostic.json"
    resources = run.out / "resources.jsonl"
    env = dict(os.environ, ASCIIFLOW_RELIABILITY_REPORT=str(resources),
        ASCIIFLOW_VULKAN_VALIDATION="1", ASCIIFLOW_REQUIRE_VULKAN_VALIDATION="1",
        VK_INSTANCE_LAYERS="VK_LAYER_KHRONOS_validation")
    run.checked("dual-aac-full-gpu", [run.binary, fixture, output,
        "--width", "80", "--charset", "standard", "--font", "builtin-8x8",
        "--color", "true", "--audio", "copy", "--decode", "vaapi", "--backend", "vulkan",
        "--vulkan-mapping", "gpu", "--encode", "vaapi", "--hw-device", "/dev/dri/renderD128",
        "--input-interop", "on", "--output-interop", "on", "--output-codec", "h264",
        "--output-bit-depth", "8", "--output-dynamic-range", "preserve", "--no-progress",
        "--diagnostic-report", diagnostic], env)
    log = (run.out / run.commands[-1]["log"]).read_text()
    assert "完成：10000 帧" in log
    assert not any(x in log for x in ("VUID-", "Validation Error", "SYNC-HAZARD"))
    plan = json.loads(diagnostic.read_text())
    assert_plan_capabilities(plan)
    assert plan["selected_plan"]["hardware_input_interop"] and plan["selected_plan"]["hardware_output_interop"]
    probes = []
    for label, path in (("input", fixture), ("output", output)):
        run.checked(label + "-probe", ["ffprobe", "-v", "error", "-count_frames",
            "-show_streams", "-show_packets", "-show_data_hash", "sha256", "-of", "json", path])
        probes.append(json.loads((run.out / run.commands[-1]["log"]).read_text()))
    video = probes[1]["streams"][0]
    assert (int(video["nb_read_frames"]), video["width"], video["height"], video["avg_frame_rate"]) == (10000, 1920, 1080, "50/1")
    assert (video["color_primaries"], video["color_transfer"], video["color_space"], video["color_range"]) == ("bt709", "bt709", "bt709", "tv")
    counts = []
    for index in (1, 2):
        streams = [p["streams"][index] for p in probes]
        for field in ("codec_name", "sample_rate", "channels", "channel_layout", "disposition", "tags"):
            assert streams[0].get(field) == streams[1].get(field), (index, field)
        def packets(probe, stream):
            tb = Fraction(stream["time_base"])
            return [(p["data_hash"], int(p["size"]),
                *(int(p[k]) * tb for k in ("pts", "dts", "duration")))
                for p in probe["packets"] if p["stream_index"] == index]
        before, after = [packets(p, s) for p, s in zip(probes, streams)]
        assert before == after, f"strict audio packet oracle stream {index}"
        counts.append(len(after))
    run.checked("complete-decode", ["ffmpeg", "-v", "error", "-nostdin", "-xerror", "-i", output, "-f", "null", "-"])
    analysis = run.out / "resource-analysis.json"
    run.checked("resource-trends", [sys.executable, "-B", ROOT / "tests/soak/analyze-resources.py", "--input", resources, "--output", analysis])
    rows = [json.loads(l) for l in resources.read_text().splitlines()]
    assert rows[0]["fd_count"] == rows[-1]["fd_count"]
    assert rows[-1]["frames_processed"] == 10000
    assert not rows[-1]["accounting_errors"]
    assert all(r["active_count"] == 0 for r in rows[-1]["resources"].values())
    resource_analysis = json.loads(analysis.read_text())
    save(run.out / "audio-pressure.json", dict(classification="NativePass",
        qualification=resource_analysis["qualification"], source_video=dict(path=str(source), sha256=digest(source)),
        input=dict(path=str(fixture), sha256=digest(fixture)), output=dict(path=str(output), sha256=digest(output)),
        audio_packet_counts=counts, strict_audio_packet_oracle="PASS", frame_count=10000,
        resources_sha256=digest(resources), analysis=resource_analysis,
        initial=rows[0], final=rows[-1]))
    return run.finish()


if __name__ == "__main__":
    raise SystemExit(main())
