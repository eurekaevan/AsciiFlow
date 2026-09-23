#!/usr/bin/env python3
"""Write a deterministic 64x64, 36-frame yuv420p10le gradient sequence."""

import pathlib
import struct
import sys


WIDTH = 64
HEIGHT = 64
FRAMES = 36


def plane(width: int, height: int, frame: int, seed: int) -> bytes:
    samples = bytearray()
    for y in range(height):
        for x in range(width):
            value = (x * 29 + y * 47 + frame * 61 + seed * 173) & 1023
            samples.extend(struct.pack("<H", value))
    return samples


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit(f"usage: {sys.argv[0]} OUTPUT.yuv")
    output = pathlib.Path(sys.argv[1])
    with output.open("wb") as stream:
        for frame in range(FRAMES):
            stream.write(plane(WIDTH, HEIGHT, frame, 0))
            stream.write(plane(WIDTH // 2, HEIGHT // 2, frame, 1))
            stream.write(plane(WIDTH // 2, HEIGHT // 2, frame, 2))


if __name__ == "__main__":
    main()
