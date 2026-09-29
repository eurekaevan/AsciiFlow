#!/usr/bin/env python3
"""Read-only C-2B CPU target-volume audit of every sealed C-1 pixel.

This measures the explicit component-clipping policy, not perceptual quality or
Annex 5 compliance. Decimal-70 separately evaluates source RGB -> XYZ -> target
RGB for the independent vectors; every canonical pixel follows the separate XYZ
path in f64 using those Decimal-derived coefficients. Normalized display-linear
1 means 100 cd/m² throughout.
"""
import argparse
from array import array
from contextlib import ExitStack
from decimal import Decimal, localcontext
from functools import lru_cache
import hashlib
import json
import math
from pathlib import Path
import struct

FIXTURE = Path(__file__).resolve().parent
C1_MAGIC = b"AF-C1-OUT-v1\0"
C2B_MAGIC = b"AF-C2B-OUT-v1\0"
C1_PIXEL = struct.Struct("<7d")
C2B_PIXEL = struct.Struct("<12d")
MATRIX_ID = "D65-xy-derived-BT2020-2-BT709-6-v1"
PRIMARY_TOLERANCE = 5e-14
POWER_TOLERANCE = 1e-15
ACHROMATIC_RADIUS = 1e-12
SAMPLE_STRIDE = 257


def sha256(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def read_header(stream, magic, pixel):
    if stream.read(len(magic)) != magic:
        raise ValueError("invalid serialization magic")
    dimensions = stream.read(8)
    if len(dimensions) != 8:
        raise ValueError("truncated dimensions")
    width, height = struct.unpack("<II", dimensions)
    if not width or not height:
        raise ValueError("zero serialization dimension")
    position = stream.tell()
    stream.seek(0, 2)
    size = stream.tell()
    stream.seek(position)
    if size != len(magic) + 8 + width * height * pixel.size:
        raise ValueError("invalid serialization size")
    return width, height


def pixels(stream, pixel):
    while block := stream.read(pixel.size * 4096):
        if len(block) % pixel.size:
            raise ValueError("truncated pixel")
        for values in pixel.iter_unpack(block):
            if not all(math.isfinite(v) for v in values):
                raise ValueError("non-finite serialized component")
            yield values


def bits(rgb):
    return struct.pack("<3d", *rgb)


def clip(rgb):
    # Explicit branches retain signed zero and all interior component bits.
    return tuple(0.0 if v < 0 else 1.0 if v > 1 else v for v in rgb)


def signed_power(rgb):
    return tuple(0.0 if v == 0 else math.copysign(abs(v) ** 2.4, v) for v in rgb)


def multiply(matrix, rgb):
    return tuple(sum(k * v for k, v in zip(row, rgb)) for row in matrix)


def verify_decimal_vectors(document, matrices):
    """Re-evaluate the independent matrix stages at their declared precision."""
    maximum = Decimal(0)
    with localcontext() as context:
        context.prec = 70
        for vector in document["vectors"]:
            source = tuple(map(Decimal, vector["source_linear_rgb"]))
            xyz = multiply(matrices["bt2020_to_xyz"], source)
            target = multiply(matrices["xyz_to_bt709"], xyz)
            for actual, expected in ((xyz, vector["xyz"]),
                                     (target, vector["unbounded_target_linear_rgb"])):
                error = max(abs(a - Decimal(b)) for a, b in zip(actual, expected))
                maximum = max(maximum, error)
                if error > Decimal("1e-65"):
                    raise ValueError(f"independent Decimal vector mismatch: {vector['name']}")
    return dict(vectors_checked=len(document["vectors"]), precision_decimal_digits=70,
                maximum_absolute_error=str(maximum), tolerance="1e-65")


def separate_xyz_oracle(matrices):
    """Bound repeated fixture work; cache equality includes all source bits."""
    source_matrix = tuple(tuple(map(float, row)) for row in matrices["bt2020_to_xyz"])
    inverse_matrix = tuple(tuple(map(float, row)) for row in matrices["xyz_to_bt709"])
    @lru_cache(maxsize=65536)
    def evaluate(source_bits):
        source = struct.unpack("<3d", source_bits)
        xyz = multiply(source_matrix, source)
        return xyz, multiply(inverse_matrix, xyz)
    return evaluate


def uv(xyz):
    denominator = xyz[0] + 15 * xyz[1] + 3 * xyz[2]
    if denominator == 0:
        return None
    result = (4 * xyz[0] / denominator, 9 * xyz[1] / denominator)
    if not all(math.isfinite(v) for v in result):
        raise ValueError("non-finite chromaticity diagnostic")
    return result


def hue_chroma(coordinates, white):
    if coordinates is None:
        return None, None
    u, v = (a - b for a, b in zip(coordinates, white))
    radius = math.hypot(u, v)
    return (None if radius <= ACHROMATIC_RADIUS else math.degrees(math.atan2(v, u))), radius


def statistics(values):
    """Linear interpolation between sorted ranks (p * (n - 1))."""
    if not values:
        return dict(count=0, mean=None, min=None, max=None, p50=None, p95=None, p99=None, max_abs=None)
    ordered = sorted(values)
    def quantile(p):
        position = p * (len(ordered) - 1)
        low = math.floor(position)
        high = math.ceil(position)
        return ordered[low] + (ordered[high] - ordered[low]) * (position - low)
    return dict(count=len(values), mean=math.fsum(values) / len(values), min=ordered[0],
                max=ordered[-1], p50=quantile(.5), p95=quantile(.95), p99=quantile(.99),
                max_abs=max(abs(ordered[0]), abs(ordered[-1])))


def signed_statistics(values):
    return dict(signed=statistics(values), absolute=statistics(array("d", map(abs, values))),
                zero=sum(v == 0 for v in values), negative=sum(v < 0 for v in values),
                positive=sum(v > 0 for v in values))


class CollisionSample:
    def __init__(self):
        self.seen_sources = set()
        self.groups = {}
        self.sample_count = 0

    def add(self, source, bounded, index):
        self.sample_count += 1
        key = bits(source)
        if key in self.seen_sources:
            return
        self.seen_sources.add(key)
        group = self.groups.setdefault(bits(bounded), dict(count=0, examples=[]))
        group["count"] += 1
        if len(group["examples"]) < 2:
            group["examples"].append(dict(pixel_index=index, source_linear_rgb=list(source)))

    def result(self):
        collisions = [(key, group) for key, group in self.groups.items() if group["count"] > 1]
        example = None
        if collisions:
            key, group = collisions[0]  # First row-major observed collision group.
            example = dict(bounded_linear_rgb=list(struct.unpack("<3d", key)),
                           distinct_source_count=group["count"], sources=group["examples"])
        return dict(stride=SAMPLE_STRIDE, sampled_pixels=self.sample_count,
                    distinct_sources=len(self.seen_sources), bounded_groups=len(self.groups),
                    many_to_one_groups=len(collisions),
                    collision_excess_sources=sum(group["count"] - 1 for _, group in collisions),
                    maximum_multiplicity=max((g["count"] for g in self.groups.values()), default=0),
                    example=example, scope="deterministic sample, distinct exact source f64 bit tuples")


def audit(c1_path, output_directory):
    identity = json.loads((FIXTURE / "identity.json").read_text(encoding="utf-8"))
    c1_digest = sha256(c1_path)
    if c1_digest != identity["output"]["sha256_runs_1_2_3"][0]:
        raise ValueError("input is not the sealed C-1 canonical intermediate")
    paths = [output_directory / f"target-volume-run{run}.bin" for run in (1, 2, 3)]
    digests = [sha256(path) for path in paths]
    if len(set(digests)) != 1:
        raise ValueError("C-2B three-run byte hashes differ")
    document = json.loads((FIXTURE / "c2b-vectors.json").read_text(encoding="utf-8"))
    matrices = {name: tuple(tuple(map(Decimal, row)) for row in matrix)
                for name, matrix in document["matrices"].items()}
    decimal_vectors = verify_decimal_vectors(document, matrices)
    oracle = separate_xyz_oracle(matrices)
    target_matrix = tuple(tuple(map(float, row)) for row in matrices["bt709_to_xyz"])
    white = uv((.3127 / .3290, 1.0, (1 - .3127 - .3290) / .3290))
    counts = dict(source_negative_only=0, source_above_only=0, source_both=0, source_interior=0,
                  preclip_negative_channels=0, preclip_high_channels=0,
                  preclip_negative_pixels=0, preclip_high_pixels=0, unchanged_samples=0)
    channel_low, channel_high, clip_histogram = [0] * 3, [0] * 3, [0] * 4
    extrema = {name: dict(min=[math.inf] * 3, max=[-math.inf] * 3)
               for name in ("c1_nonlinear", "source_linear", "unbounded_linear", "bounded_linear", "target_nonlinear")}
    errors = dict(source_signed_power=0.0, separate_xyz_inverse=0.0,
                  preclip_xyz_preservation=0.0, preclip_y_preservation=0.0, target_inverse_power=0.0)
    delta_y, displacement, hue_delta = array("d"), array("d"), array("d")
    chroma_before, chroma_after, chroma_delta = array("d"), array("d"), array("d")
    undefined = dict(uv_before=0, uv_after=0, uv_displacement=0,
                     hue_before=0, hue_after=0, hue_delta=0, chroma_delta=0)
    collisions, representatives = CollisionSample(), {}
    category_representatives = dict(source_negative_only=None, source_above_only=None,
                                   source_both=None, preclip_y_below_zero=None,
                                   preclip_y_below_100_cd_m2=None, preclip_y_above_100_cd_m2=None)
    representative_x = {840: "green", 1080: "cyan", 1320: "yellow"}
    with ExitStack() as stack:
        c1 = stack.enter_context(c1_path.open("rb"))
        width, height = read_header(c1, C1_MAGIC, C1_PIXEL)
        if (width, height) != (identity["input"]["width"], identity["input"]["height"]):
            raise ValueError("unexpected canonical C-1 dimensions")
        # Equal hashes establish the same bytes in runs 2/3; still check headers.
        runs = [stack.enter_context(path.open("rb")) for path in paths]
        for stream in runs:
            if read_header(stream, C2B_MAGIC, C2B_PIXEL) != (width, height):
                raise ValueError("C-2B dimensions differ from C-1")
        sample_count = 0
        for index, (input_pixel, output_pixel) in enumerate(zip(pixels(c1, C1_PIXEL), pixels(runs[0], C2B_PIXEL), strict=True)):
            signal = input_pixel[:3]
            source, unbounded, bounded, nonlinear = (output_pixel[offset:offset + 3] for offset in (0, 3, 6, 9))
            source_error = max(abs(a - b) for a, b in zip(source, signed_power(signal)))
            if source_error > POWER_TOLERANCE:
                raise ValueError(f"source signed-power mismatch at pixel {index}")
            errors["source_signed_power"] = max(errors["source_signed_power"], source_error)
            xyz, expected_unbounded = oracle(bits(source))
            inverse_error = max(abs(a - b) for a, b in zip(unbounded, expected_unbounded))
            reconstructed_xyz = multiply(target_matrix, unbounded)
            xyz_error = max(abs(a - b) for a, b in zip(xyz, reconstructed_xyz))
            y_error = abs(xyz[1] - reconstructed_xyz[1])
            if max(inverse_error, xyz_error, y_error) > PRIMARY_TOLERANCE:
                raise ValueError(f"preclip primary/XYZ preservation mismatch at pixel {index}")
            errors["separate_xyz_inverse"] = max(errors["separate_xyz_inverse"], inverse_error)
            errors["preclip_xyz_preservation"] = max(errors["preclip_xyz_preservation"], xyz_error)
            errors["preclip_y_preservation"] = max(errors["preclip_y_preservation"], y_error)
            if bits(bounded) != bits(clip(unbounded)) or bits(clip(bounded)) != bits(bounded):
                raise ValueError(f"component clamp/interior identity/idempotence mismatch at pixel {index}")
            if any(v < 0 or v > 1 for v in bounded + nonlinear):
                raise ValueError(f"output outside target cube at pixel {index}")
            power_error = max(abs(a - b ** (1 / 2.4)) for a, b in zip(nonlinear, bounded))
            if power_error > POWER_TOLERANCE:
                raise ValueError(f"target inverse-power mismatch at pixel {index}")
            errors["target_inverse_power"] = max(errors["target_inverse_power"], power_error)
            negative, high = any(v < 0 for v in signal), any(v > 1 for v in signal)
            category = "source_both" if negative and high else "source_negative_only" if negative else "source_above_only" if high else "source_interior"
            counts[category] += 1
            low_mask, high_mask = [v < 0 for v in unbounded], [v > 1 for v in unbounded]
            counts["preclip_negative_channels"] += sum(low_mask)
            counts["preclip_high_channels"] += sum(high_mask)
            counts["preclip_negative_pixels"] += any(low_mask)
            counts["preclip_high_pixels"] += any(high_mask)
            clips = sum(low_mask) + sum(high_mask)
            clip_histogram[clips] += 1
            counts["unchanged_samples"] += clips == 0
            for channel in range(3):
                channel_low[channel] += low_mask[channel]
                channel_high[channel] += high_mask[channel]
            for name, rgb in zip(extrema, (signal, source, unbounded, bounded, nonlinear)):
                for channel, value in enumerate(rgb):
                    extrema[name]["min"][channel] = min(extrema[name]["min"][channel], value)
                    extrema[name]["max"][channel] = max(extrema[name]["max"][channel], value)
            after_xyz = multiply(target_matrix, bounded)
            loss = 100 * (after_xyz[1] - reconstructed_xyz[1])
            delta_y.append(loss)
            before_uv, after_uv = uv(reconstructed_xyz), uv(after_xyz)
            before_hue, before_chroma = hue_chroma(before_uv, white)
            after_hue, after_chroma = hue_chroma(after_uv, white)
            undefined["uv_before"] += before_uv is None
            undefined["uv_after"] += after_uv is None
            if before_uv is None or after_uv is None:
                undefined["uv_displacement"] += 1
            else:
                displacement.append(math.hypot(*(a - b for a, b in zip(after_uv, before_uv))))
            undefined["hue_before"] += before_hue is None
            undefined["hue_after"] += after_hue is None
            if before_hue is None or after_hue is None:
                undefined["hue_delta"] += 1
            else:
                hue_delta.append((after_hue - before_hue + 180) % 360 - 180)
            if before_chroma is not None:
                chroma_before.append(before_chroma)
            if after_chroma is not None:
                chroma_after.append(after_chroma)
            if before_chroma is None or after_chroma is None:
                undefined["chroma_delta"] += 1
            else:
                chroma_delta.append(after_chroma - before_chroma)
            if index % SAMPLE_STRIDE == 0:
                collisions.add(source, bounded, index)
            categories = [category] if category in category_representatives else []
            if reconstructed_xyz[1] < 0:
                categories.append("preclip_y_below_zero")
            if reconstructed_xyz[1] < 1:
                categories.append("preclip_y_below_100_cd_m2")
            elif reconstructed_xyz[1] > 1:
                categories.append("preclip_y_above_100_cd_m2")
            pending_categories = [name for name in categories if category_representatives[name] is None]
            if index in representative_x or pending_categories:
                record = dict(pixel_index=index, pixel_x=index % width, pixel_y=index // width,
                    c1_nonlinear_rgb=list(signal), source_linear_rgb=list(source),
                    unbounded_target_linear_rgb=list(unbounded), bounded_target_linear_rgb=list(bounded),
                    nonlinear_target_rgb=list(nonlinear), y_before_cd_m2=100 * reconstructed_xyz[1],
                    y_after_cd_m2=100 * after_xyz[1], delta_y_cd_m2=loss)
                if index in representative_x:
                    representatives[representative_x[index]] = record
                for name in pending_categories:
                    category_representatives[name] = record
            sample_count += 1
    if sample_count != width * height:
        raise ValueError("unexpected sample count")
    return dict(audit_schema_version=1, numerical_gate_status="PASS",
        stage_sealing_authority="docs/stage5.3c2b-target-volume-cpu.md",
        scope="Internal f64 CPU reference only",
        width=width, height=height, samples=sample_count, matrix_identity=MATRIX_ID,
        normalization="Display-linear 1 = 100 cd/m²", input_sha256=c1_digest,
        input_bytes=c1_path.stat().st_size, output_sha256_runs_1_2_3=digests,
        output_bytes_per_run=[path.stat().st_size for path in paths], runs_byte_identical=True,
        independent_vectors_sha256=sha256(FIXTURE / "c2b-vectors.json"),
        independent_vector_generator_sha256=sha256(FIXTURE / "generate-c2b-vectors.py"),
        independent_vector_validation=decimal_vectors,
        independent_oracle="Every pixel: separate RGB -> XYZ -> inverse BT.709 in f64, coefficients loaded from Decimal-70 matrices",
        tolerances=dict(primary_and_preclip_xyz_y_max_abs=PRIMARY_TOLERANCE,
                        transfer_max_abs=POWER_TOLERANCE), maximum_absolute_errors=errors,
        checks=dict(source_no_preclip=True, exact_component_clamp=True,
                    interior_bit_identity=True, clip_idempotence=True, finite_target_cube=True),
        counts=counts, clipped_channels_histogram_0_1_2_3=clip_histogram,
        channel_order=["R", "G", "B"], channel_low_clips=channel_low, channel_high_clips=channel_high,
        rgb_extrema=extrema, delta_y_cd_m2=signed_statistics(delta_y),
        chromaticity_diagnostics=dict(white_d65_uv=list(white), undefined_counts=undefined,
            zero_denominator_policy="undefined", achromatic_radius_uv=ACHROMATIC_RADIUS,
            uv_displacement=statistics(displacement), hue_delta_degrees=signed_statistics(hue_delta),
            chroma_radius_before=statistics(chroma_before), chroma_radius_after=statistics(chroma_after),
            chroma_radius_delta=signed_statistics(chroma_delta), quality_threshold=None),
        quantile_definition="linear interpolation at p*(n-1) in sorted samples",
        collision_sample=collisions.result(), representatives=representatives,
        first_observed_category_representatives=category_representatives,
        empty_category_representation="null means no canonical pixel in that category")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("canonical_c1_output", type=Path)
    parser.add_argument("c2b_output_directory", type=Path)
    args = parser.parse_args()
    print(json.dumps(audit(args.canonical_c1_output, args.c2b_output_directory),
                     indent=2, allow_nan=False))


if __name__ == "__main__":
    main()
