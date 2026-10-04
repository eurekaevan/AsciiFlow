#!/usr/bin/env python3
"""Print schema-validated candidate records from retained generation inventories.

This is a review aid, not corpus promotion. Outcomes remain explicitly
unqualified until the harness observes and reviews the production behavior.
"""
import argparse
from fractions import Fraction
import hashlib
from itertools import groupby
import json
from pathlib import Path
import re
import sys

from schema import validate


ROOT = Path(__file__).resolve().parents[2]


def read(path):
    return json.loads(path.read_text(encoding="utf-8"))


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def parts(probe):
    mixed = probe.get("packets_and_frames", [])
    frames = probe.get("frames", [item for item in mixed if item.get("type") == "frame"])
    packets = probe.get("packets", [item for item in mixed if item.get("type") == "packet"])
    return frames, packets


def describe(probe, name):
    streams = probe["streams"]
    video = next(stream for stream in streams if stream["codec_type"] == "video")
    frames, packets = parts(probe)
    frames = [frame for frame in frames if frame.get("media_type") == "video"]
    packets = [packet for packet in packets if packet.get("stream_index") == video["index"]]
    first = frames[0] if frames else {}
    pts = [frame.get("pts") for frame in frames]
    deltas = {b - a for a, b in zip(pts, pts[1:]) if a is not None and b is not None}
    durations = {frame["duration"] for frame in frames if frame.get("duration", 0) > 0}
    rate = video.get("avg_frame_rate", "0/0")
    if rate == "0/0":
        rate = video.get("r_frame_rate", "0/1")
    model = "Unknown"
    if pts and all(value is not None for value in pts):
        observed_steps = deltas or durations
        model = "VFR" if len(observed_steps) > 1 else "CFR"
        if rate not in ("0/0", "0/1") and video.get("time_base"):
            step = 1 / Fraction(rate) / Fraction(video["time_base"])
            model = "CFR" if all(abs(value - step) <= 1 for value in observed_steps) else "VFR"
    keys = [index for index, frame in enumerate(frames) if frame.get("key_frame") == 1]
    gaps = {b - a for a, b in zip(keys, keys[1:])}
    pixel = first.get("pix_fmt", video.get("pix_fmt", "unknown"))
    match = re.search(r"p(10|12|16)(?:le|be)?$", pixel)
    depth = int(match[1]) if match else 8
    side_data = video.get("side_data_list", []) + first.get("side_data_list", [])
    rotation = next((side.get("rotation") for side in side_data if side.get("side_data_type") == "Display Matrix"), None)
    colors = {key: first.get(raw, video.get(raw)) for key, raw in
              (("color_primaries", "color_primaries"), ("color_transfer", "color_transfer"),
               ("color_matrix", "color_space"), ("color_range", "color_range"),
               ("chroma_location", "chroma_location"))}
    metadata = {**colors, "side_data": sorted({side["side_data_type"] for side in side_data}),
                "title": video.get("tags", {}).get("title"),
                "language": video.get("tags", {}).get("language"),
                "rotation_degrees": rotation, "orientation": None,
                "sample_aspect_ratio": first.get("sample_aspect_ratio", video.get("sample_aspect_ratio")),
                "mastering_display": None, "content_light": None}
    for side in side_data:
        if side["side_data_type"] == "Mastering display metadata":
            metadata["mastering_display"] = {
                "primaries_xy": [[side[f"{color}_x"], side[f"{color}_y"]] for color in ("red", "green", "blue")],
                "white_point_xy": [side["white_point_x"], side["white_point_y"]],
                "min_luminance": side.get("min_luminance"), "max_luminance": side.get("max_luminance")}
        elif side["side_data_type"] == "Content light level metadata":
            metadata["content_light"] = {"max_cll": side.get("max_content"), "max_fall": side.get("max_average")}
    audio = [{"codec": stream["codec_name"], "channels": stream["channels"],
              "sample_rate": int(stream["sample_rate"]), "time_base": stream["time_base"],
              "language": stream.get("tags", {}).get("language"),
              "default": bool(stream.get("disposition", {}).get("default", 0))}
             for stream in streams if stream["codec_type"] == "audio"]
    subtitles = [{"codec": stream["codec_name"], "time_base": stream["time_base"],
                  "language": stream.get("tags", {}).get("language"),
                  "default": bool(stream.get("disposition", {}).get("default", 0)),
                  "expected_behavior": "unqualified"}
                 for stream in streams if stream["codec_type"] == "subtitle"]
    return {"container": Path(name).suffix.lstrip("."), "codec": video["codec_name"],
            "profile": video.get("profile", "unknown"), "width": first.get("width", video["width"]),
            "height": first.get("height", video["height"]), "pixel_format": pixel,
            "bit_depth": depth, "chroma_420": pixel.startswith("yuv420"),
            "timing": {"model": model, "frame_rate": rate, "frame_count": len(frames) or None,
                       "time_base": video.get("time_base"), "start_pts": pts[0] if pts else None,
                       "negative_dts": any(packet.get("dts", 0) < 0 for packet in packets),
                       "reorder": any(packet.get("pts") is not None and packet.get("dts") is not None
                                      and packet["pts"] != packet["dts"] for packet in packets)
                                  or any(frame.get("pict_type") == "B" for frame in frames),
                       "odd_durations": len(deltas) > 1 or len(durations) > 1},
            "gop": {"model": "observed key-frame cadence; encoder open-GOP state unknown",
                    "keyint": next(iter(gaps)) if len(gaps) == 1 else None,
                    "b_frames": max((sum(1 for _ in run) for kind, run in
                                     groupby(frame.get("pict_type") for frame in frames) if kind == "B"), default=0),
                    "open_gop": None,
                    "key_cadence": "decoded frame indices " + ",".join(map(str, keys)) if keys else None},
            "audio": audio, "subtitles": subtitles, "metadata": metadata}, video, first


def classification(media, stream, first):
    for key in ("color_primaries", "color_transfer", "color_space", "color_range", "chroma_location"):
        left, right = stream.get(key), first.get(key)
        if left not in (None, "unknown", "unspecified") and right not in (None, "unknown", "unspecified") and left != right:
            return "Conflicting"
    metadata = media["metadata"]
    if metadata["color_transfer"] == "smpte2084":
        return "HdrPq"
    if metadata["color_transfer"] == "arib-std-b67":
        return "HdrHlg"
    if any(metadata[key] in (None, "unknown", "unspecified") for key in
           ("color_primaries", "color_transfer", "color_matrix")):
        return "Sdr" if media["bit_depth"] == 8 else "Unknown"
    return "Sdr"


def build(directory, group):
    inventory = read(directory / "inventory.json")
    if group == "real-video":
        script = "tests/corpus/generate-real-video.py"
        outputs = inventory["outputs"]
        probes = {entry["path"]: entry["json"] for entry in inventory["probe_inventory"]}
        version = inventory["ffmpeg_version"]
    else:
        script = ("tests/corpus/generate-real-negative.py" if group == "real-negative"
                  else "tests/corpus/generate-real-audio-metadata.py")
        outputs = inventory["artifacts"]
        probes = {}
        for entry in outputs:
            name = entry["path"]
            probe = read(directory / f"{name}.streams.json") or {}
            probe.update(read(directory / f"{name}.frames.json") or {})
            probe.update(read(directory / f"{name}.packets.json") or {})
            probes[name] = probe
        version = inventory["tools"]["ffmpeg"]["version_and_configuration"]
        if group == "real-negative" and sha(ROOT / script) != inventory["script"]["sha256"]:
            raise ValueError("negative inventory generator identity differs from the current source")
    records = []
    for output in outputs:
        name = output["path"]
        path = directory / name
        if sha(path) != output["sha256"] or path.stat().st_size != output["bytes"]:
            raise ValueError(f"inventory/media identity mismatch: {path}")
        invalid_probe = (group == "real-negative" and
                         (output["probes"]["streams"]["returncode"] != 0 or
                          not any(s.get("codec_type") == "video" for s in probes[name].get("streams", []))))
        invalid_probe = invalid_probe or not any(frame.get("media_type") == "video"
                                                for frame in parts(probes[name])[0])
        if invalid_probe:
            media, category = None, "Unknown"
        else:
            media, stream, first = describe(probes[name], name)
            category = classification(media, stream, first)
        hdr = media is not None and (category in ("HdrPq", "HdrHlg") or media["metadata"]["color_transfer"] in ("smpte2084", "arib-std-b67"))
        categories = ["basic", "timing", "gop", "container", "color"]
        if media and media["audio"]:
            categories.append("audio")
        if (media and media["subtitles"]) or "sar-" in name or "rotation" in name:
            categories.append("metadata")
        if "midstream" in name or "changed-sps" in name:
            categories.append("mutation")
        if "missing" in name or "unsupported" in name or "conflict" in name:
            categories.append("negative")
        if name.startswith("short-"):
            categories.append("short")
        if "medium-" in name or "1080p" in name or "720p" in name:
            categories.append("stress")
        if group == "real-negative" and "negative" not in categories:
            categories.append("negative")
        source = {"kind": "generated", "path": "generated/" + name, "sha256": output["sha256"],
                  "byte_size": output["bytes"], "generator": script, "generator_sha256": sha(ROOT / script),
                  "tool_identity": version.splitlines()[0] + "; ffmpeg -version SHA256 " + hashlib.sha256(version.encode()).hexdigest(),
                  "exact_command": ["python3", script, "{output_directory}"], "generation_group": group}
        if name in ("pq-no-static.mp4", "stream-frame-conflict-attempt.mp4"):
            reference = "tests/fixtures/codecs/c3-pq-legal-v1-identity.json"
            source["reference_manifest"] = {"path": reference, "sha256": sha(ROOT / reference)}
        evidence = "Candidate expectation derived from observed probe properties; production result requires explicit evidence review."
        if categories.count("mutation"):
            evidence += " Native elementary-stream PTS may be absent; mutation outcome has not been frozen."
        if "missing-range-attempt" in name:
            evidence += " Attempt did not remove range: actual stream/frame range remains limited."
        if "sdr-with-hdr" in name:
            evidence += " SDR transfer with retained static HDR metadata requires policy review."
        fixture_name = name if group == "real-negative" else Path(name).stem
        records.append({"id": "real-" + re.sub(r"[^a-z0-9-]", "-", fixture_name.lower()),
                        "description": f"Observed generated candidate {name}.", "categories": categories,
                        "source": source, "media": media,
                        "request": {"dynamic_range": "preserve", "codec": "hevc" if hdr else "h264",
                                    "bit_depth": 10 if hdr else 8, "audio": "copy" if media and media["audio"] else "none",
                                    "route": "hardware" if hdr else "portable"},
                        "expected": {"classification": category, "status": "Unqualified", "planner": "accept",
                                     "runtime": "Pass", "failure_stage": None, "root_cause": None,
                                     "mutation": None, "corruption": None, "evidence": evidence,
                                     "video_timing": "cfr_origin" if media and media["audio"] else "cfr_zero",
                                     "blocker": "unqualified 5.4A property; requires explicit evidence review"},
                        "performance_fixture": False})
        if group == "real-negative":
            expected = records[-1]["expected"]
            mutation = output["mutation"]
            expected["evidence"] = "Retained ffprobe and FFmpeg decode characterization only; no production outcome observed. " + json.dumps(mutation, sort_keys=True)
            expected["blocker"] = "characterization only; production outcome requires explicit evidence review"
            if invalid_probe:
                expected.update(runtime="RejectAtClassification", failure_stage="InputProbe")
            elif mutation["kind"] in ("video-block-timestamp", "vfr-with-aac"):
                expected.update(runtime="RuntimeFailureExpected", failure_stage="DecodeRuntime")
                records[-1]["request"]["audio"] = "copy"
            kinds = {"truncate": "truncated_file", "encoded-packet-damage": "damaged_packet", "invalid-avcc": "invalid_extradata"}
            if mutation["kind"] in kinds:
                expected["corruption"] = {"kind": kinds[mutation["kind"]],
                                          "offset": mutation.get("offset", mutation.get("keep_bytes")),
                                          "description": json.dumps(mutation, sort_keys=True)}
    return records


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("video_dir", type=Path)
    parser.add_argument("audio_dir", type=Path)
    parser.add_argument("--negative-dir", type=Path)
    args = parser.parse_args()
    manifest = {"schema_version": 1,
                "fixtures": build(args.video_dir, "real-video") + build(args.audio_dir, "real-audio-metadata")}
    if args.negative_dir:
        manifest["fixtures"].extend(build(args.negative_dir, "real-negative"))
    for fixture in manifest["fixtures"]:
        fixture["id"] = "real-" + re.sub(r"[^a-z0-9-]", "-", Path(fixture["source"]["path"]).name.lower())
    if len({fixture["id"] for fixture in manifest["fixtures"]}) != len(manifest["fixtures"]):
        raise ValueError("generated corpus fixture identities are not unique")
    validate(manifest, read(ROOT / "tests/corpus/manifest.schema.json"))
    json.dump(manifest, sys.stdout, indent=2, sort_keys=True)
    sys.stdout.write("\n")


if __name__ == "__main__":
    main()
