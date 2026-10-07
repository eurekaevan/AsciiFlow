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


def qualify(run, kind, frames):
    identity = generate(kind, frames, run.out / ("source-" + kind))
    output = run.out / (kind + ".mp4")
    diagnostic = run.out / (kind + "-diagnostic.json")
    codec, depth = ("h264", "8") if kind == "sdr" else ("hevc", "10")
    dynamic = "sdr" if kind == "pq-to-sdr" else "preserve"
    env = dict(os.environ, ASCIIFLOW_VULKAN_VALIDATION="1",
               ASCIIFLOW_REQUIRE_VULKAN_VALIDATION="1",
               VK_INSTANCE_LAYERS="VK_LAYER_KHRONOS_validation")
    argv = [run.binary, identity["output"]["path"], output,
            "--width", "80", "--charset", "standard", "--font", "builtin-8x8",
            "--color", "true", "--audio", "none", "--decode", "vaapi",
            "--backend", "vulkan", "--vulkan-mapping", "gpu", "--encode", "vaapi",
            "--hw-device", "/dev/dri/renderD128", "--vaapi-vulkan-input-interop", "on",
            "--vaapi-vulkan-output-interop", "on", "--output-codec", codec,
            "--output-bit-depth", depth, "--output-dynamic-range", dynamic,
            "--no-progress", "--diagnostic-report", diagnostic]
    run.checked(kind + "-production", argv, env)
    plan = json.loads(diagnostic.read_text())
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
                "-count_frames", "-show_streams", "-show_packets", "-of", "json", output])
    probe = json.loads((run.out / run.commands[-1]["log"]).read_text())
    stream = probe["streams"][0]
    assert int(stream["nb_read_frames"]) == len(probe["packets"]) == frames
    assert (stream["width"], stream["height"], stream["avg_frame_rate"]) == (1920, 1080, "50/1")
    assert stream["pix_fmt"] == ("yuv420p" if depth == "8" else "yuv420p10le")
    colors = ("bt2020", "smpte2084", "bt2020nc") if kind == "pq-preserve" else ("bt709", "bt709", "bt709")
    assert tuple(stream[key] for key in ("color_primaries", "color_transfer", "color_space")) == colors
    assert stream["color_range"] == "tv"
    tb = Fraction(stream["time_base"])
    for index, packet in enumerate(probe["packets"]):
        assert int(packet["pts"]) * tb == Fraction(index, 50)
        assert int(packet["dts"]) * tb == Fraction(index, 50)
    run.checked(kind + "-full-decode", ["ffmpeg", "-v", "error", "-nostdin", "-xerror",
                "-i", output, "-map", "0:v:0", "-an", "-f", "null", "-"])
    assert not list(run.out.glob("*asciiflow-part*"))
    save(run.out / (kind + "-record.json"), dict(kind=kind, frames=frames,
         input=identity, output=dict(path=str(output), sha256=digest(output)),
         resource_classification="Unresolved", qualification="ShortPreflightOnly"))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/release/asciiflow")
    parser.add_argument("--frames", type=int, default=1000)
    args = parser.parse_args()
    if not 1 <= args.frames <= 10000:
        parser.error("preflight supports 1..10000 frames, not a primary 100k resource soak")
    run = Run(SimpleNamespace(output=args.output, binary=args.binary,
              device="/dev/dri/renderD128", watchdog_seconds=1200, mode="d1-preflight", stack=None))
    run.environment()
    for kind in ("sdr", "pq-preserve", "pq-to-sdr"):
        run.gate(kind, "hardware", lambda kind=kind: qualify(run, kind, args.frames), True)
    status = run.finish()
    if status:
        return status
    # The shared corpus runner permits skipped hardware. A hardware preflight
    # must not signal success when no production path was actually exercised.
    return 0 if all(result["result"] == "PASS" for result in run.results) else 2


if __name__ == "__main__":
    raise SystemExit(main())
