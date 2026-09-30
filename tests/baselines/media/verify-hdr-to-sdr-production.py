#!/usr/bin/env python3
"""C4B encoded SDR oracle; repeated lossy outputs are never compared to raw pixels.

Tier meanings match verify-pq-production.py: packets, stream/coded signal,
decoded framehash, frame/container metadata, and optional whole-file hash.
An established baseline is evidence from actual runs, not a qualification claim.
"""
import argparse
from fractions import Fraction
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import sys
import tempfile

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("pq_oracle", Path(__file__).with_name("verify-pq-production.py"))
pq = importlib.util.module_from_spec(spec)
spec.loader.exec_module(pq)
run, probe, file_hash = pq.run, pq.probe, pq.file_hash

PROFILES = ("h264-8", "hevc-8", "av1-8", "hevc-10", "av1-10")
SOURCES = ("hevc", "av1")
TIERS = ("tier1a", "tier1b", "tier1c", "tier2", "tier3")
COMMON = ["--width", "80", "--charset", "standard", "--font", "builtin-8x8", "--color", "true",
          "--audio", "none", "--max-frames", "300", "--decode", "vaapi", "--backend", "vulkan",
          "--vulkan-mapping", "gpu", "--encode", "vaapi", "--hw-device", "/dev/dri/renderD128",
          "--vaapi-vulkan-input-interop", "on", "--vaapi-vulkan-output-interop", "on",
          "--output-dynamic-range", "sdr"]


def identity(directory):
    path = directory / "c3-pq-legal-v1-identity.json"
    document = json.loads(path.read_text(encoding="utf-8"))
    artifacts = {item["file"]: item for item in document["encoding"]["outputs"]}
    for source in SOURCES:
        filename = f"{source}-main10-pq-c3-legal-v1.mp4"
        expected = artifacts[filename]
        actual = file_identity(directory / filename)
        assert actual["bytes"] == expected["bytes"], f"{filename}: input size changed"
        assert actual["sha256"] == expected["sha256"], f"{filename}: input SHA-256 changed"
    return {"manifest": file_identity(path), "document": document}


def file_identity(path):
    return {"path": str(path.resolve()), "bytes": path.stat().st_size, "sha256": file_hash(path)}


def is_production_source(filename):
    parts = Path(filename).parts
    if parts[0] == "shaders":
        return True
    if parts[0] not in ("crates", "apps") or len(parts) < 3:
        return False
    return parts[2] == "src" or (len(parts) == 3 and parts[2] in ("Cargo.toml", "build.rs"))


def build_identity(binary):
    # rg includes untracked source files; Git diff alone cannot bind C4A/C4B additions.
    filenames = run("rg", "--files", "--no-ignore", "crates", "apps", "shaders").splitlines()
    sources = {filename: file_hash(Path(filename)) for filename in sorted(filenames)
               if is_production_source(filename)}
    for filename in ("Cargo.toml", "Cargo.lock", "build.rs", "rust-toolchain.toml", ".cargo/config.toml"):
        if Path(filename).is_file():
            sources[filename] = file_hash(Path(filename))
    if shutil.which("rpm"):
        packages = run("rpm", "-qa", "--qf", "%{NAME} %{EPOCHNUM}:%{VERSION}-%{RELEASE}.%{ARCH}\n").splitlines()
        selected = sorted(item for item in packages if item.split()[0].lower().startswith(
            ("ffmpeg", "mesa", "intel-media-driver", "spirv-tools", "spirvtools")))
        rpm_identity = {"status": "queried", "packages": selected}
    else:
        rpm_identity = {"status": "rpm-unavailable", "packages": []}
    return {"binary_sha256": file_hash(binary), "source_sha256": sources,
            "kernel": run("uname", "-a").strip(), "rpm": rpm_identity}


def check_color(value):
    for key, expected in {"color_primaries": "bt709", "color_transfer": "bt709",
                          "color_space": "bt709", "color_range": "tv"}.items():
        assert value.get(key) == expected, (key, value.get(key), expected)
    for side in value.get("side_data_list", []):
        kind = side.get("side_data_type", "").lower()
        assert not any(marker in kind for marker in ("mastering", "content light", "hdr", "dovi", "dolby vision")), side


def check_frames(stream, frames, codec, depth):
    pixel_format = "yuv420p10le" if depth == 10 else "yuv420p"
    assert stream["codec_name"] == codec
    expected_profile = {"h264": "High", "hevc": "Main 10" if depth == 10 else "Main", "av1": "Main"}[codec]
    assert stream["profile"] == expected_profile, (stream["profile"], expected_profile)
    assert stream["avg_frame_rate"] == "50/1"
    assert stream["chroma_location"] == "left"
    assert len(frames) == 300, len(frames)
    for value in [stream, *frames]:
        check_color(value)
        assert value.get("chroma_location") == "left", value.get("chroma_location")
        assert (value["width"], value["height"], value["pix_fmt"]) == (1920, 1080, pixel_format)
    time_base = Fraction(stream["time_base"])
    for index, frame in enumerate(frames):
        assert int(frame["best_effort_timestamp"]) * time_base == Fraction(index, 50), (index, frame)


def inspect(path, codec, depth):
    all_streams = probe(path, "-show_streams")["streams"]
    assert len(all_streams) == 1 and all_streams[0]["codec_type"] == "video", "expected video only"
    document = probe(path, "-select_streams", "v:0", "-show_streams", "-show_frames", "-show_packets", "-show_data_hash", "sha256")
    stream, = document["streams"]
    combined = document.get("packets_and_frames", [])
    frames = document.get("frames", [item for item in combined if item["type"] == "frame"])
    packets = document.get("packets", [item for item in combined if item["type"] == "packet"])
    check_frames(stream, frames, codec, depth)
    assert len(packets) == 300, len(packets)
    for index, packet in enumerate(packets):
        time_base = Fraction(stream["time_base"])
        assert int(packet["pts"]) * time_base == Fraction(index, 50), (index, packet)
        assert int(packet["dts"]) * time_base == Fraction(index, 50), (index, packet)
        assert int(packet["duration"]) * time_base == Fraction(1, 50), packet
    pixel_format = "yuv420p10le" if depth == 10 else "yuv420p"
    hashes = run("ffmpeg", "-v", "error", "-xerror", "-nostdin", "-i", str(path), "-map", "0:v:0", "-an",
                 "-pix_fmt", pixel_format, "-f", "framehash", "-hash", "sha256", "-")
    decoded = [line for line in hashes.splitlines() if line.strip() and not line.startswith("#")]
    assert len(decoded) == 300
    elementary_format = {"h264": "h264", "hevc": "hevc", "av1": "obu"}[codec]
    with tempfile.TemporaryDirectory(prefix="asciiflow-c4b-bitstream-") as folder:
        elementary = Path(folder) / f"video.{elementary_format}"
        run("ffmpeg", "-v", "error", "-xerror", "-nostdin", "-i", str(path), "-map", "0:v:0", "-an",
            "-c:v", "copy", "-f", elementary_format, str(elementary))
        coded = probe(elementary, "-show_streams", "-show_frames")
        elementary_stream, = coded["streams"]
        assert elementary_stream["codec_name"] == codec
        assert elementary_stream["profile"] == stream["profile"], "coded profile changed"
        assert len(coded["frames"]) == 300
        for value in [elementary_stream, *coded["frames"]]:
            check_color(value)
            assert value.get("chroma_location") == "left", value.get("chroma_location")
            assert (value["width"], value["height"], value["pix_fmt"]) == (1920, 1080, pixel_format)
    return {"identity": file_identity(path), "stream": stream, "packets": packets,
            "frames": frames, "decoded_framehash": decoded, "elementary_stream": elementary_stream}


def compare_runs(records):
    first = records[0]
    fields = {"tier1a": ("packets",), "tier1b": ("stream", "elementary_stream"),
              "tier1c": ("decoded_framehash",), "tier2": ("frames",)}
    tiers = {tier: all(all(record[field] == first[field] for field in keys) for record in records)
             for tier, keys in fields.items()}
    tiers["tier3"] = len({record["identity"]["sha256"] for record in records}) == 1
    assert all(tiers[tier] for tier in TIERS[:-1]), "Tier 1A/1B/1C/2 changed between repeated runs"
    return tiers


def check_baseline(report, baseline):
    assert report["input_identity"]["manifest"]["sha256"] == baseline["input_identity"]["manifest"]["sha256"]
    assert set(report["profiles"]) == set(baseline["profiles"])
    for key, profile in report["profiles"].items():
        golden = baseline["profiles"][key]
        if golden["tiers"]["tier3"]:
            assert report["build_identity"] == baseline["build_identity"], f"{key}: Tier 3 exact build scope changed"
        first = golden["runs"][0]
        for current in profile["runs"]:
            for field in ("packets", "stream", "elementary_stream", "decoded_framehash", "frames"):
                assert current[field] == first[field], f"{key}: retained {field} oracle changed"
            if golden["tiers"]["tier3"]:
                assert current["identity"]["sha256"] == first["identity"]["sha256"], f"{key}: retained Tier 3 changed"


def write_json(path, document):
    # Evidence is append-only; no accidental baseline/report overwrite.
    with path.open("x", encoding="utf-8") as output:
        json.dump(document, output, indent=2, sort_keys=True)
        output.write("\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    modes = parser.add_subparsers(dest="mode", required=True)
    input_parser = modes.add_parser("identity")
    input_parser.add_argument("directory", type=Path)
    record_parser = modes.add_parser("record")
    record_parser.add_argument("path", type=Path)
    record_parser.add_argument("command", nargs=argparse.REMAINDER)
    success_parser = modes.add_parser("success")
    success_parser.add_argument("command", type=Path)
    success_parser.add_argument("output", type=Path)
    success_parser.add_argument("record", type=Path)
    verify_parser = modes.add_parser("verify")
    verify_parser.add_argument("directory", type=Path)
    verify_parser.add_argument("report", type=Path)
    verify_parser.add_argument("--input-directory", type=Path, required=True)
    verify_parser.add_argument("--baseline", type=Path)
    verify_parser.add_argument("--check-baseline", type=Path)
    args = parser.parse_args()
    if args.mode == "identity":
        identity(args.directory)
        return
    if args.mode == "record":
        command = args.command[1:] if args.command[:1] == ["--"] else args.command
        assert len(command) > 3
        write_json(args.path, {"argv": command, "cwd": str(Path.cwd()),
                              "binary": file_identity(Path(command[0])), "input": file_identity(Path(command[1])),
                              "build_identity": build_identity(Path(command[0]))})
        return
    if args.mode == "success":
        write_json(args.record, {"exit_code": 0, "command_sha256": file_hash(args.command),
                                 "output": file_identity(args.output)})
        return
    version = run("ffmpeg", "-version")
    assert version.startswith("ffmpeg version 8.1.3 "), version.splitlines()[0]
    report = {"schema_version": 1, "scope": "C4B repeated production output oracle; hardware closure is separate",
              "input_identity": identity(args.input_directory), "ffmpeg_version": version,
              "oracle": file_identity(Path(__file__)),
              "generator": file_identity(Path(__file__).with_name("generate-hdr-to-sdr-production-v1.sh")),
              "source_head": run("git", "rev-parse", "HEAD").strip(),
              "cargo_lock": file_identity(Path("Cargo.lock")),
              "production_diff_sha256": hashlib.sha256(run("git", "diff", "HEAD", "--",
                  "apps/asciiflow-cli/src", "crates/asciiflow-core/src", "crates/asciiflow-media/src",
                  "crates/asciiflow-vulkan/src", "crates/asciiflow-interop/src").encode()).hexdigest(),
              "profiles": {}}
    generation_binary = None
    generation_build = None
    for source in SOURCES:
        for profile in PROFILES:
            codec, depth = profile.split("-")
            records = []
            for index in (1, 2, 3):
                stem = args.directory / f"{source}-to-{profile}-run{index}"
                output = stem.with_suffix(".mp4")
                command = json.loads(stem.with_suffix(".command.json").read_text(encoding="utf-8"))
                argv = command["argv"]
                assert command["cwd"] == str(Path.cwd()), "verify from the generation working directory"
                assert argv[3:] == COMMON + ["--output-codec", codec, "--output-bit-depth", depth, "--no-progress"]
                assert Path(argv[1]).resolve() == (args.input_directory / f"{source}-main10-pq-c3-legal-v1.mp4").resolve()
                assert Path(argv[2]).resolve() == output.resolve()
                assert command["binary"] == file_identity(Path(argv[0])), "generation binary changed"
                if generation_binary is None:
                    generation_binary = command["binary"]
                    generation_build = build_identity(Path(argv[0]))
                assert command["binary"] == generation_binary, "matrix used different production binaries"
                assert command["build_identity"] == generation_build, "generation build or machine identity changed"
                assert command["input"] == file_identity(Path(argv[1])), "generation input changed"
                record = inspect(output, codec, int(depth))
                success = json.loads(stem.with_suffix(".success.json").read_text(encoding="utf-8"))
                assert success == {"exit_code": 0, "command_sha256": file_hash(stem.with_suffix(".command.json")),
                                   "output": record["identity"]}, "missing or changed successful-run evidence"
                record["production_command"] = command
                record["successful_run"] = success
                record["process_time"] = stem.with_suffix(".time").read_text(encoding="utf-8").strip()
                record["stage_summary"] = stem.with_suffix(".log").read_text(encoding="utf-8").splitlines()
                records.append(record)
            report["profiles"][f"{source}-to-{profile}"] = {"runs": records, "tiers": compare_runs(records)}
    report["binary"] = generation_binary
    report["build_identity"] = generation_build
    if args.check_baseline:
        check_baseline(report, json.loads(args.check_baseline.read_text(encoding="utf-8")))
    write_json(args.report, report)
    if args.baseline:
        write_json(args.baseline, report)


if __name__ == "__main__":
    main()
