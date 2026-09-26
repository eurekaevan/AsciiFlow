#!/usr/bin/env python3
"""Generate deterministic 192x108/300-frame true-10-bit YUV420P samples.

The baseline shell script upscales these samples by exactly 10x with nearest
neighbor to create the canonical 1920x1080 production input. No randomness,
clock, locale, external media, or floating-point arithmetic is involved.
"""

import struct
import sys


WIDTH = 192
HEIGHT = 108
FRAMES = 300
WORD = struct.Struct("<H")


def sample(x: int, y: int, frame: int, plane: int) -> int:
    # A horizontal/vertical gradient and moving rectangle/circle. The
    # different plane coefficients also exercise time-varying chroma.
    scale = 2 if plane else 1
    full_x = x * scale
    full_y = y * scale
    rect_x = (frame * 3) % WIDTH
    rect_y = (frame * 2) % HEIGHT
    inside_rectangle = (
        (full_x - rect_x) % WIDTH < 40
        and (full_y - rect_y) % HEIGHT < 28
    )
    circle_x = (frame * 5) % WIDTH
    circle_y = (frame * 3) % HEIGHT
    dx = (full_x - circle_x + WIDTH // 2) % WIDTH - WIDTH // 2
    dy = (full_y - circle_y + HEIGHT // 2) % HEIGHT - HEIGHT // 2
    inside_circle = dx * dx + dy * dy < 18 * 18
    base = (
        x * (5 + plane * 4)
        + y * (7 + plane * 6)
        + frame * (11 + plane * 8)
        + (241 if inside_rectangle else 0)
        + (137 if inside_circle else 0)
    )
    return 64 + base % (877 if plane == 0 else 897)


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit(f"usage: {sys.argv[0]} OUTPUT.yuv")
    with open(sys.argv[1], "wb") as output:
        for frame in range(FRAMES):
            for plane in range(3):
                width = WIDTH if plane == 0 else WIDTH // 2
                height = HEIGHT if plane == 0 else HEIGHT // 2
                for y in range(height):
                    row = bytearray(width * 2)
                    for x in range(width):
                        WORD.pack_into(row, x * 2, sample(x, y, frame, plane))
                    output.write(row)


if __name__ == "__main__":
    main()
