#!/usr/bin/env python3
"""Integer-only, full-resolution BT.2020 NCL/PQ canonical source, version 1."""

import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import re
import struct
import subprocess
import sys


WIDTH = 1920
HEIGHT = 1080
FRAMES = 300
FRAME_RATE = 50
GENERATOR = "pq-canonical-v1"
GRAY_NITS = ["0", "0.01", "0.1", "1", "100", "1000", "4000", "10000"]
GRAY_CODES = [64, 83, 119, 195, 509, 723, 855, 940]
COLOR_NITS = [
    [1000, 100, 10], [10, 1000, 100], [100, 10, 1000], [1000, 1000, 100],
    [100, 1000, 1000], [1000, 100, 1000], [4000, 1000, 100], [100, 1000, 4000],
]
COLOR_CODES = [
    [554, 388, 629], [606, 459, 318], [398, 688, 589], [710, 403, 521],
    [667, 542, 403], [578, 591, 612], [745, 384, 588], [674, 610, 397],
]
PLANE_SAMPLES = [WIDTH * HEIGHT, WIDTH * HEIGHT // 4, WIDTH * HEIGHT // 4]
FRAME_BYTES = sum(PLANE_SAMPLES) * 2


def words(values: list[int]) -> bytes:
    return struct.pack(f"<{len(values)}H", *values)


def histogram(values: list[int], repetitions: int) -> list[int]:
    counts = Counter(value & 3 for value in values)
    return [counts[residue] * repetitions for residue in range(4)]


def build_frame(index: int) -> tuple[bytes, list[list[int]]]:
    """Return planar unshifted LE16 source and exact low-two-bit histograms.

    Histograms count the same rows used to emit bytes, with rectangle replacements
    subtracted explicitly. Counting rows avoids scanning almost a billion words.
    """
    if not 0 <= index < FRAMES:
        raise ValueError("frame index must be in 0..299")
    top = [GRAY_CODES[x // 240] for x in range(WIDTH)]
    middle = [64 + ((x * 877 // WIDTH + index) % 877) for x in range(WIDTH)]
    bottom = [COLOR_CODES[x // 240][0] for x in range(WIDTH)]
    y = bytearray(words(top) * 180 + words(middle) * 540 + words(bottom) * 360)
    left = 2 * ((index * 7) % 864)
    upper = 180 + 2 * ((index * 3) % 222)
    rectangle = words([940] * 192)
    for row in range(upper, upper + 96):
        start = (row * WIDTH + left) * 2
        y[start:start + len(rectangle)] = rectangle
    y_histogram = [
        sum(parts) for parts in zip(
            histogram(top, 180), histogram(middle, 540), histogram(bottom, 360)
        )
    ]
    for residue, count in enumerate(histogram(middle[left:left + 192], 96)):
        y_histogram[residue] -= count
    y_histogram[940 & 3] += 192 * 96
    planes = [bytes(y)]
    histograms = [y_histogram]
    neutral = [512] * (WIDTH // 2)
    for component in [1, 2]:
        color = [COLOR_CODES[x // 120][component] for x in range(WIDTH // 2)]
        # Both gradient and moving rectangle have neutral chroma.
        planes.append(words(neutral) * 360 + words(color) * 180)
        histograms.append([
            sum(parts) for parts in zip(histogram(neutral, 360), histogram(color, 180))
        ])
    frame = b"".join(planes)
    assert len(frame) == FRAME_BYTES
    assert [sum(counts) for counts in histograms] == PLANE_SAMPLES
    return frame, histograms


def write_json(path: Path, value: dict) -> None:
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def generate(raw: Path, identity: Path) -> None:
    if identity.exists():
        raise FileExistsError(f"refusing to replace existing {identity}")
    source_hash = hashlib.sha256()
    frame_identities = []
    totals = [[0] * 4 for _ in range(3)]
    # Exclusive creation prevents a generation attempt from replacing an existing source.
    with raw.open("xb") as output:
        for index in range(FRAMES):
            frame, counts = build_frame(index)
            output.write(frame)
            source_hash.update(frame)
            frame_identities.append({
                "index": index,
                "raw_yuv420p10le_sha256": hashlib.sha256(frame).hexdigest(),
                "low_two_bit_histograms_y_u_v": counts,
            })
            for total, current in zip(totals, counts):
                for residue in range(4):
                    total[residue] += current[residue]
    write_json(identity, {
        "schema_version": 1,
        "generator": GENERATOR,
        "python_generator_sha256": file_hash(Path(__file__)),
        "python_version": sys.version.split()[0],
        "source": {
            "width": WIDTH, "height": HEIGHT, "frames": FRAMES,
            "frame_rate": [FRAME_RATE, 1], "pixel_format": "yuv420p10le",
            "storage": "planar Y, U, V; little-endian unshifted 10-bit words",
            "color_primaries": "bt2020", "transfer": "smpte2084",
            "matrix": "bt2020nc", "range": "limited", "chroma_location": "left",
            "frame_bytes": FRAME_BYTES, "raw_bytes": FRAME_BYTES * FRAMES,
            "raw_yuv420p10le_sha256": source_hash.hexdigest(),
        },
        "recipe": {
            "top": {"rows": [0, 179], "bar_width": 240, "nits": GRAY_NITS, "y_codes": GRAY_CODES, "u_v": [512, 512]},
            "middle": {"rows": [180, 719], "y": "64+((floor(x*877/1920)+frame)%877)", "u_v": [512, 512]},
            "bottom": {"rows": [720, 1079], "bar_width": 240, "rgb_nits": COLOR_NITS, "y_u_v_codes": COLOR_CODES},
            "moving_rectangle": {"width": 192, "height": 96, "left": "2*((frame*7)%864)", "top": "180+2*((frame*3)%222)", "y_u_v_codes": [940, 512, 512]},
            "quantization": "Fixed ST.2084/BT.2020 NCL code tables; nearest integer, half up; no runtime floating point or color conversion",
        },
        "low_two_bits": {
            "active_samples": sum(PLANE_SAMPLES) * FRAMES,
            "histograms_y_u_v": totals,
            "nonzero_y_u_v": [sum(counts[1:]) for counts in totals],
            "nonzero_total": sum(sum(counts[1:]) for counts in totals),
        },
        "frames": frame_identities,
    })
    print(f"{source_hash.hexdigest()}  raw yuv420p10le ({FRAME_BYTES * FRAMES} bytes)")
    print(f"low_two_bits_nonzero={sum(sum(counts[1:]) for counts in totals)}")


def file_hash(path: Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def finalize(identity: Path, ffmpeg: Path, version: Path, hevc: Path, av1: Path) -> None:
    document = json.loads(identity.read_text(encoding="utf-8"))
    dependencies = []
    linked = subprocess.run(["ldd", str(ffmpeg)], check=True, capture_output=True, text=True)
    for line in linked.stdout.splitlines():
        found = re.match(r"\s*(lib(?:x265|aom)\.so[.\d]*)\s+=>\s+(\S+)", line)
        if found:
            name, path = found.groups()
            library = Path(path).resolve(strict=True)
            dependencies.append({"name": name, "path": str(library), "sha256": file_hash(library)})
    if len(dependencies) != 2:
        raise RuntimeError("canonical generator requires identifiable shared libx265 and libaom")
    document["encoding"] = {
        "ffmpeg_version": version.read_text(encoding="utf-8"),
        "ffmpeg_version_sha256": file_hash(version),
        "ffmpeg_binary": str(ffmpeg), "ffmpeg_binary_sha256": file_hash(ffmpeg),
        "native_dependencies": sorted(dependencies, key=lambda entry: entry["name"]),
        "bash_generator_sha256": file_hash(Path(__file__).with_suffix(".sh")),
        "input_metadata": "rawvideo explicitly tagged BT2020 NCL/PQ/limited/left before -i; output carries matching tags; no implicit metadata conversion",
        "hevc_parameters": "libx265 ultrafast; lossless=1:bframes=0:keyint=50:min-keyint=50:open-gop=0:pools=none:frame-threads=1:wpp=0:log-level=error",
        "av1_parameters": "libaom-av1; cpu-used=8 crf=0 b:v=0 g=50 bf=0 row-mt=0 tiles=1x1 lag-in-frames=0 auto-alt-ref=0",
        "outputs": [{"file": path.name, "bytes": path.stat().st_size, "sha256": file_hash(path)} for path in [hevc, av1]],
    }
    write_json(identity, document)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="operation", required=True)
    source = commands.add_parser("generate")
    source.add_argument("raw", type=Path)
    source.add_argument("identity", type=Path)
    encoded = commands.add_parser("finalize")
    for name in ["identity", "ffmpeg", "version", "hevc", "av1"]:
        encoded.add_argument(name, type=Path)
    args = parser.parse_args()
    if args.operation == "generate":
        generate(args.raw, args.identity)
    else:
        finalize(args.identity, args.ffmpeg, args.version, args.hevc, args.av1)


if __name__ == "__main__":
    main()
