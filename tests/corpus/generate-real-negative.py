#!/usr/bin/env python3
"""Build reproducible encoded corruption/timeline candidates; never qualification.

Requires FFmpeg/ffprobe 8.1.3 and a fresh output directory. Timestamp mutations
touch only video block timestamps in CRC-free Matroska, never audio packets.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[2]
MASTERS = {0x18538067, 0x1F43B675, 0xA0}  # Segment, Cluster, BlockGroup


def identity(path):
    return {"path": str(path), "bytes": path.stat().st_size,
            "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}


def save(path, value):
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def vint(data, offset, end, identifier=False):
    if offset >= end or data[offset] == 0:
        raise ValueError("invalid/truncated EBML variable integer")
    first = data[offset]
    length = 1
    marker = 0x80
    while not first & marker:
        length += 1
        marker >>= 1
    if length > (4 if identifier else 8) or offset + length > end:
        raise ValueError("oversized/truncated EBML integer")
    value = int.from_bytes(data[offset:offset + length], "big")
    if not identifier:
        value &= (1 << (7 * length)) - 1
        if value == (1 << (7 * length)) - 1:
            value = None
    return value, offset + length


def video_blocks(data):
    """Walk bounded known EBML masters; return video track-1 timestamp fields."""
    blocks = []
    elements = 0

    def walk(start, end, depth=0):
        nonlocal elements
        if depth > 3:
            raise ValueError("unexpected EBML nesting")
        offset = start
        while offset < end:
            elements += 1
            if elements > 100000:
                raise ValueError("EBML element limit exceeded")
            identifier, cursor = vint(data, offset, end, True)
            size, cursor = vint(data, cursor, end)
            limit = end if size is None else cursor + size
            if limit > end or limit <= offset:
                raise ValueError("EBML payload exceeds parent")
            if identifier in MASTERS:
                walk(cursor, limit, depth + 1)
            elif identifier in (0xA1, 0xA3):
                track, timestamp = vint(data, cursor, limit)
                if timestamp + 3 > limit:
                    raise ValueError("truncated Matroska block header")
                if data[timestamp + 2] & 6:
                    raise ValueError("laced blocks are outside this generator")
                if track == 1:
                    blocks.append(timestamp)
            offset = limit
    walk(0, len(data))
    return blocks


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("out_dir", type=Path)
    parser.add_argument("--ffmpeg", default="ffmpeg")
    parser.add_argument("--ffprobe", default="ffprobe")
    parser.add_argument("--watchdog-seconds", type=int, default=120)
    args = parser.parse_args()
    if args.watchdog_seconds <= 0:
        parser.error("watchdog must be positive")
    out = args.out_dir.resolve()
    out.mkdir(parents=False, exist_ok=False)
    inventory = {"stage": "real-media-candidates", "qualification": "candidate-only",
                 "script": identity(Path(__file__).resolve()), "tools": {},
                 "commands": [], "artifacts": [], "limitations": []}

    def run(argv, required=True):
        record = {"argv": list(map(str, argv)), "cwd": str(ROOT)}
        inventory["commands"].append(record)
        try:
            result = subprocess.run(record["argv"], cwd=ROOT,
                                    env={**os.environ, "LC_ALL": "C"},
                                    capture_output=True, text=True, errors="replace",
                                    timeout=args.watchdog_seconds)
        except subprocess.TimeoutExpired:
            record["failure"] = "watchdog-expired"
            raise
        record.update(returncode=result.returncode, stdout=result.stdout, stderr=result.stderr)
        if required and result.returncode:
            raise RuntimeError(f"command failed: {record['argv']}: {result.stderr}")
        return result

    try:
        for name, requested in (("ffmpeg", args.ffmpeg), ("ffprobe", args.ffprobe)):
            executable = Path(shutil.which(requested) or requested).resolve(strict=True)
            version = run([executable, "-version"]).stdout
            if not version.startswith(f"{name} version 8.1.3 "):
                raise RuntimeError(f"expected {name} 8.1.3")
            inventory["tools"][name] = {**identity(executable), "version_and_configuration": version}
        ffmpeg, ffprobe = (inventory["tools"][name]["path"] for name in ("ffmpeg", "ffprobe"))
        common = [ffmpeg, "-nostdin", "-hide_banner", "-loglevel", "error", "-n"]
        audio_tags = ["-metadata:s:a:0", "language=eng", "-metadata:s:a:0", "title=negative-source"]
        inventory["limitations"].append(
            "Earlier untagged source.mkv characterization failed the strict language oracle "
            "because missing Matroska language became MP4 und. These new media identities "
            "explicitly use eng; they do not qualify preservation of absent language metadata.")
        base = out / "negative-source.mp4"
        run([*common, "-f", "lavfi", "-i", "testsrc2=size=128x96:rate=25:duration=2",
             "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000:duration=2",
             "-map", "0:v", "-map", "1:a", "-c:v", "libx264", "-preset", "fast",
             "-profile:v", "high", "-pix_fmt", "yuv420p", "-bf", "0", "-g", "25",
             "-threads:v", "1", "-x264-params", "threads=1:lookahead_threads=1:scenecut=0",
             "-color_primaries", "bt709", "-color_trc", "bt709", "-colorspace", "bt709",
             "-color_range", "tv", "-c:a", "aac", "-b:a", "96k", "-ac", "2",
             "-threads:a", "1", "-flags:a", "+bitexact", "-map_metadata", "-1",
             *audio_tags, "-fflags", "+bitexact", "-movflags", "+faststart", base])
        mkv = out / "negative-source.mkv"
        run([*common, "-i", base, "-map", "0:v:0", "-map", "0:a:0", "-c", "copy",
             "-map_metadata", "-1", *audio_tags, "-fflags", "+bitexact", "-write_crc32", "0", mkv])

        def observe(path, mutation):
            probes = {}
            observed = {}
            for kind, flags in (("streams", ["-show_format", "-show_streams"]),
                                ("frames", ["-show_frames"]), ("packets", ["-show_packets"])):
                result = run([ffprobe, "-v", "error", *flags, "-of", "json", path], False)
                try:
                    probe = json.loads(result.stdout)
                except json.JSONDecodeError:
                    probe = None
                if probe and "format" in probe:
                    probe["format"]["filename"] = path.name
                target = out / f"{path.name}.{kind}.json"
                save(target, probe)
                probes[kind] = {**identity(target), "path": target.name,
                                "returncode": result.returncode, "stderr": result.stderr}
                observed[kind] = probe
            decoded = run([*common[:-1], "-i", path, "-map", "0:v:0", "-an",
                           "-fps_mode", "passthrough", "-f", "null", "-"], False)
            frames = (observed["frames"] or {}).get("frames", [])
            packets = (observed["packets"] or {}).get("packets", [])
            pts = [frame.get("pts") for frame in frames if frame.get("media_type") == "video"]
            video_packets = [packet for packet in packets if packet.get("stream_index") == 0]
            entry = {**identity(path), "path": path.name, "mutation": mutation,
                     "probes": probes, "observed_streams": (observed["streams"] or {}).get("streams"),
                     "decoded_video_pts": pts, "video_packet_pts": [p.get("pts") for p in video_packets],
                     "decode": {"returncode": decoded.returncode, "stderr": decoded.stderr}}
            inventory["artifacts"].append(entry)
            return entry, observed

        base_entry, base_probe = observe(base, {"kind": "encoded-source"})
        if len(base_entry["decoded_video_pts"]) != 50:
            raise RuntimeError("source did not decode exactly 50 frames")
        mkv_entry, mkv_probe = observe(mkv, {"kind": "encoded-remux", "video_track": 1})
        raw = base.read_bytes()
        for label, cut in (("begin", 16), ("middle", len(raw) // 2), ("end", len(raw) - 128)):
            path = out / f"truncated-{label}.mp4"
            path.write_bytes(raw[:cut])
            observe(path, {"kind": "truncate", "source": base.name, "keep_bytes": cut,
                           "removed_bytes": len(raw) - cut})
        packet = [p for p in base_probe["packets"]["packets"] if p["stream_index"] == 0][20]
        position, size = int(packet["pos"]), int(packet["size"])
        if size < 8 or position < 0 or position + size > len(raw):
            raise RuntimeError("packet does not provide a safe encoded payload mutation")
        damaged = bytearray(raw)
        damaged[position:position + size] = bytes(size)
        path = out / "damaged-packet.mp4"
        path.write_bytes(damaged)
        observe(path, {"kind": "encoded-packet-damage", "source": base.name,
                       "event_frame_index": 20, "offset": position, "bytes": size,
                       "replacement": "zero complete encoded packet; container headers unchanged"})
        marker = raw.find(b"avcC")
        if marker < 4 or marker + 8 >= len(raw):
            raise RuntimeError("missing H.264 configuration box")
        damaged = bytearray(raw)
        damaged[marker + 4:marker + 8] = bytes(4)
        path = out / "invalid-extradata.mp4"
        path.write_bytes(damaged)
        observe(path, {"kind": "invalid-avcc", "source": base.name,
                       "offset": marker + 4, "bytes": 4, "replacement": "zero avcC version/profile/level"})
        path = out / "garbage.mp4"
        path.write_bytes(b"AsciiFlow invalid media\x00\xff\x13\x37\n")
        observe(path, {"kind": "fixed-garbage"})
        data = mkv.read_bytes()
        blocks = video_blocks(data)
        if len(blocks) != 50:
            raise RuntimeError(f"expected 50 track-1 blocks, got {len(blocks)}")
        audio_before = [p.get("pts") for p in mkv_probe["packets"]["packets"] if p["stream_index"] == 1]
        for name, delta in (("timestamp-gap.mkv", 200), ("timestamp-backward.mkv", -400)):
            changed = bytearray(data)
            changes = []
            for index, offset in enumerate(blocks):
                if index < 20:
                    continue
                previous = int.from_bytes(data[offset:offset + 2], "big", signed=True)
                value = previous + delta
                if not -32768 <= value <= 32767:
                    raise RuntimeError("relative block timestamp overflow")
                changed[offset:offset + 2] = value.to_bytes(2, "big", signed=True)
                changes.append({"offset": offset, "old_relative_ticks": previous, "new_relative_ticks": value})
            path = out / name
            path.write_bytes(changed)
            entry, probe = observe(path, {"kind": "video-block-timestamp", "source": mkv.name,
                                         "event_frame_index": 20, "delta_milliseconds": delta,
                                         "changes": changes})
            audio_after = [p.get("pts") for p in (probe["packets"] or {}).get("packets", []) if p["stream_index"] == 1]
            if audio_after != audio_before:
                raise RuntimeError("mutation changed demuxed audio timestamps")
            pts = entry["decoded_video_pts"]
            baseline = mkv_entry["decoded_video_pts"]
            expected = [value + (delta if i >= 20 else 0) for i, value in enumerate(baseline)]
            entry["timeline_verified_from_decode"] = pts == expected
            if pts != expected:
                inventory["limitations"].append(f"{name}: decoded timestamps did not expose intended mutation; unqualified blocker")
        zero = out / "zero-frame-attempt.mp4"
        run([*common, "-i", base, "-map", "0:v:0", "-c:v", "copy", "-frames:v", "0", "-an", zero], False)
        if zero.exists():
            entry, _ = observe(zero, {"kind": "zero-frame-attempt"})
            if not entry["observed_streams"]:
                inventory["limitations"].append("zero-frame: FFmpeg removed the video track; retained output is no-video, not a qualified zero-frame video fixture")
        else:
            inventory["limitations"].append("zero-frame: FFmpeg did not produce a container; explicit construction blocker")
        vfr = out / "vfr-aac.mkv"
        run([*common, "-i", base, "-map", "0:v:0", "-map", "0:a:0",
             "-vf", "settb=1/1000,setpts=40*(N+floor(N/3)+2*floor((N+1)/3))",
             "-fps_mode", "vfr", "-c:v", "libx264", "-preset", "fast", "-bf", "0",
             "-g", "25", "-pix_fmt", "yuv420p", "-threads:v", "1",
             "-x264-params", "threads=1:lookahead_threads=1:scenecut=0", "-c:a", "copy",
             "-map_metadata", "-1", *audio_tags, "-fflags", "+bitexact", "-write_crc32", "0", vfr])
        entry, probe = observe(vfr, {"kind": "vfr-with-aac", "source": base.name,
                                    "presentation_pattern_milliseconds": [40, 120, 80]})
        pts = entry["decoded_video_pts"]
        deltas = sorted({right - left for left, right in zip(pts, pts[1:])})
        video = next(s for s in probe["streams"]["streams"] if s["codec_type"] == "video")
        if len(pts) != 50 or len(deltas) < 3 or video["time_base"] != "1/1000":
            raise RuntimeError("VFR fixture did not decode 50 frames with three observed intervals")
        entry["observed_video_intervals_milliseconds"] = deltas
        entry["timeline_verified_from_decode"] = True
    finally:
        save(out / "inventory.json", inventory)
    print(out / "inventory.json")


if __name__ == "__main__":
    main()
