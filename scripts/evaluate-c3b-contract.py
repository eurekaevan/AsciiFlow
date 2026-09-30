#!/usr/bin/env python3
"""Evaluate a derived N3 candidate; never seal C3 or change the historical gate."""
import argparse
import hashlib
import json
from pathlib import Path

EPSILON = 1 / 1792  # Half a 10-bit limited chroma code, not an observed maximum.
ROOT = Path(__file__).resolve().parent.parent
FROZEN = {
    "crates/asciiflow-core/src/tone_map_bt2446.rs": "209208b850936869e6a8b73c960b2cf7be9d957ceb544eaca1bbe4fe88407107",
    "crates/asciiflow-core/src/sdr_target_volume.rs": "ca92bb3d6d26113254788a0a09549e064ce3c37be0db7b7636f71ea6dcf836b5",
}
INPUTS = {
    "hevc-main10-pq-c3-legal-v1.mp4": "eb8d8c42cd99369b99031900e8861c08f611bfcc662b8d0eb741eb695a0c366a",
    "av1-main10-pq-c3-legal-v1.mp4": "7eb4fd7e7513fc181ad8489e13edbac10a88a8f3a7947d2f958d36a0ce08e7c4",
}


def evaluate(canonical, real):
    cases = [case for case in canonical["cases"] if case["variant"] == 0]
    if sorted(case["path"] for case in cases) != ["BC", "C"]:
        raise ValueError("expected exactly two canonical best-f32 cases")
    checks = []
    for case in cases:
        if (case["width"], case["height"]) != (1920, 1080):
            raise ValueError("wrong canonical geometry")
        numeric = case["max_abs_nonlinear"] <= EPSILON and all(
            c["max"] <= 1 and c["over1"] == 0 and
            c["exact"] + c["delta1"] + c["over1"] == 1920 * 1080
            for c in case["quantization"]["limited444_BT709_10"])
        checks.append({"source": "canonical-" + case["path"],
                       "numerical_candidate_pass": numeric,
                       "semantic_parity_pass": case["mask_mismatch_pixels"] == 0,
                       "validation_pass": case["validation_errors"] == 0})
    expected_names = {"hevc-main10-pq-c3-legal-v1.mp4", "av1-main10-pq-c3-legal-v1.mp4"}
    if len(real["reports"]) != 2 or {r["source"] for r in real["reports"]} != expected_names:
        raise ValueError("expected both legal-source records exactly once")
    for record in real["reports"]:
        if record["input_sha256"] != INPUTS[record["source"]]:
            raise ValueError("wrong real-source identity")
        if record["frames"] != 300 or record["pixels"] != 300 * 1920 * 1080:
            raise ValueError("incomplete real-source record")
        numeric = max(record["max_nonlinear_abs_by_rgb"]) <= EPSILON and all(
            c["max"] <= 1 and c["over_1"] == 0 and
            c["exact"] + c["delta_1"] + c["over_1"] == record["pixels"]
            for c in record["limited_bt709_444_10bit"]["code_delta"])
        checks.append({"source": record["source"], "numerical_candidate_pass": numeric,
                       "semantic_parity_pass": record["glyph_mismatches"] == 0 and
                       record["coverage_mismatches"] == 0 and record["mask_mismatched_pixels"] == 0,
                       "validation_pass": record["vulkan_validation_errors"] == 0})
    return {"candidate": "N3: finite/bounded RGB, absolute nonlinear error<=1/1792, limited444 YUV10 max<=1 and over1=0",
            "epsilon": EPSILON, "checks": checks,
            "numerical_candidate_pass": all(c["numerical_candidate_pass"] for c in checks),
            "semantic_parity_pass": all(c["semantic_parity_pass"] for c in checks),
            "validation_pass": all(c["validation_pass"] for c in checks),
            "scope": "corpus evaluation only; no stage sealing, output-format selection or automatic gate replacement"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("canonical", type=Path)
    parser.add_argument("real", type=Path)
    args = parser.parse_args()
    for path, expected in FROZEN.items():
        if hashlib.sha256((ROOT / path).read_bytes()).hexdigest() != expected:
            raise ValueError("frozen CPU oracle changed: " + path)
    print(json.dumps(evaluate(json.loads(args.canonical.read_text()),
                              json.loads(args.real.read_text())), indent=2, allow_nan=False))


if __name__ == "__main__":
    main()
