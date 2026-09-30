#!/usr/bin/env python3
"""Audit actual C3 SPIR-V arithmetic and sealed matrix representations."""

import argparse
import hashlib
import json
from pathlib import Path
import re
import struct
import subprocess


def bits32(value):
    return struct.unpack("<I", struct.pack("<f", value))[0]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("module", type=Path)
    args = parser.parse_args()
    text = subprocess.run(["spirv-dis", str(args.module)], check=True,
                          capture_output=True, text=True).stdout
    float_type = re.search(r"(%\S+) = OpTypeFloat 32\b", text).group(1)
    constants = set()
    for value in re.findall(r"OpConstant " + re.escape(float_type) + r" (\S+)", text):
        constants.add(bits32(float(value)))
    uint_type = re.search(r"(%\S+) = OpTypeInt 32 0\b", text).group(1)
    uint_constants = {int(v, 0) for v in re.findall(
        r"OpConstant " + re.escape(uint_type) + r" (\S+)", text)}
    # Read the immutable CPU constant, not an independently rounded decimal table.
    root = Path(__file__).resolve().parent.parent
    source = root / "crates/asciiflow-core/src/sdr_target_volume.rs"
    cpu = source.read_text(encoding="utf-8")
    matrix = re.search(r"pub const BT2020_TO_BT709:.*?=\s*\[(.*?)\];", cpu, re.S)
    values = [float(v.replace("_", "")) for v in
              re.findall(r"[-+]?\d[\d_]*\.\d[\d_]*(?:[eE][-+]?\d+)?", matrix.group(1))]
    assert len(values) == 9
    coefficients = []
    for i, value in enumerate(values):
        hi_bits = bits32(value)
        hi = struct.unpack("<f", struct.pack("<I", hi_bits))[0]
        low_bits = bits32(value - hi)
        present = hi_bits in constants
        coefficients.append({"row": i // 3, "column": i % 3,
                             "sealed_f64": value,
                             "f64_bits": struct.pack(">d", value).hex(),
                             "f32_bits": f"{hi_bits:08x}",
                             "residual_f32_bits": f"{low_bits:08x}",
                             "residual_used": i % 3 != 1,
                             "actual_spirv_residual_uint_present": low_bits in uint_constants,
                             "actual_spirv_constant_present": present})
    capabilities = re.findall(r"OpCapability (\S+)", text)
    operations = {op: len(re.findall(r"\b" + op + r"\b", text)) for op in
                  ["OpFAdd", "OpFSub", "OpFMul", "OpFDiv", "Fma", "Pow", "Log", "Exp", "NoContraction"]}
    fma_ids = re.findall(r"(%\S+) = OpExtInst .*? Fma\b", text)
    no_contraction_ids = set(re.findall(r"OpDecorate (%\S+) NoContraction", text))
    report = {"module": str(args.module),
              "module_sha256": hashlib.sha256(args.module.read_bytes()).hexdigest(),
              "cpu_matrix_source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
              "capabilities": capabilities, "float_widths": re.findall(r"OpTypeFloat (\d+)", text),
              "operations": operations, "coefficients": coefficients,
              "fma_instructions": [{"id": i, "no_contraction": i in no_contraction_ids} for i in fma_ids],
              "scope": "module-level instruction census; expression order is additionally audited against disassembly and captured terms"}
    print(json.dumps(report, indent=2))
    assert "Float64" not in capabilities and "Float16" not in capabilities
    assert all(c["actual_spirv_constant_present"] for c in coefficients)
    assert all(c["actual_spirv_residual_uint_present"] for c in coefficients if c["residual_used"])
    assert operations["NoContraction"] > 0 and operations["Fma"] > 0


if __name__ == "__main__":
    main()
