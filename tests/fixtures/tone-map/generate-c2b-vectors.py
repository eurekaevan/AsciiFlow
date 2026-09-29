#!/usr/bin/env python3
"""Independent Decimal-70 C2B oracle; emits JSON, never invokes production code.

Derive normalized primary matrices from standard xy chromaticities. Evaluate
BT.2020 -> XYZ -> BT.709 in separate stages, then apply the explicitly selected
target-linear component clip and the inverse display power. The signed power
extension is an oracle policy for out-of-range Method A components, not a claim
that BT.1886 defines negative display light.
"""

import argparse
import decimal
import hashlib
import json
from pathlib import Path


decimal.getcontext().prec = 70
decimal.getcontext().rounding = decimal.ROUND_HALF_EVEN
D = decimal.Decimal
ZERO, ONE = D(0), D(1)
GAMMA = D("2.4")
EPSILON = D("0.0000001")
PRIMARIES = {
    "bt2020": ((".708", ".292"), (".170", ".797"), (".131", ".046")),
    "bt709": ((".64", ".33"), (".30", ".60"), (".15", ".06")),
}
D65 = (D(".3127"), D(".3290"))


def inverse(matrix):
    """Gauss-Jordan inversion with deterministic partial pivoting."""
    size = len(matrix)
    rows = [list(row) + [D(i == j) for j in range(size)]
            for i, row in enumerate(matrix)]
    for column in range(size):
        pivot = max(range(column, size), key=lambda i: abs(rows[i][column]))
        if not rows[pivot][column]:
            raise ValueError("Singular primary matrix")
        rows[column], rows[pivot] = rows[pivot], rows[column]
        scale = rows[column][column]
        rows[column] = [value / scale for value in rows[column]]
        for i in range(size):
            if i != column:
                scale = rows[i][column]
                rows[i] = [value - scale * other
                           for value, other in zip(rows[i], rows[column])]
    return [row[size:] for row in rows]


def transform(matrix, vector):
    return [sum((a * b for a, b in zip(row, vector)), ZERO) for row in matrix]


def multiply(left, right):
    return [[sum((left[i][k] * right[k][j] for k in range(3)), ZERO)
             for j in range(3)] for i in range(3)]


def normalized_primary_matrix(primaries):
    columns = [(D(x) / D(y), ONE, (ONE - D(x) - D(y)) / D(y))
               for x, y in primaries]
    basis = [list(row) for row in zip(*columns)]
    x, y = D65
    white = [x / y, ONE, (ONE - x - y) / y]
    scales = transform(inverse(basis), white)
    return [[value * scales[j] for j, value in enumerate(row)] for row in basis]


def signed_display_power(value):
    if value == ZERO:
        return ZERO
    magnitude = abs(value) ** GAMMA
    return -magnitude if value < ZERO else magnitude


def strings(values):
    return [str(value) for value in values]


def evaluate(name, input_rgb, domain, matrices, **identity):
    source = ([signed_display_power(value) for value in input_rgb]
              if domain == "bt2020_method_a_nonlinear" else input_rgb)
    xyz = transform(matrices["bt2020_to_xyz"], source)
    unbounded = transform(matrices["xyz_to_bt709"], xyz)
    bounded = [min(ONE, max(ZERO, value)) for value in unbounded]
    nonlinear = [value ** (ONE / GAMMA) if value else ZERO for value in bounded]
    return {
        "name": name,
        "input_domain": domain,
        "input_rgb": strings(input_rgb),
        **identity,
        "source_linear_rgb": strings(source),
        "xyz": strings(xyz),
        "unbounded_target_linear_rgb": strings(unbounded),
        "bounded_target_linear_rgb": strings(bounded),
        "nonlinear_target_rgb": strings(nonlinear),
        "y_before_clip": str(xyz[1]),
        "y_after_clip": str(transform(matrices["bt709_to_xyz"], bounded)[1]),
    }


def generate():
    npm2020 = normalized_primary_matrix(PRIMARIES["bt2020"])
    npm709 = normalized_primary_matrix(PRIMARIES["bt709"])
    matrices = {
        "bt2020_to_xyz": npm2020,
        "xyz_to_bt2020": inverse(npm2020),
        "bt709_to_xyz": npm709,
        "xyz_to_bt709": inverse(npm709),
    }
    matrices["bt2020_to_bt709"] = multiply(matrices["xyz_to_bt709"], npm2020)
    vectors = []

    def add(name, rgb, domain="bt2020_linear", **identity):
        vectors.append(evaluate(name, list(map(D, rgb)), domain, matrices, **identity))

    def target_to_source(target):
        return transform(matrices["xyz_to_bt2020"], transform(npm709, target))

    add("black", ("0",) * 3)
    add("white", ("1",) * 3)
    add("gray", (".18",) * 3)
    colors = {
        "red": ("1", "0", "0"), "green": ("0", "1", "0"),
        "blue": ("0", "0", "1"), "cyan": ("0", "1", "1"),
        "magenta": ("1", "0", "1"), "yellow": ("1", "1", "0"),
    }
    for name, rgb in colors.items():
        add(f"bt2020-{name}", rgb)
    for name in ("red", "green", "blue"):
        target = list(map(D, colors[name]))
        add(f"bt709-{name}-in-bt2020", target_to_source(target),
            constructed_target_linear_rgb=strings(target))
    add("below-black-neutral", ("-.01",) * 3)
    add("above-white-neutral", ("1.01",) * 3)
    for component, color in enumerate(("red", "green", "blue")):
        for boundary_name, boundary in (("low", ZERO), ("high", ONE)):
            for side, offset in (("minus", -EPSILON), ("plus", EPSILON)):
                target = [D(".25"), D(".5"), D(".75")]
                target[component] = boundary + offset
                add(f"boundary-epsilon-{boundary_name}-{color}-{side}",
                    target_to_source(target), constructed_target_linear_rgb=strings(target))

    method_a_path = Path(__file__).with_name("method-a-vectors.json")
    method_a_bytes = method_a_path.read_bytes()
    method_a = json.loads(method_a_bytes)
    for saved in method_a["vectors"]:
        add(f"c1-{saved['name']}", saved["rgb"], "bt2020_method_a_nonlinear",
            c1_vector_name=saved["name"], c1_input_nits=saved["input_nits"],
            c1_saved_rgb=saved["rgb"])

    return {
        "precision_decimal_digits": 70,
        "metadata": {
            "oracle": "Python standard-library Decimal; separate xy-derived NPM and inverse stages",
            "rounding": "ROUND_HALF_EVEN",
            "chromaticity_sources": {
                "bt2020": "ITU-R BT.2020-2 (10/2015), printed page 3, Table 3",
                "bt709": "ITU-R BT.709-6 (06/2015), Part 1, item 1.3",
                "d65": "Common D65 white, x=0.3127 y=0.3290",
            },
            "primaries_xy": {name: [list(pair) for pair in pairs]
                             for name, pairs in PRIMARIES.items()},
            "white_xy": strings(D65),
            "display_power": "sign(v) * abs(v)^2.4; signed extension is an explicit oracle policy",
            "target_policy": "Clip each BT.709 linear component to [0,1], then power 1/2.4",
            "boundary_epsilon": str(EPSILON),
            "c1_source_file": method_a_path.name,
            "c1_source_sha256": hashlib.sha256(method_a_bytes).hexdigest(),
            "c1_source_standard": method_a["standard"],
            "c1_source_precision_decimal_digits": method_a["precision_decimal_digits"],
            "y_units": "Relative display-linear luminance; white Y=1",
            "limitations": "Component clipping is bounded but neither perceptual gamut mapping nor luminance preservation. Decimal rounding may leave order-1e-69 residuals at exact boundaries.",
        },
        "matrices": {name: [strings(row) for row in matrix]
                     for name, matrix in matrices.items()},
        "vectors": vectors,
    }


def rust_table(document):
    """Emit a fixed 22-column companion without rounding the oracle values."""
    fields = ("input_rgb", "source_linear_rgb", "xyz", "unbounded_target_linear_rgb",
              "bounded_target_linear_rgb", "nonlinear_target_rgb")
    rows = []
    for vector in document["vectors"]:
        columns = [vector["name"], vector["input_domain"]]
        for field in fields:
            columns.extend(vector[field])
        columns.extend((vector["y_before_clip"], vector["y_after_clip"]))
        rows.append("\t".join(columns))
    return "\n".join(rows)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rust-table", action="store_true",
                        help="emit the fixed whitespace table instead of JSON")
    arguments = parser.parse_args()
    document = generate()
    print(rust_table(document) if arguments.rust_table else json.dumps(document, indent=2))
