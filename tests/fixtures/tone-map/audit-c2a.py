#!/usr/bin/env python3
"""Domain geometry and formula diagnostics only; never a gamut mapper.

No pixels are changed. Signed power is a separately labelled sensitivity
experiment (BT.2408-9 section 5.3), not a new C-1 transfer contract.
"""
import argparse
from collections import Counter
from decimal import Decimal, localcontext
import hashlib
import json
import math
from pathlib import Path
import struct

FIXTURE = Path(__file__).resolve().parent
PRIMARIES = {
    "bt709": ((.64, .33), (.30, .60), (.15, .06)),
    "p3-d65": ((.68, .32), (.265, .69), (.15, .06)),
    "bt2020": ((.708, .292), (.170, .797), (.131, .046)),
}
D65 = (.3127, .3290)
LUMA = (.2627, .6780, .0593)  # Preserve C-2's diagnostic coefficient identity.
EPSILON = 1e-12
MAGIC = b"AF-C1-OUT-v1\0"
PIXEL = struct.Struct("<7d")
PATCHES = ("black", "neutral-.0001", "neutral-100", "neutral-203",
           "neutral-400", "neutral-1000", "red", "green", "blue", "cyan",
           "magenta", "yellow", "skin-203-120-80", "skin-400-200-100",
           "skin-100-70-50", "neutral-1")


def xy_xyz(xy):
    x, y = xy
    if not all(math.isfinite(v) for v in xy) or not (x >= 0 and y > 0 and x + y <= 1 + 1e-15):
        raise ValueError("invalid xy chromaticity")
    return (x / y, 1.0, (1 - x - y) / y)


def luminance_weights(primaries, white=D65):
    """Solve the normalized primary matrix's Y row; no rounded matrix oracle."""
    if len(primaries) != 3:
        raise ValueError("three primaries required")
    columns = [xy_xyz(p) for p in primaries]
    target = xy_xyz(white)
    rows = [[columns[c][r] for c in range(3)] + [target[r]] for r in range(3)]
    for c in range(3):
        pivot = max(range(c, 3), key=lambda r: abs(rows[r][c]))
        rows[c], rows[pivot] = rows[pivot], rows[c]
        divisor = rows[c][c]
        if abs(divisor) < 1e-14:
            raise ValueError("degenerate primary matrix")
        rows[c] = [v / divisor for v in rows[c]]
        for r in range(3):
            if r != c:
                factor = rows[r][c]
                rows[r] = [v - factor * p for v, p in zip(rows[r], rows[c])]
    weights = tuple(row[3] for row in rows)
    if not all(math.isfinite(w) and w > 0 for w in weights):
        raise ValueError("white must be strictly inside positive-primary cone")
    total = sum(weights)
    return tuple(w / total for w in weights)


def feasibility(primaries, peak, ranges, desired_y, white=D65):
    """Exact box-image interval test, NOT a chromaticity/hue feasibility test.

    With positive weights, Y extrema occur at the two opposite box corners.
    Their connecting segment attains every intermediate Y, proving sufficiency.
    No epsilon is used to accept an above-peak desired Y.
    """
    weights = luminance_weights(primaries, white)
    if not math.isfinite(peak) or peak <= 0 or not math.isfinite(desired_y):
        raise ValueError("finite desired Y and positive finite peak required")
    if len(ranges) != 3 or any(len(pair) != 2 or not all(math.isfinite(v) for v in pair)
                               or pair[0] > pair[1] for pair in ranges):
        raise ValueError("three finite ordered component intervals required")
    # Treat the floating coefficients as a normalized weighted mean. Summing
    # already-normalized binary floats alone can put nominal white either side
    # of peak and falsely reject white or accept nextafter(peak,+inf). Dividing
    # by the same sum makes the unit-cube endpoint exactly peak, without epsilon.
    weight_sum = math.fsum(weights)
    low = peak * (math.fsum(w * pair[0] for w, pair in zip(weights, ranges)) / weight_sum)
    high = peak * (math.fsum(w * pair[1] for w, pair in zip(weights, ranges)) / weight_sum)
    if not math.isfinite(low) or not math.isfinite(high):
        raise ValueError("luminance interval exceeds finite f64 audit domain")
    return dict(luminance_weights=weights, peak_nits=peak, component_ranges=ranges,
                desired_y_nits=desired_y, minimum_y_nits=low, maximum_y_nits=high,
                feasible=low <= desired_y <= high,
                scope="existence of some legal RGB at desired Y; no fixed chromaticity or hue")


def classify(rgb, signed=False):
    negative = any(v < 0 for v in rgb)
    over = any(v > 1 for v in rgb)
    outside = negative or over
    if negative and not signed:
        category = "negative_y_unclassified"
    else:
        linear = [math.copysign(abs(v) ** 2.4, v) for v in rgb]
        y = sum(w * v for w, v in zip(LUMA, linear))
        if y < -EPSILON:
            category = "y_below_zero"
        elif y > 1 + EPSILON:
            category = "class_b_y_above_peak"
        elif outside:
            category = "class_a_outside_source_y_feasible"
        else:
            category = "inside_source_effective_gamut"
    return category, negative, over, outside


def region(index, width):
    y, x = divmod(index, width)
    if y < 540:
        return "patch-" + PATCHES[x // 120]
    return "neutral-ramp" if y < 720 else "rgb-gradient"


def percentages(counts, total):
    # Decimal strings avoid platform-dependent formatting of percentage floats.
    with localcontext() as context:
        context.prec = 28
        return {key: str(Decimal(value) * 100 / Decimal(total)) for key, value in counts.items()}


def taxonomy(path):
    identity = json.loads((FIXTURE / "identity.json").read_text())
    historical = json.loads((FIXTURE / "c2-domain-audit.json").read_text())
    with path.open("rb") as source:
        digest = hashlib.file_digest(source, "sha256").hexdigest()
    if digest != identity["output"]["sha256_runs_1_2_3"][0]:
        raise ValueError("not the sealed C-1 canonical intermediate")
    categories = ("inside_source_effective_gamut", "class_a_outside_source_y_feasible",
                  "class_b_y_above_peak", "negative_y_unclassified", "y_below_zero")
    bounded = Counter(dict.fromkeys(categories, 0))
    signed = Counter(dict.fromkeys(categories, 0))
    intersections = Counter()
    regions = {}
    with path.open("rb") as source:
        if source.read(len(MAGIC)) != MAGIC:
            raise ValueError("invalid output magic")
        width, height = struct.unpack("<II", source.read(8))
        if (width, height) != (1920, 1080) or path.stat().st_size != len(MAGIC) + 8 + width * height * PIXEL.size:
            raise ValueError("invalid canonical dimensions/size")
        index = 0
        while block := source.read(PIXEL.size * 4096):
            for values in PIXEL.iter_unpack(block):
                rgb = values[:3]
                if not all(math.isfinite(v) for v in rgb):
                    raise ValueError("nonfinite canonical RGB")
                category, negative, over, outside = classify(rgb)
                signed_category = classify(rgb, signed=True)[0]
                bounded[category] += 1
                signed[signed_category] += 1
                intersections["negative_component"] += negative
                intersections["component_above_nominal"] += over
                intersections["negative_and_above_nominal"] += negative and over
                intersections["source_excursion"] += outside
                intersections["source_and_y_above_peak_positive_only"] += outside and category == "class_b_y_above_peak"
                intersections["negative_and_y_above_peak_signed_sensitivity"] += negative and signed_category == "class_b_y_above_peak"
                intersections["negative_and_y_feasible_signed_sensitivity"] += negative and signed_category == "class_a_outside_source_y_feasible"
                label = region(index, width)
                summary = regions.setdefault(label, Counter())
                summary["pixels"] += 1
                summary["source_excursion"] += outside
                summary["negative_component"] += negative
                summary["y_above_peak_positive_only"] += category == "class_b_y_above_peak"
                index += 1
    total = width * height
    if (index != total
            or intersections["source_excursion"] != historical["source_cube_excursion_samples"]
            or bounded["class_b_y_above_peak"] != historical["diagnostic_linear_y_above_one_samples"]
            or intersections["negative_component"] != historical["negative_samples"]):
        raise ValueError("canonical scan disagrees with preserved C-2 evidence")
    # Source-valid pixels cannot exceed white with positive normalized weights.
    strict_partition = dict(
        strict_source_and_y_valid=bounded["inside_source_effective_gamut"],
        only_y_infeasible=0,
        only_source_infeasible_known=bounded["class_a_outside_source_y_feasible"],
        both_source_and_y_infeasible=bounded["class_b_y_above_peak"],
        source_infeasible_y_unclassified=bounded["negative_y_unclassified"])
    if sum(strict_partition.values()) != total:
        raise ValueError("incomplete canonical taxonomy partition")
    return dict(schema_version=1, input_sha256=digest, total_pixels=total,
                historical_audit_sha256=hashlib.sha256((FIXTURE / "c2-domain-audit.json").read_bytes()).hexdigest(),
                preserved_historical_counts={k: historical[k] for k in (
                    "samples", "source_cube_excursion_samples", "diagnostic_linear_y_above_one_samples")},
                source_y_coefficients=LUMA, y_comparison_epsilon=EPSILON,
                source_excursion_test="raw nonlinear RGB <0 or >1; also outside under monotone signed power",
                positive_only_taxonomy=dict(bounded), positive_only_percentages=percentages(bounded, total),
                strict_partition=strict_partition, strict_partition_percentages=percentages(strict_partition, total),
                intersections=dict(intersections), intersection_percentages=percentages(intersections, total),
                signed_power_sensitivity=dict(signed), signed_power_percentages=percentages(signed, total),
                signed_power_status="diagnostic only: sign(v)*abs(v)^2.4, BT.2408-9 section5.3; NOT adopted C-1/production contract",
                spatial_regions={k: dict(v) for k, v in regions.items()},
                preclamp_count=0, changed_pixel_count=0, mapper_run=False)


def printed_rolloff(r, alpha, beta):
    """Literal BT.2407-0 printed Eq5-4, intentionally NOT corrected."""
    return r - alpha / (beta - alpha) ** 2 * (
        beta - (beta ** 2 + (alpha - beta) * (r + beta - 1)).sqrt())


def bezier_diagnostic(t, alpha, beta):
    """Derived parametric diagnostic ONLY; not an executable gamut oracle."""
    return (1 - beta + 2 * beta * t + (alpha - beta) * t * t,
            1 - beta + 2 * beta * t - beta * t * t)


def formula_audit():
    with localcontext() as context:
        context.prec = 70
        D = Decimal
        cases = []
        for a, b in ((".5", ".2"), (".1", ".2"), (".2", ".2"), ("0", ".2")):
            alpha, beta = D(a), D(b)
            case = dict(alpha=a, beta=b)
            if alpha == beta:
                case["printed_status"] = "undefined: division by zero"
            else:
                low, high = 1 - beta, 1 + alpha
                left = printed_rolloff(high, alpha, beta)
                case.update(lower_value=str(printed_rolloff(low, alpha, beta)),
                            upper_left_value=str(left), upper_right_value="1",
                            upper_value_jump=str(1 - left),
                            lower_right_derivative=str(1 + alpha / (2 * beta * (alpha - beta))),
                            lower_left_derivative="1", upper_right_derivative="0",
                            upper_left_derivative=(str(1 + 1 / (2 * (alpha - beta))) if alpha else "1"))
                samples = [printed_rolloff(low + (high - low) * D(i) / 1000, alpha, beta)
                           for i in range(1001)]
                case.update(sampled_min=str(min(samples)), sampled_max=str(max(samples)),
                            sampled_rolloff_nondecreasing=all(x <= y for x, y in zip(samples, samples[1:])),
                            sampled_rolloff_in_unit_range=all(0 <= v <= 1 for v in samples))
            # Parameterization avoids a singular alpha=beta closed form.
            pairs = [bezier_diagnostic(D(i) / 1000, alpha, beta) for i in range(1001)]
            case["derived_not_normative"] = dict(
                lower=list(map(str, pairs[0])), upper=list(map(str, pairs[-1])),
                radial_nondecreasing=all(x[0] <= y[0] for x, y in zip(pairs, pairs[1:])),
                output_nondecreasing=all(x[1] <= y[1] for x, y in zip(pairs, pairs[1:])),
                in_unit_range=all(0 <= p[1] <= 1 for p in pairs))
            cases.append(case)
        return dict(schema_version=1, arithmetic="Decimal-70", cases=cases,
                    original_author_corroboration_required=True, mapper_run=False,
                    derivative="1+alpha/(2*(alpha-beta)*sqrt(q)); q=beta^2+(alpha-beta)*(r+beta-1)",
                    second_derivative="-alpha/(4*q^(3/2))",
                    derived_candidate_status="independent quadratic Bezier diagnostic; NOT normative; not selected executable reference")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="mode", required=True)
    scan = sub.add_parser("taxonomy")
    scan.add_argument("canonical_c1_output", type=Path)
    sub.add_parser("formula")
    feasible = sub.add_parser("feasibility")
    feasible.add_argument("--primaries", choices=PRIMARIES, default="bt709")
    feasible.add_argument("--xy", nargs=6, type=float, help="override with Rxy Gxy Bxy")
    feasible.add_argument("--white", nargs=2, type=float, default=D65)
    feasible.add_argument("--peak", type=float, default=100)
    feasible.add_argument("--range", nargs=6, type=float, default=(0, 1, 0, 1, 0, 1), metavar="BOUND")
    feasible.add_argument("--desired-y", type=float, required=True)
    args = parser.parse_args()
    if args.mode == "taxonomy":
        result = taxonomy(args.canonical_c1_output)
    elif args.mode == "formula":
        result = formula_audit()
    else:
        primaries = tuple(zip(args.xy[::2], args.xy[1::2])) if args.xy else PRIMARIES[args.primaries]
        result = feasibility(primaries, args.peak, tuple(zip(args.range[::2], args.range[1::2])),
                             args.desired_y, args.white)
    print(json.dumps(result, indent=2, allow_nan=False))


if __name__ == "__main__":
    main()
