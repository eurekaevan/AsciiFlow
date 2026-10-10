#!/usr/bin/env python3
"""Inspect actual container, packets, decoded frames and elementary PQ signals."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile


def run(*args):
    return subprocess.run(args, check=True, capture_output=True, text=True).stdout


def probe(path, *args):
    return json.loads(run("ffprobe", "-v", "error", *args, "-of", "json", str(path)))


def check_color(value):
    expected = {"color_primaries": "bt2020", "color_transfer": "smpte2084",
                "color_space": "bt2020nc", "color_range": "tv"}
    for key, target in expected.items():
        assert value.get(key) == target, (key, value.get(key), target)
    for side in value.get("side_data_list", []):
        assert side.get("side_data_type") not in {
            "Mastering display metadata", "Content light level metadata"}, side


def file_hash(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def canonical_identity(directory):
    identity = json.loads((directory / "pq-canonical-v1-identity.json").read_text(encoding="utf-8"))
    for artifact in identity["encoding"]["outputs"]:
        path = directory / artifact["file"]
        assert path.stat().st_size == artifact["bytes"], f"{path}: canonical input size changed"
        assert file_hash(path) == artifact["sha256"], f"{path}: canonical input SHA-256 changed"
    return identity


def inspect(path, codec, count=300, fps=50):
    document = probe(path, "-select_streams", "v:0", "-show_streams", "-show_frames", "-show_packets", "-show_data_hash", "sha256")
    stream, = document["streams"]
    assert stream["codec_name"] == codec
    assert stream["profile"] == ("Main 10" if codec == "hevc" else "Main")
    assert (stream["width"], stream["height"], stream["pix_fmt"]) == (1920, 1080, "yuv420p10le")
    assert stream["avg_frame_rate"] == f"{fps}/1"
    assert stream["chroma_location"] == "left"
    check_color(stream)
    # ffprobe combines frame/packet output into one ordered list.
    combined = document.get("packets_and_frames", [])
    frames = document.get("frames", [item for item in combined if item["type"] == "frame"])
    packets = document.get("packets", [item for item in combined if item["type"] == "packet"])
    assert len(frames) == len(packets) == count, (len(frames), len(packets), count)
    for index, frame in enumerate(frames):
        check_color(frame)
        assert (frame["width"], frame["height"], frame["pix_fmt"]) == (1920, 1080, "yuv420p10le")
        assert abs(float(frame["best_effort_timestamp_time"]) - index / fps) < 1e-6
    hashes = run("ffmpeg", "-v", "error", "-nostdin", "-i", str(path), "-map", "0:v:0",
                 "-an", "-pix_fmt", "yuv420p10le", "-f", "framehash", "-hash", "sha256", "-")
    decoded = [line for line in hashes.splitlines() if not line.startswith("#") and line.strip()]
    assert len(decoded) == count
    with tempfile.TemporaryDirectory(prefix="asciiflow-pq-bitstream-") as folder:
        elementary = Path(folder) / ("video.hevc" if codec == "hevc" else "video.obu")
        run("ffmpeg", "-v", "error", "-nostdin", "-i", str(path), "-map", "0:v:0", "-an", "-c:v", "copy", "-f", "hevc" if codec == "hevc" else "obu", str(elementary))
        elementary_document = probe(elementary, "-show_streams", "-show_frames")
        elementary_stream, = elementary_document["streams"]
        check_color(elementary_stream)
        assert len(elementary_document["frames"]) == count
        for frame in elementary_document["frames"]:
            check_color(frame)
    return {"filename": path.name, "byte_size": path.stat().st_size,
            "sha256": file_hash(path),
            "stream": stream, "packets": packets, "frames": frames,
            "decoded_framehash": decoded, "elementary_stream": elementary_stream}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("directory", type=Path)
    parser.add_argument("report", type=Path)
    parser.add_argument("--input-directory", type=Path)
    parser.add_argument("--baseline", type=Path)
    parser.add_argument("--check-baseline", type=Path)
    parser.add_argument("--measurement-directory", type=Path)
    parser.add_argument("--qualification", type=Path)
    parser.add_argument("--binary", type=Path, default=Path("target/release/asciiflow"))
    args = parser.parse_args()
    identity = canonical_identity(args.input_directory) if args.input_directory else None
    version = run("ffmpeg", "-version")
    assert version.startswith("ffmpeg version 8.1.3 "), version.splitlines()[0]
    report = {"ffmpeg_version": version, "profiles": {}}
    for codec in ["hevc", "av1"]:
        records = [inspect(args.directory / f"{codec}-10-run{index}.mp4", codec) for index in [1, 2, 3]]
        tier1a = all(record["packets"] == records[0]["packets"] for record in records)
        tier1c = all(record["decoded_framehash"] == records[0]["decoded_framehash"] for record in records)
        tier3 = len({record["sha256"] for record in records}) == 1
        assert tier1a and tier1c, "encoded packets or decoded pixels are nondeterministic; do not establish a hash gate"
        tier1b = all(record["stream"] == records[0]["stream"]
                      and record["elementary_stream"] == records[0]["elementary_stream"]
                      for record in records)
        tier2 = all(record["frames"] == records[0]["frames"] for record in records)
        assert tier1b and tier2, "output signal/container semantics changed between runs"
        report["profiles"][codec] = {"runs": records, "tier1a": tier1a,
            "tier1b": tier1b, "tier1c": tier1c, "tier2": tier2, "tier3": tier3}
    args.report.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    if args.check_baseline:
        established = json.loads(args.check_baseline.read_text(encoding="utf-8"))
        assert args.input_directory is not None, "regression check requires canonical input identity"
        assert file_hash(args.input_directory / "pq-canonical-v1-identity.json") == established["input_identity_sha256"]
        for codec, record in report["profiles"].items():
            golden = established["outputs"][codec]
            for current in record["runs"]:
                assert current["packets"] == golden["packet_oracle"], f"{codec}: retained Tier 1A changed"
                assert current["stream"] == golden["stream"], f"{codec}: retained Tier 1B changed"
                assert current["elementary_stream"] == golden["elementary_stream"], f"{codec}: coded signal changed"
                assert current["decoded_framehash"] == golden["decoded_framehash"], f"{codec}: retained Tier 1C changed"
                if "frame_oracle" in golden:
                    assert current["frames"] == golden["frame_oracle"], f"{codec}: retained Tier 2 changed"
                if golden["hash_gate"]:
                    assert current["sha256"] == golden["repeated_output_sha256"][0], f"{codec}: retained Tier 3 changed"
    if args.baseline:
        assert args.input_directory is not None, "baseline requires the canonical input directory"
        identity_path = args.input_directory / "pq-canonical-v1-identity.json"
        golden = {}
        for codec, record in report["profiles"].items():
            first = record["runs"][0]
            golden[codec] = {
                "repeated_output_sha256": [item["sha256"] for item in record["runs"]],
                "hash_gate": record["tier1a"] and record["tier1c"] and record["tier3"],
                "tiers": {key: record[key] for key in ["tier1a", "tier1b", "tier1c", "tier2", "tier3"]},
                "byte_size": first["byte_size"], "stream": first["stream"],
                "elementary_stream": first["elementary_stream"],
                "packet_oracle": first["packets"], "frame_oracle": first["frames"],
                "decoded_framehash": first["decoded_framehash"],
            }
        common = ["--width", "80", "--charset", "standard", "--font", "builtin-8x8", "--color", "true",
                  "--audio", "none", "--max-frames", "300", "--decode", "vaapi", "--backend", "vulkan",
                  "--vulkan-mapping", "gpu", "--encode", "vaapi", "--hw-device", "/dev/dri/renderD128",
                  "--vaapi-vulkan-input-interop", "on", "--vaapi-vulkan-output-interop", "on", "--no-progress"]
        manifest = {
            "schema_version": 1, "name": "Canonical PQ production baseline v1",
            "scope": "Intel Arc Meteor Lake / iHD / ANV; full closure gates in docs/production-support.md",
            "color_processing": "HdrPqPreserve", "tone_mapping": "None",
            "output_color": {"primaries": "BT.2020", "transfer": "PQ", "matrix": "BT.2020 NCL", "range": "limited", "chroma_location": "left"},
            "source_static_metadata": "mastering display / MaxCLL / MaxFALL neither propagated nor recomputed; not HDR10 mastering qualification",
            "source_head": run("git", "rev-parse", "HEAD").strip(),
            "cargo_lock_sha256": file_hash(Path("Cargo.lock")),
            "production_diff_sha256": hashlib.sha256(run("git", "diff", "--", "apps/asciiflow-cli/src", "crates/asciiflow-core/src", "crates/asciiflow-media/src", "crates/asciiflow-vulkan/src", "crates/asciiflow-interop/src").encode()).hexdigest(),
            "binary_sha256": file_hash(args.binary), "ffmpeg_version": version,
            "input_identity": identity, "input_identity_sha256": file_hash(identity_path),
            "production_script": "tests/baselines/media/generate-pq-production-v1.sh",
            "production_script_sha256": file_hash(Path(__file__).with_name("generate-pq-production-v1.sh")),
            "common_arguments": common, "profile_arguments": ["--output-codec", "<hevc|av1>", "--output-bit-depth", "10"],
            "oracle_script_sha256": file_hash(Path(__file__)), "oracle_report_sha256": file_hash(args.report),
            "outputs": golden,
        }
        if args.qualification:
            manifest["hardware_qualification"] = json.loads(args.qualification.read_text(encoding="utf-8"))
            manifest["hardware_qualification_sha256"] = file_hash(args.qualification)
        if args.measurement_directory:
            measurements = {}
            for codec in ["hevc", "av1"]:
                records = []
                for index in [1, 2, 3]:
                    stem = f"{codec}-10-run{index}"
                    folder = args.measurement_directory
                    assert file_hash(folder / f"{stem}.mp4") == golden[codec]["repeated_output_sha256"][0], "measurement feature changed output"
                    records.append({
                        "process_time": (folder / f"{stem}.time").read_text().strip(),
                        "stage_summary": (folder / f"{stem}.log").read_text().splitlines(),
                        "output_sha256": file_hash(folder / f"{stem}.mp4"),
                    })
                measurements[codec] = records
            manifest["production_measurements"] = measurements
        args.baseline.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    for codec, record in report["profiles"].items():
        print(codec, [item["sha256"] for item in record["runs"]], "tiers", [record[key] for key in ["tier1a", "tier1b", "tier1c", "tier2", "tier3"]])


if __name__ == "__main__":
    main()
