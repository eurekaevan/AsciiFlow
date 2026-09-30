#!/usr/bin/env python3
"""Generate and identify the fixed legal-domain C3 PQ source."""

import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import re
import struct
import subprocess
import sys


WIDTH, HEIGHT, FRAMES, RATE = 1920, 1080, 300, 50
GENERATOR = "c3-pq-legal-v1"
GRAY_NITS = ["0", "0.01", "0.1", "1", "10", "100", "203", "400", "600", "800", "900"]
GRAY_CODES = [64, 83, 119, 195, 327, 509, 573, 636, 674, 701, 713]
BAR_CODES = [64, 83, 195, 327, 509, 573, 636, 674, 701, 713]
COLOR_NITS = [
    [800, 100, 10], [10, 800, 100], [100, 10, 800], [800, 800, 100],
    [100, 800, 800], [800, 100, 800], [600, 300, 100], [100, 400, 700],
]
COLOR_CODES = [
    [549, 391, 618], [591, 467, 328], [397, 678, 590], [690, 414, 520],
    [651, 539, 414], [571, 583, 602], [620, 452, 549], [606, 557, 445],
]
PLANE_SAMPLES = [WIDTH * HEIGHT, WIDTH * HEIGHT // 4, WIDTH * HEIGHT // 4]
FRAME_BYTES = WIDTH * HEIGHT * 3


def pack_words(values, shift=0):
    if shift:
        values = [value << shift for value in values]
    return struct.pack(f"<{len(values)}H", *values)


def digest(path):
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def hist(values):
    counts = Counter(value & 3 for value in values)
    return [counts[i] for i in range(4)]


def build_frame(index):
    if not 0 <= index < FRAMES:
        raise ValueError("frame index outside 0..299")
    y = bytearray()
    yh = [0] * 4
    top = [BAR_CODES[x // 192] for x in range(WIDTH)]
    middle = [64 + ((x * 650 // WIDTH + index) % 650) for x in range(WIDTH)]
    bottom = [COLOR_CODES[x // 240][0] for x in range(WIDTH)]
    top_bytes, middle_bytes, bottom_bytes = (pack_words(row, 6) for row in (top, middle, bottom))
    top_hist, middle_hist, bottom_hist = map(hist, (top, middle, bottom))
    y.extend(top_bytes * 180)
    y.extend(middle_bytes * 540)
    y.extend(bottom_bytes * 360)
    for residue in range(4):
        yh[residue] = 180 * top_hist[residue] + 540 * middle_hist[residue] + 360 * bottom_hist[residue]

    left = 2 * ((index * 7) % 864)
    upper = 180 + 2 * ((index * 3) % 222)
    rectangle = pack_words([713] * 192, 6)
    for row in range(upper, upper + 96):
        start = row * WIDTH * 2 + left * 2
        y[start:start + len(rectangle)] = rectangle
    rect_old_hist = hist(middle[left:left + 192])
    rect_new_hist = hist([713] * 192)
    for residue in range(4):
        yh[residue] += 96 * (rect_new_hist[residue] - rect_old_hist[residue])

    uv = bytearray()
    uvh, uvk = [0] * 4, [0] * 4
    neutral = pack_words([512] * WIDTH, 6)
    uv.extend(neutral * 90)
    uvh[512 & 3] += (WIDTH // 2) * 90
    uvk[512 & 3] += (WIDTH // 2) * 90
    uv.extend(neutral * 270)
    uvh[512 & 3] += (WIDTH // 2) * 270
    uvk[512 & 3] += (WIDTH // 2) * 270
    chroma_row = bytearray()
    chroma_h, chroma_k = [0] * 4, [0] * 4
    for x in range(WIDTH // 2):
        _, cb, cr = COLOR_CODES[x // 120]
        chroma_row.extend(struct.pack("<HH", cb << 6, cr << 6))
        chroma_h[cb & 3] += 1
        chroma_k[cr & 3] += 1
    uv.extend(chroma_row * 180)
    for residue in range(4):
        uvh[residue] += 180 * chroma_h[residue]
        uvk[residue] += 180 * chroma_k[residue]
    p010 = bytes(y + uv)
    assert len(p010) == FRAME_BYTES
    return p010, [yh, uvh, uvk]


def generate(raw, identity):
    if identity.exists():
        raise FileExistsError(identity)
    whole = hashlib.sha256()
    frames, totals = [], [[0] * 4 for _ in range(3)]
    with raw.open("xb") as output:
        for index in range(FRAMES):
            frame, counts = build_frame(index)
            output.write(frame)
            whole.update(frame)
            frames.append({"index": index, "tight_p010_sha256": hashlib.sha256(frame).hexdigest(),
                           "low_two_bit_histograms_y_u_v": counts})
            for target, current in zip(totals, counts):
                for residue in range(4):
                    target[residue] += current[residue]
    doc = {
        "schema_version": 1, "generator": GENERATOR,
        "python_generator_sha256": digest(Path(__file__)), "python_version": sys.version.split()[0],
        "source": {"width": WIDTH, "height": HEIGHT, "frames": FRAMES, "frame_rate": [RATE, 1],
                   "pixel_format": "p010le", "storage": "tight little-endian P010; Y then interleaved UV; 10-bit codes shifted left 6",
                   "color_primaries": "bt2020", "transfer": "smpte2084", "matrix": "bt2020nc",
                   "range": "limited", "chroma_location": "left", "frame_bytes": FRAME_BYTES,
                   "raw_bytes": FRAME_BYTES * FRAMES, "tight_p010_sha256": whole.hexdigest()},
        "recipe": {"gray_patch_nits": GRAY_NITS, "gray_y_codes": GRAY_CODES,
                   "top": {"rows": [0, 179], "bar_width": 192, "y_codes": BAR_CODES, "u_v": [512, 512]},
                   "middle": {"rows": [180, 719], "y_code": "64+((floor(x*650/1920)+frame)%650)", "u_v": [512, 512]},
                   "bottom": {"rows": [720, 1079], "bar_width": 240, "designed_rgb_nits": COLOR_NITS, "y_u_v_codes": COLOR_CODES},
                   "moving_rectangle": {"width": 192, "height": 96, "left": "2*((frame*7)%864)", "top": "180+2*((frame*3)%222)", "y_u_v_codes": [713, 512, 512]},
                   "quantization": "fixed supplied integer code tables; source values are never clipped or renormalized"},
        "low_two_bits": {"active_samples_per_plane": PLANE_SAMPLES, "histograms_y_u_v": totals,
                         "nonzero_y_u_v": [sum(v[1:]) for v in totals], "nonzero_total": sum(sum(v[1:]) for v in totals)},
        "frames": frames,
    }
    identity.write_text(json.dumps(doc, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"{whole.hexdigest()}  tight P010 ({FRAME_BYTES * FRAMES} bytes)")
    print(f"low_two_bits_nonzero={doc['low_two_bits']['nonzero_total']}")


def finalize(identity, ffmpeg, version, hevc, av1):
    doc = json.loads(identity.read_text(encoding="utf-8"))
    linked = subprocess.run(["ldd", str(ffmpeg)], check=True, capture_output=True, text=True)
    native = []
    for line in linked.stdout.splitlines():
        found = re.match(r"\s*(lib(?:x265|aom)\.so[.\d]*)\s+=>\s+(\S+)", line)
        if found:
            name, path = found.groups()
            path = Path(path).resolve(strict=True)
            native.append({"name": name, "path": str(path), "sha256": digest(path)})
    if len(native) != 2:
        raise RuntimeError("requires identifiable shared libx265 and libaom")
    doc["encoding"] = {
        "ffmpeg_version": version.read_text(encoding="utf-8"), "ffmpeg_version_sha256": digest(version),
        "ffmpeg_binary": str(ffmpeg), "ffmpeg_binary_sha256": digest(ffmpeg),
        "native_dependencies": sorted(native, key=lambda item: item["name"]),
        "bash_generator_sha256": digest(Path(__file__).with_suffix(".sh")),
        "input_metadata": "raw P010 explicitly tagged BT.2020 NCL/PQ/limited/left before input; output tags match",
        "hevc_parameters": "libx265 ultrafast; lossless=1:bframes=0:keyint=50:min-keyint=50:open-gop=0:pools=none:frame-threads=1:wpp=0:log-level=error",
        "av1_parameters": "libaom-av1; cpu-used=8 crf=0 b:v=0 g=50 bf=0 row-mt=0 tiles=1x1 lag-in-frames=0 auto-alt-ref=0",
        "outputs": [{"file": p.name, "bytes": p.stat().st_size, "sha256": digest(p)} for p in (hevc, av1)],
    }
    identity.write_text(json.dumps(doc, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="operation", required=True)
    g = sub.add_parser("generate")
    g.add_argument("raw", type=Path)
    g.add_argument("identity", type=Path)
    f = sub.add_parser("finalize")
    for name in ("identity", "ffmpeg", "version", "hevc", "av1"):
        f.add_argument(name, type=Path)
    args = parser.parse_args()
    if args.operation == "generate":
        generate(args.raw, args.identity)
    else:
        finalize(args.identity, args.ffmpeg, args.version, args.hevc, args.av1)


if __name__ == "__main__":
    main()
