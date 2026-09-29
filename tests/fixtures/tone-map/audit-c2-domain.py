#!/usr/bin/env python3
"""Read sealed C-1 bytes without modifying, clipping or mapping their RGB.

This is a C-2 entry-domain audit, NOT an Annex 5 implementation. Positive
signals above one use a display-power continuation for counterexamples only;
negative signals have no project-authorized continuation and are not powered.
"""
import argparse
import decimal
import hashlib
import json
import math
from pathlib import Path
import struct

FIXTURE = Path(__file__).resolve().parent
MAGIC = b"AF-C1-OUT-v1\0"
PIXEL = struct.Struct("<7d")
EPSILON = 1e-12  # Separately expose raw excursions and material violations.
LUMA = (0.2627, 0.6780, 0.0593)  # Sealed C-1 / BT.2020 NCL coefficients.


def audit(path):
    identity = json.loads((FIXTURE / "identity.json").read_text(encoding="utf-8"))
    with path.open("rb") as source:
        digest = hashlib.file_digest(source, "sha256").hexdigest()
    if digest != identity["output"]["sha256_runs_1_2_3"][0]:
        raise ValueError("input is not the sealed C-1 canonical intermediate")
    counts = dict(samples=0, negative_components=0, above_one_components=0,
                  negative_samples=0, above_one_samples=0, source_cube_excursion_samples=0,
                  material_source_cube_excursion_samples=0, finite_nonnegative_samples=0,
                  diagnostic_linear_y_above_one_samples=0, nan_components=0, inf_components=0)
    low, high = math.inf, -math.inf
    max_y = 0.0
    with path.open("rb") as source:
        if source.read(len(MAGIC)) != MAGIC:
            raise ValueError("invalid C-1 output serialization magic")
        width, height = struct.unpack("<II", source.read(8))
        if (width, height) != (identity["input"]["width"], identity["input"]["height"]):
            raise ValueError("unexpected C-1 dimensions")
        if path.stat().st_size != len(MAGIC) + 8 + width * height * PIXEL.size:
            raise ValueError("invalid C-1 output serialization size")
        while block := source.read(PIXEL.size * 4096):
            for values in PIXEL.iter_unpack(block):
                rgb = values[:3]
                counts["samples"] += 1
                counts["nan_components"] += sum(math.isnan(v) for v in rgb)
                counts["inf_components"] += sum(math.isinf(v) for v in rgb)
                if not all(math.isfinite(v) for v in rgb):
                    continue
                low, high = min(low, *rgb), max(high, *rgb)
                negative = any(v < 0 for v in rgb)
                over = any(v > 1 for v in rgb)
                counts["negative_components"] += sum(v < 0 for v in rgb)
                counts["above_one_components"] += sum(v > 1 for v in rgb)
                counts["negative_samples"] += negative
                counts["above_one_samples"] += over
                counts["source_cube_excursion_samples"] += negative or over
                counts["material_source_cube_excursion_samples"] += any(v < -EPSILON or v > 1 + EPSILON for v in rgb)
                if not negative:
                    counts["finite_nonnegative_samples"] += 1
                    # Diagnostic continuation, not permission to qualify outside
                    # Annex 5's cube or to alter the fixed SDR dynamic range.
                    y = sum(k * v ** 2.4 for k, v in zip(LUMA, rgb))
                    max_y = max(max_y, y)
                    counts["diagnostic_linear_y_above_one_samples"] += y > 1 + EPSILON
    assert counts["samples"] == width * height
    expected = identity["diagnostics_each_run"]
    assert counts["negative_components"] == expected["output_negative_components"]
    assert counts["above_one_components"] == expected["output_above_one_components"]
    decimal.getcontext().prec = 70
    D = decimal.Decimal
    vectors = json.loads((FIXTURE / "method-a-vectors.json").read_text(encoding="utf-8"))
    counterexamples = []
    for vector in vectors["vectors"]:
        signal = list(map(D, vector["rgb"]))
        if min(signal) < 0:
            continue
        linear = [v ** D("2.4") for v in signal]
        y = sum(k * v for k, v in zip(map(D, (".2627", ".6780", ".0593")), linear))
        if y > 1:
            counterexamples.append(dict(name=vector["name"], nonlinear_rgb=vector["rgb"],
                                        diagnostic_linear_rgb=list(map(str, linear)),
                                        diagnostic_linear_y=str(y), diagnostic_nits=str(100 * y)))
    assert {v["name"] for v in counterexamples} == {"green", "cyan", "yellow"}
    # Literal printed BT.2407-0 Eq(5-4), visually checked on printed p37.
    # This evaluates a source inconsistency, NOT a corrected gamut operator.
    alpha, beta = D(".5"), D(".2")
    r = 1 + alpha
    printed_boundary = r - alpha / (beta - alpha) ** 2 * (beta - (beta ** 2 + (alpha - beta) * (r + beta - 1)).sqrt())
    printed_derivative = 1 + alpha / (2 * beta * (alpha - beta))
    return dict(audit_schema_version=1, stage_status="NOT SEALED",
                sealed_c1_commit="4d95a7e2c6560e061510a6b27ee685c1db08ae0a",
                input_sha256=digest, input_bytes=path.stat().st_size, width=width, height=height,
                epsilon=EPSILON, nonlinear_rgb_min=low, nonlinear_rgb_max=high, **counts,
                diagnostic_max_linear_y=max_y,
                diagnostic_max_nits=100 * max_y,
                negative_transfer_applied=False, preclamp_count=0, unexpected_clamp_count=0,
                annex5_mapping_run=False,
                independent_counterexample_evaluation="Decimal-70 of checked C-1 expected vectors; not a Rust gamut mapper",
                positive_only_counterexamples=counterexamples,
                printed_equation_5_4_audit=dict(alpha=str(alpha), beta=str(beta), r=str(r),
                    rolloff_value_at_upper_boundary=str(printed_boundary),
                    constant_value_above_upper_boundary="1",
                    rolloff_derivative_at_lower_boundary=str(printed_derivative),
                    identity_derivative="1", corrected_formula_implemented=False),
                blocker="Annex 5 target cube [0,1] has Y<=1. Positive-only C-1 counterexamples have Y>1: preserving Y and target containment cannot both hold without an additional dynamic-range policy.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("canonical_c1_output", type=Path)
    args = parser.parse_args()
    print(json.dumps(audit(args.canonical_c1_output), indent=2, allow_nan=False))


if __name__ == "__main__":
    main()
