#!/usr/bin/env python3
"""Behavior checks for the test-only precision diagnostic utility."""

import importlib.util
import math
from pathlib import Path
import struct
import tempfile
import unittest
import sys

sys.dont_write_bytecode = True

spec = importlib.util.spec_from_file_location(
    "diagnostics", Path(__file__).with_name("c3b-precision-diagnostics.py"))
diagnostics = importlib.util.module_from_spec(spec)
spec.loader.exec_module(diagnostics)


class PrecisionDiagnosticsTests(unittest.TestCase):
    def test_output_budget_is_derived_before_observation(self):
        kr, kg, kb = diagnostics.LUMA_WEIGHTS
        rows = ((kr, kg, kb),
                (-kr / 1.8556, -kg / 1.8556, (1 - kb) / 1.8556),
                ((1 - kr) / 1.5748, -kg / 1.5748, -kb / 1.5748))
        for row in rows:
            self.assertAlmostEqual(sum(abs(c) for c in row), 1)
        epsilon = 1 / 1792
        self.assertLess(876 * epsilon, .5)
        self.assertEqual(896 * epsilon, .5)
        self.assertLess(1023 * epsilon, 1)
        # Exercise both sides of many half-up boundaries, not just code centers.
        for code in range(64, 940):
            y = (code + .5 - 64) / 876
            cpu = (max(0, y - epsilon / 2),) * 3
            gpu = (min(1, y + epsilon / 2),) * 3
            a, b = diagnostics.limited_ycbcr444(cpu, 10), diagnostics.limited_ycbcr444(gpu, 10)
            self.assertTrue(all(abs(c - g) <= 1 for c, g in zip(a, b)))

    def test_rgb_half_up_and_code_bounds(self):
        for bits in diagnostics.RGB_BITS:
            maximum = (1 << bits) - 1
            self.assertEqual(diagnostics.quantize_rgb((0, .5, 1), bits), (0, (maximum + 1) // 2, maximum))
            # Exhaustively check every representable code's center and every half tie.
            for code in range(maximum + 1):
                self.assertEqual(diagnostics.quantize_rgb((code / maximum,) * 3, bits), (code,) * 3)
                if code < maximum:
                    tie = (code + .5) / maximum
                    self.assertEqual(diagnostics.quantize_rgb((tie,) * 3, bits), (code + 1,) * 3)
                    self.assertEqual(diagnostics.boundary_distance(tie, bits), 0)

    def test_limited_black_white_and_primaries(self):
        self.assertEqual(diagnostics.limited_ycbcr444((0, 0, 0)), (16, 128, 128))
        self.assertEqual(diagnostics.limited_ycbcr444((1, 1, 1)), (235, 128, 128))
        self.assertEqual(diagnostics.limited_ycbcr444((1, 0, 0)), (63, 102, 240))
        self.assertEqual(diagnostics.limited_ycbcr444((0, 1, 0)), (173, 42, 26))
        self.assertEqual(diagnostics.limited_ycbcr444((0, 0, 1)), (32, 240, 118))
        self.assertEqual(diagnostics.limited_ycbcr444((0, 0, 0), 10), (64, 512, 512))
        self.assertEqual(diagnostics.limited_ycbcr444((1, 1, 1), 10), (940, 512, 512))
        for bits in (8, 10):
            scale = 1 << (bits - 8)
            for r in (0, .5, 1):
                for g in (0, .5, 1):
                    for b in (0, .5, 1):
                        y, cb, cr = diagnostics.limited_ycbcr444((r, g, b), bits)
                        self.assertTrue(16 * scale <= y <= 235 * scale)
                        self.assertTrue(16 * scale <= cb <= 240 * scale)
                        self.assertTrue(16 * scale <= cr <= 240 * scale)

    def test_display_and_relative_errors(self):
        self.assertEqual(diagnostics.display_linear((-1, 0, 1)), (-1, 0, 1))
        self.assertAlmostEqual(diagnostics.luminance_nits((1, 1, 1)), 100)
        report = diagnostics.compare_pairs([((0, .5, 1), (0, .5, 1))])
        errors = report["errors"]["nonlinear_rgb"]
        self.assertEqual(errors[0]["relative_samples_nonzero_cpu"], 0)
        self.assertIsNone(errors[0]["max_relative_to_cpu"])
        self.assertEqual(errors[1]["max_relative_to_cpu"], 0)

    def test_histogram_and_nearest_rank_percentiles(self):
        pairs = [((0, 0, 0), (delta / 255,) * 3) for delta in (0, 1, 1, 2, 10)]
        report = diagnostics.compare_pairs(pairs)
        histogram = report["code_deltas"]["rgb8"][0]
        self.assertEqual((histogram["exact"], histogram["delta_1"], histogram["over_1"]), (1, 2, 2))
        self.assertEqual((histogram["max"], histogram["p50"], histogram["p95"], histogram["p99"], histogram["p999"]), (10, 1, 10, 10, 10))
        self.assertEqual(diagnostics.compare_pairs([])["code_deltas"]["rgb8"][0]["max"], None)

    def test_invalid_values_are_counted_without_clipping(self):
        cpu, gpu = [0, .5, 1], [float("nan"), -0.1, 1.1]
        report = diagnostics.compare_pairs([(cpu, gpu)])
        self.assertEqual(report["finite_in_bounds_pairs_projected"], 0)
        self.assertEqual(report["invalid_inputs"]["gpu"], {"nonfinite_components": 1, "out_of_bounds_components": 2})
        self.assertEqual(report["errors"]["nonlinear_rgb"][1]["samples"], 1)
        self.assertTrue(math.isnan(gpu[0]))
        self.assertEqual(cpu, [0, .5, 1])
        self.assertEqual(gpu[1:], [-0.1, 1.1])
        for rgb in ((-0.1, 0, 0), (0, 0, 1.1), (0, 0, float("inf"))):
            with self.assertRaises(ValueError):
                diagnostics.quantize_rgb(rgb, 8)

    def test_binary_stream_rejects_mismatch_and_partial_triples(self):
        with tempfile.TemporaryDirectory() as directory:
            cpu, gpu = Path(directory) / "cpu", Path(directory) / "gpu"
            cpu.write_bytes(struct.pack("<3d", 0, .5, 1))
            gpu.write_bytes(struct.pack("<3f", 0, .5, 1))
            self.assertEqual(list(diagnostics.binary_pairs(cpu, gpu)), [((0, .5, 1), (0, .5, 1))])
            gpu.write_bytes(b"")
            with self.assertRaises(ValueError):
                list(diagnostics.binary_pairs(cpu, gpu))
            gpu.write_bytes(b"x")
            with self.assertRaises(ValueError):
                list(diagnostics.binary_pairs(cpu, gpu))


if __name__ == "__main__":
    unittest.main()
