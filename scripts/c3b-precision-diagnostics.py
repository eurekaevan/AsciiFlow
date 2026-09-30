#!/usr/bin/env python3
"""Test-only numerical comparisons; these diagnostics define no production contract.

RGB inputs are nonlinear, normalized BT.709 triples. The limited-range Y'CbCr
projection is a mathematical 4:4:4 diagnostic only: it does not model a codec,
chroma subsampling, the production renderer, or the production 4:2:0 pipeline.
The signed power display model is likewise a diagnostic, not an EOTF standard.
"""

import argparse
import collections
import json
import math
import struct
import sys

RGB_BITS = (8, 10, 12, 16)
LUMA_WEIGHTS = (0.2126, 0.7152, 0.0722)
DISPLAY_Y_WEIGHTS = (0.21263900587151036, 0.7151686787677559, 0.07219231536073371)


def _triple(value):
    if len(value) != 3:
        raise ValueError("each RGB vector must contain exactly three components")
    if any(isinstance(x, bool) or not isinstance(x, (int, float)) for x in value):
        raise ValueError("RGB components must be numbers")
    return tuple(float(x) for x in value)


def quantize_rgb(rgb, bits):
    """Half-up normalized RGB projection, without clipping invalid inputs."""
    if bits not in RGB_BITS:
        raise ValueError("RGB bit depth must be 8, 10, 12, or 16")
    rgb = _triple(rgb)
    if any(not math.isfinite(x) or not 0 <= x <= 1 for x in rgb):
        raise ValueError("quantization requires finite RGB components in [0, 1]")
    maximum = (1 << bits) - 1
    return tuple(math.floor(x * maximum + 0.5) for x in rgb)


def limited_ycbcr444(rgb, bits=8):
    """BT.709 Y'CbCr limited-range integer diagnostic at 8 or 10 bits."""
    if bits not in (8, 10):
        raise ValueError("Y'CbCr diagnostic bit depth must be 8 or 10")
    rgb = _triple(rgb)
    # Reuse validation, without changing or clipping the vector.
    quantize_rgb(rgb, 8)
    r, g, b = rgb
    y = sum(weight * component for weight, component in zip(LUMA_WEIGHTS, rgb))
    cb = (b - y) / (2 * (1 - LUMA_WEIGHTS[2]))
    cr = (r - y) / (2 * (1 - LUMA_WEIGHTS[0]))
    scale = 1 << (bits - 8)
    return tuple(math.floor(code * scale + 0.5)
                 for code in (16 + 219 * y, 128 + 224 * cb, 128 + 224 * cr))


def display_linear(rgb):
    """Signed power 2.4 diagnostic; negative values retain their sign."""
    return tuple(math.copysign(abs(x) ** 2.4, x) for x in _triple(rgb))


def luminance_nits(rgb):
    """100-nit weighted luminance under the signed power diagnostic model."""
    return 100 * sum(w * x for w, x in zip(DISPLAY_Y_WEIGHTS, display_linear(rgb)))


def vector_details(vector):
    """Preserve a failure vector and explain its projections without hiding it."""
    cpu, gpu = _triple(vector["cpu"]), _triple(vector["gpu"])
    report = compare_pairs([(cpu, gpu)])
    report["input"] = vector
    report["projections"] = {
        str(bits): {"cpu": quantize_rgb(cpu, bits), "gpu": quantize_rgb(gpu, bits),
                    "cpu_boundary_distance": [boundary_distance(v, bits) for v in cpu],
                    "gpu_boundary_distance": [boundary_distance(v, bits) for v in gpu]}
        for bits in RGB_BITS
    }
    report["linear_rgb"] = {"cpu": display_linear(cpu), "gpu": display_linear(gpu)}
    report["luminance_nits"] = {"cpu": luminance_nits(cpu), "gpu": luminance_nits(gpu)}
    return report


def boundary_distance(value, bits):
    """Distance in code units to the nearest in-range half-up RGB boundary."""
    if bits not in RGB_BITS or not math.isfinite(value) or not 0 <= value <= 1:
        raise ValueError("boundary distance requires a supported depth and value in [0, 1]")
    maximum = (1 << bits) - 1
    position = value * maximum
    nearest = min(maximum - 0.5, max(0.5, math.floor(position) + 0.5))
    return abs(position - nearest)


def _percentile(histogram, fraction):
    """Nearest-rank percentile of absolute integer code differences."""
    count = sum(histogram.values())
    if not count:
        return None
    rank = max(1, math.ceil(fraction * count))
    cumulative = 0
    for delta, frequency in sorted(histogram.items()):
        cumulative += frequency
        if cumulative >= rank:
            return delta


def _histogram_report(histogram):
    count = sum(histogram.values())
    return {
        "samples": count,
        "exact": histogram[0],
        "delta_1": histogram[1],
        "over_1": count - histogram[0] - histogram[1],
        "max": max(histogram) if count else None,
        **{label: _percentile(histogram, fraction) for label, fraction in
           (("p50", .5), ("p95", .95), ("p99", .99), ("p999", .999))},
        "absolute_delta_histogram": {str(k): v for k, v in sorted(histogram.items()) if v},
    }


class _Errors:
    def __init__(self):
        self.count = 0
        self.absolute_sum = 0.0
        self.maximum = 0.0
        self.relative_count = 0
        self.relative_maximum = 0.0

    def add(self, cpu, gpu):
        delta = abs(gpu - cpu)
        self.count += 1
        self.absolute_sum += delta
        self.maximum = max(self.maximum, delta)
        if cpu != 0:
            self.relative_count += 1
            self.relative_maximum = max(self.relative_maximum, delta / abs(cpu))

    def report(self):
        return {"samples": self.count,
                "max_absolute": self.maximum if self.count else None,
                "mean_absolute": self.absolute_sum / self.count if self.count else None,
                "relative_samples_nonzero_cpu": self.relative_count,
                "max_relative_to_cpu": self.relative_maximum if self.relative_count else None}


def compare_pairs(pairs):
    """Compare an iterable of (CPU RGB, GPU RGB), preserving input values."""
    errors = {name: [_Errors() for _ in range(3)] for name in ("nonlinear_rgb", "display_linear")}
    luminance = _Errors()
    projections = {f"rgb{bits}": [collections.Counter() for _ in range(3)] for bits in RGB_BITS}
    projections.update({f"limited444_{bits}": [collections.Counter() for _ in range(3)] for bits in (8, 10)})
    boundaries = {str(bits): {side: [None] * 3 for side in ("cpu", "gpu")} for bits in RGB_BITS}
    invalid = {side: {"nonfinite_components": 0, "out_of_bounds_components": 0} for side in ("cpu", "gpu")}
    total = projected = 0
    for cpu, gpu in pairs:
        cpu, gpu = _triple(cpu), _triple(gpu)
        total += 1
        valid = True
        for side, rgb in (("cpu", cpu), ("gpu", gpu)):
            for channel, x in enumerate(rgb):
                if not math.isfinite(x):
                    invalid[side]["nonfinite_components"] += 1
                    valid = False
                elif not 0 <= x <= 1:
                    invalid[side]["out_of_bounds_components"] += 1
                    valid = False
                else:
                    for bits in RGB_BITS:
                        distance = boundary_distance(x, bits)
                        previous = boundaries[str(bits)][side][channel]
                        boundaries[str(bits)][side][channel] = distance if previous is None else min(previous, distance)
        for channel, (c, g) in enumerate(zip(cpu, gpu)):
            if math.isfinite(c) and math.isfinite(g):
                errors["nonlinear_rgb"][channel].add(c, g)
        if not valid:
            continue
        projected += 1
        for channel, (c, g) in enumerate(zip(display_linear(cpu), display_linear(gpu))):
            errors["display_linear"][channel].add(c, g)
        luminance.add(luminance_nits(cpu), luminance_nits(gpu))
        for name, histograms in projections.items():
            if name.startswith("rgb"):
                c, g = quantize_rgb(cpu, int(name[3:])), quantize_rgb(gpu, int(name[3:]))
            else:
                c, g = limited_ycbcr444(cpu, int(name.rsplit("_", 1)[1])), limited_ycbcr444(gpu, int(name.rsplit("_", 1)[1]))
            for histogram, a, b in zip(histograms, c, g):
                histogram[abs(a - b)] += 1
    return {
        "scope": "test-only mathematical diagnostics; no production, codec, or 4:2:0 contract",
        "channel_order": {"rgb": ["R", "G", "B"], "limited444": ["Y", "Cb", "Cr"]},
        "pairs": total, "finite_in_bounds_pairs_projected": projected,
        "invalid_inputs": invalid,
        "errors": {name: [error.report() for error in channels] for name, channels in errors.items()},
        "luminance_100_nits_errors": luminance.report(),
        "code_deltas": {name: [_histogram_report(h) for h in channels] for name, channels in projections.items()},
        "minimum_rgb_boundary_distance_code_units": boundaries,
        "notes": ["Relative error uses nonzero CPU references only.",
                  "Display and code projections use pairs with all components finite and in [0, 1].",
                  "Percentiles use nearest rank over absolute per-channel integer code differences."],
    }


def binary_pairs(cpu_path, gpu_path):
    """Stream headerless little-endian float64 CPU / float32 GPU RGB triples."""
    with open(cpu_path, "rb") as cpu_file, open(gpu_path, "rb") as gpu_file:
        while True:
            cpu = cpu_file.read(24)
            gpu = gpu_file.read(12)
            if not cpu and not gpu:
                return
            if len(cpu) != 24 or len(gpu) != 12:
                raise ValueError("binary inputs must contain equal numbers of complete RGB triples")
            yield struct.unpack("<3d", cpu), struct.unpack("<3f", gpu)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("vectors", nargs="?", help="JSON array of {cpu: [R,G,B], gpu: [R,G,B]}; defaults to stdin")
    parser.add_argument("--cpu-f64", help="headerless little-endian float64 RGB file")
    parser.add_argument("--gpu-f32", help="headerless little-endian float32 RGB file")
    parser.add_argument("--details", action="store_true", help="preserve per-vector diagnostics for small JSON inputs")
    args = parser.parse_args(argv)
    if bool(args.cpu_f64) != bool(args.gpu_f32) or (args.vectors and args.cpu_f64) or (args.details and args.cpu_f64):
        parser.error("provide both binary paths, or a JSON input")
    try:
        if args.cpu_f64:
            pairs = binary_pairs(args.cpu_f64, args.gpu_f32)
        else:
            if args.vectors:
                with open(args.vectors, encoding="utf-8") as source:
                    vectors = json.load(source)
            else:
                vectors = json.load(sys.stdin)
            if not isinstance(vectors, list):
                raise ValueError("JSON input must be an array")
            pairs = ((vector["cpu"], vector["gpu"]) for vector in vectors)
        report = compare_pairs(pairs)
        if args.details:
            report["vectors"] = [vector_details(vector) for vector in vectors]
        report["signal_matrix_source"] = "ITU-R BT.709-6 sections 3.2-3.4"
        report["luminance_model"] = "sealed C2B signed power2.4, D65 xy-derived BT709 XYZ Y, 100nit white"
        print(json.dumps(report, indent=2, allow_nan=False))
    except (OSError, ValueError, TypeError, KeyError, OverflowError) as error:
        parser.exit(2, f"error: {error}\n")


if __name__ == "__main__":
    main()
