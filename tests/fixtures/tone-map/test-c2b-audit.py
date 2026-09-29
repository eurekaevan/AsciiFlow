#!/usr/bin/env python3
"""Contract tests for the independent read-only C-2B audit."""
import importlib.util
import io
import json
import math
from pathlib import Path
import struct
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("audit_c2b", Path(__file__).with_name("audit-c2b-target-volume.py"))
audit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(audit)


class SerializationTests(unittest.TestCase):
    def test_header_and_finite_pixel(self):
        payload = audit.C2B_MAGIC + struct.pack("<II", 1, 1) + audit.C2B_PIXEL.pack(*([.5] * 12))
        stream = io.BytesIO(payload)
        self.assertEqual(audit.read_header(stream, audit.C2B_MAGIC, audit.C2B_PIXEL), (1, 1))
        self.assertEqual(list(audit.pixels(stream, audit.C2B_PIXEL)), [(.5,) * 12])

    def test_rejects_magic_dimensions_size_and_trailing_data(self):
        valid = audit.C1_MAGIC + struct.pack("<II", 1, 1) + audit.C1_PIXEL.pack(*([0] * 7))
        invalid = [b"wrong", audit.C1_MAGIC + b"\0", valid[:-1], valid + b"x",
                   audit.C1_MAGIC + struct.pack("<II", 0, 1)]
        for payload in invalid:
            with self.subTest(payload_length=len(payload)), self.assertRaises(ValueError):
                audit.read_header(io.BytesIO(payload), audit.C1_MAGIC, audit.C1_PIXEL)

    def test_rejects_nonfinite_in_any_serialized_field(self):
        for pixel in (audit.C1_PIXEL, audit.C2B_PIXEL):
            components = pixel.size // 8
            for index in range(components):
                for bad in (math.nan, math.inf, -math.inf):
                    values = [0.] * components
                    values[index] = bad
                    with self.assertRaises(ValueError):
                        list(audit.pixels(io.BytesIO(pixel.pack(*values)), pixel))
        with self.assertRaises(ValueError):
            list(audit.pixels(io.BytesIO(b"x"), audit.C1_PIXEL))

    def test_complete_read_only_audit_and_corrupt_clip_rejection(self):
        with tempfile.TemporaryDirectory() as directory:
            fixture = Path(directory)
            # A small sealed synthetic fixture exercises the integration flow.
            for name in ("c2b-vectors.json", "generate-c2b-vectors.py"):
                (fixture / name).write_bytes((audit.FIXTURE / name).read_bytes())
            c1 = fixture / "c1.bin"
            c1.write_bytes(audit.C1_MAGIC + struct.pack("<II", 2, 1)
                           + audit.C1_PIXEL.pack(*([0.] * 7))
                           + audit.C1_PIXEL.pack(.5, .5, .5, 0., 0., 0., 0.))
            digest = audit.sha256(c1)
            (fixture / "identity.json").write_text(json.dumps(dict(
                input=dict(width=2, height=1), output=dict(sha256_runs_1_2_3=[digest] * 3))))
            linear = .5 ** 2.4
            valid = (audit.C2B_MAGIC + struct.pack("<II", 2, 1)
                     + audit.C2B_PIXEL.pack(*([0.] * 12))
                     + audit.C2B_PIXEL.pack(*([linear] * 9 + [.5] * 3)))
            for run in (1, 2, 3):
                (fixture / f"target-volume-run{run}.bin").write_bytes(valid)
            with patch.object(audit, "FIXTURE", fixture):
                result = audit.audit(c1, fixture)
                self.assertEqual(result["samples"], 2)
                self.assertEqual(result["numerical_gate_status"], "PASS")
                self.assertEqual(result["counts"]["unchanged_samples"], 2)
                self.assertEqual(result["delta_y_cd_m2"]["zero"], 2)
                self.assertIsNone(result["first_observed_category_representatives"]["source_both"])
                self.assertEqual(audit.sha256(c1), digest)
                self.assertEqual((fixture / "target-volume-run1.bin").read_bytes(), valid)
                # Same corrupted bytes in all runs pass repeatability but must
                # fail the numerical contract; hashes alone are insufficient.
                invalid = (audit.C2B_MAGIC + struct.pack("<II", 2, 1)
                           + audit.C2B_PIXEL.pack(*([0.] * 12))
                           + audit.C2B_PIXEL.pack(*([linear] * 6 + [.25] * 3 + [.5] * 3)))
                for run in (1, 2, 3):
                    (fixture / f"target-volume-run{run}.bin").write_bytes(invalid)
                with self.assertRaisesRegex(ValueError, "clamp"):
                    audit.audit(c1, fixture)


class DiagnosticTests(unittest.TestCase):
    def test_independent_decimal_matrix_vectors(self):
        document = json.loads((audit.FIXTURE / "c2b-vectors.json").read_text())
        matrices = {name: tuple(tuple(map(audit.Decimal, row)) for row in matrix)
                    for name, matrix in document["matrices"].items()}
        result = audit.verify_decimal_vectors(document, matrices)
        self.assertEqual(result["vectors_checked"], len(document["vectors"]))
        self.assertLessEqual(audit.Decimal(result["maximum_absolute_error"]), audit.Decimal("1e-65"))
        oracle = audit.separate_xyz_oracle(matrices)
        xyz, target = oracle(audit.bits((1., 1., 1.)))
        for component in target:
            self.assertAlmostEqual(component, 1., places=14)
        self.assertAlmostEqual(xyz[1], 1., places=14)

    def test_uv_black_and_neutral_guards(self):
        self.assertIsNone(audit.uv((0, 0, 0)))
        self.assertIsNone(audit.uv((3, 0, -1)))
        white = audit.uv((.3127 / .3290, 1, (1 - .3127 - .3290) / .3290))
        self.assertEqual(audit.hue_chroma(None, white), (None, None))
        self.assertEqual(audit.hue_chroma(white, white), (None, 0))
        hue, radius = audit.hue_chroma((white[0] + .1, white[1]), white)
        self.assertEqual(hue, 0)
        self.assertAlmostEqual(radius, .1)

    def test_quantiles_and_signed_counts(self):
        result = audit.signed_statistics([-2, 0, 1, 3])
        self.assertEqual((result["negative"], result["zero"], result["positive"]), (1, 1, 2))
        self.assertEqual(result["signed"]["mean"], .5)
        self.assertEqual(result["signed"]["p50"], .5)
        self.assertAlmostEqual(result["signed"]["p95"], 2.7)
        self.assertEqual(result["absolute"]["mean"], 1.5)
        self.assertEqual(result["signed"]["max_abs"], 3)
        self.assertIsNone(audit.statistics([])["mean"])
        self.assertEqual(audit.statistics([4])["p99"], 4)

    def test_clip_retains_interior_bits_and_is_idempotent(self):
        for rgb in ((-0., .25, 1.), (-.1, .5, 1.1), (0., 0., 0.)):
            bounded = audit.clip(rgb)
            self.assertEqual(audit.bits(audit.clip(bounded)), audit.bits(bounded))
            for before, after in zip(rgb, bounded):
                if 0 <= before <= 1:
                    self.assertEqual(struct.pack("<d", before), struct.pack("<d", after))
        self.assertEqual(audit.clip((-.1, .5, 1.1)), (0, .5, 1))

    def test_signed_power_has_no_source_preclip(self):
        result = audit.signed_power((-2, -0., 2))
        self.assertLess(result[0], -1)
        self.assertGreater(result[2], 1)
        self.assertEqual(struct.pack("<d", result[1]), struct.pack("<d", 0.))

    def test_collision_sampling_deduplicates_source_before_grouping(self):
        sample = audit.CollisionSample()
        sample.add((-1, .5, 0), (0, .5, 0), 0)
        sample.add((-1, .5, 0), (0, .5, 0), 257)
        self.assertEqual(sample.result()["many_to_one_groups"], 0)
        sample.add((-2, .5, 0), (0, .5, 0), 514)
        result = sample.result()
        self.assertEqual(result["sampled_pixels"], 3)
        self.assertEqual(result["distinct_sources"], 2)
        self.assertEqual(result["bounded_groups"], 1)
        self.assertEqual(result["many_to_one_groups"], 1)
        self.assertEqual(result["collision_excess_sources"], 1)
        self.assertEqual(result["maximum_multiplicity"], 2)
        self.assertEqual(len(result["example"]["sources"]), 2)


if __name__ == "__main__":
    unittest.main()
