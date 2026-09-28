#!/usr/bin/env python3
"""Decode every canonical PQ frame and verify samples against its source identity."""

import argparse
from array import array
from collections import Counter
from fractions import Fraction
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time


def file_hash(path: Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def executable(value: str) -> Path:
    found = shutil.which(value)
    if found is None:
        raise RuntimeError(f"tool not found: {value}")
    return Path(found).resolve(strict=True)


def probe(path: Path, ffprobe: Path, codec: str, source: dict) -> dict:
    result = subprocess.run([
        str(ffprobe), "-v", "error", "-show_streams", "-of", "json", str(path)
    ], check=True, capture_output=True, text=True)
    streams = json.loads(result.stdout)["streams"]
    if len(streams) != 1:
        raise AssertionError(f"{path}: expected exactly one video-only stream")
    stream = streams[0]
    expected = {
        "codec_type": "video", "codec_name": codec,
        "width": source["width"], "height": source["height"],
        "pix_fmt": "yuv420p10le", "color_primaries": "bt2020",
        "color_transfer": "smpte2084", "color_space": "bt2020nc",
        "color_range": "tv", "chroma_location": "left",
        "profile": "Main 10" if codec == "hevc" else "Main",
        "nb_frames": str(source["frames"]),
    }
    for key, value in expected.items():
        if stream.get(key) != value:
            raise AssertionError(f"{path}: {key}={stream.get(key)!r}, expected {value!r}")
    if Fraction(stream["avg_frame_rate"]) != Fraction(*source["frame_rate"]):
        raise AssertionError(f"{path}: frame rate differs")
    if Fraction(stream["duration"]) != Fraction(source["frames"] * source["frame_rate"][1], source["frame_rate"][0]):
        raise AssertionError(f"{path}: duration differs")
    # Retain only the inspected facts, not incidental ffprobe build-dependent fields.
    return {key: stream[key] for key in [*expected, "avg_frame_rate", "time_base", "start_time", "duration"]}


def read_frame(stream, length: int) -> bytes:
    data = bytearray()
    while len(data) < length:
        block = stream.read(length - len(data))
        if not block:
            break
        data.extend(block)
    return bytes(data)


def decode(path: Path, ffmpeg: Path, identity: dict) -> dict:
    source = identity["source"]
    frame_bytes = source["frame_bytes"]
    samples = [source["width"] * source["height"], source["width"] * source["height"] // 4, source["width"] * source["height"] // 4]
    totals = [[0] * 4 for _ in range(3)]
    minima = [1023] * 3
    maxima = [0] * 3
    source_hash = hashlib.sha256()
    frames = []
    started = time.monotonic()
    # Scoped stderr avoids pipe backpressure if a damaged input emits many errors.
    with tempfile.TemporaryFile() as errors:
        process = subprocess.Popen([
            str(ffmpeg), "-hide_banner", "-loglevel", "error", "-nostdin",
            "-threads", "1", "-i", str(path), "-map", "0:v:0", "-an",
            "-fps_mode", "passthrough", "-pix_fmt", "yuv420p10le",
            "-f", "rawvideo", "-"
        ], stdout=subprocess.PIPE, stderr=errors)
        try:
            for index, expected in enumerate(identity["frames"]):
                data = read_frame(process.stdout, frame_bytes)
                if len(data) != frame_bytes:
                    raise AssertionError(f"{path}: truncated/missing decoded frame {index}: {len(data)} bytes")
                digest = hashlib.sha256(data).hexdigest()
                if expected["index"] != index or digest != expected["raw_yuv420p10le_sha256"]:
                    raise AssertionError(f"{path}: decoded frame {index} differs from canonical source")
                source_hash.update(data)
                offset = 0
                histograms = []
                for plane, (count, high) in enumerate(zip(samples, [940, 960, 960])):
                    values = array("H")
                    values.frombytes(data[offset:offset + count * 2])
                    if sys.byteorder != "little":
                        values.byteswap()
                    # Counter traverses every actual decoded word; no sampling or
                    # inference from the source generator's claimed statistics.
                    codes = Counter(values)
                    low_bits = [0] * 4
                    for code, frequency in codes.items():
                        if not 64 <= code <= high:
                            raise AssertionError(f"{path}: frame {index}, plane {plane}, illegal code {code}")
                        low_bits[code & 3] += frequency
                    if sum(low_bits) != count:
                        raise AssertionError("decoded sample count differs")
                    minima[plane] = min(minima[plane], min(codes))
                    maxima[plane] = max(maxima[plane], max(codes))
                    for residue in range(4):
                        totals[plane][residue] += low_bits[residue]
                    histograms.append(low_bits)
                    offset += count * 2
                if histograms != expected["low_two_bit_histograms_y_u_v"]:
                    raise AssertionError(f"{path}: frame {index} actual low bits differ from source identity")
                frames.append({"index": index, "decoded_yuv420p10le_sha256": digest})
            if process.stdout.read(1):
                raise AssertionError(f"{path}: extra decoded frames after frame 299")
            status = process.wait()
            errors.seek(0)
            diagnostic = errors.read().decode("utf-8", errors="replace")
            if status != 0:
                raise RuntimeError(f"{path}: FFmpeg decode failed ({status}): {diagnostic}")
        finally:
            process.stdout.close()
            if process.poll() is None:
                process.kill()
            process.wait()
    if source_hash.hexdigest() != source["raw_yuv420p10le_sha256"]:
        raise AssertionError(f"{path}: decoded whole-source hash differs")
    if totals != identity["low_two_bits"]["histograms_y_u_v"]:
        raise AssertionError(f"{path}: actual low bits differ from source totals")
    nonzero = [sum(counts[1:]) for counts in totals]
    if not all(nonzero):
        raise AssertionError(f"{path}: each decoded plane must demonstrate genuine ten-bit low bits")
    return {
        "decoded_frames": len(frames), "decoded_bytes": len(frames) * frame_bytes,
        "decoded_source_sha256": source_hash.hexdigest(), "frames": frames,
        "actual_low_two_bit_histograms_y_u_v": totals,
        "actual_nonzero_low_two_bits_y_u_v": nonzero,
        "actual_active_samples": sum(samples) * len(frames),
        "actual_minimum_codes_y_u_v": minima, "actual_maximum_codes_y_u_v": maxima,
        "verification_wall_seconds": time.monotonic() - started,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("report", type=Path)
    parser.add_argument("--ffmpeg", default="ffmpeg")
    parser.add_argument("--ffprobe", default="ffprobe")
    args = parser.parse_args()
    identity_path = args.directory / "pq-canonical-v1-identity.json"
    identity = json.loads(identity_path.read_text(encoding="utf-8"))
    source = identity["source"]
    if identity["generator"] != "pq-canonical-v1" or source["frames"] != 300 or len(identity["frames"]) != 300:
        raise AssertionError("not a complete canonical PQ v1 identity")
    if (source["width"], source["height"], source["frame_rate"], source["frame_bytes"]) != (1920, 1080, [50, 1], 6220800):
        raise AssertionError("canonical PQ dimensions, rate or frame storage differ")
    if len(identity["encoding"]["outputs"]) != 2:
        raise AssertionError("canonical identity must contain exactly HEVC and AV1 outputs")
    ffmpeg, ffprobe = executable(args.ffmpeg), executable(args.ffprobe)
    version = subprocess.run([str(ffmpeg), "-version"], check=True, capture_output=True).stdout
    if not version.startswith(b"ffmpeg version 8.1.3 "):
        raise AssertionError("verification requires fixed FFmpeg 8.1.3")
    probe_version = subprocess.run([str(ffprobe), "-version"], check=True, capture_output=True).stdout
    if not probe_version.startswith(b"ffprobe version 8.1.3 "):
        raise AssertionError("metadata verification requires fixed FFprobe 8.1.3")
    if file_hash(ffmpeg) != identity["encoding"]["ffmpeg_binary_sha256"] or hashlib.sha256(version).hexdigest() != identity["encoding"]["ffmpeg_version_sha256"]:
        raise AssertionError("verification FFmpeg build identity differs from generation")
    report = {
        "schema_version": 1, "status": "passed", "identity_sha256": file_hash(identity_path),
        "generator": identity["generator"], "verifier_sha256": file_hash(Path(__file__)),
        "ffmpeg_sha256": file_hash(ffmpeg), "ffprobe_sha256": file_hash(ffprobe),
        "ffmpeg_version_sha256": hashlib.sha256(version).hexdigest(),
        "ffprobe_version_sha256": hashlib.sha256(probe_version).hexdigest(),
        "raw_source_sha256": source["raw_yuv420p10le_sha256"], "inputs": [],
    }
    for encoded, codec in zip(identity["encoding"]["outputs"], ["hevc", "av1"]):
        path = args.directory / encoded["file"]
        if file_hash(path) != encoded["sha256"] or path.stat().st_size != encoded["bytes"]:
            raise AssertionError(f"{path}: encoded identity differs")
        facts = probe(path, ffprobe, codec, source)
        actual = decode(path, ffmpeg, identity)
        report["inputs"].append({"file": path.name, "encoded_sha256": encoded["sha256"], "stream": facts, **actual})
        print(f"{path.name}: decoded {actual['decoded_frames']} frames, full source/hash/histogram parity; actual low bits={actual['actual_nonzero_low_two_bits_y_u_v']}", flush=True)
    if len(report["inputs"]) != 2:
        raise AssertionError("both canonical codecs are required")
    args.report.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"verification report: {args.report}", flush=True)


if __name__ == "__main__":
    main()
