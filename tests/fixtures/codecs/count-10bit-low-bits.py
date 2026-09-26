#!/usr/bin/env python3
"""Count non-zero low-two-bit samples in decoded yuv420p10le on stdin."""

import sys


LOW_BITS_PRESENT = bytes(int(value & 3 != 0) for value in range(256))
low_bits = 0
samples = 0
while chunk := sys.stdin.buffer.read(8 * 1024 * 1024):
    if len(chunk) % 2:
        raise SystemExit("decoded yuv420p10le ended with a partial sample")
    low_bits += chunk[::2].translate(LOW_BITS_PRESENT).count(1)
    samples += len(chunk) // 2
if samples == 0:
    raise SystemExit("no decoded samples")
print(f"non_zero_low_two_bits={low_bits} total_samples={samples} ratio={low_bits / samples:.6f}")
