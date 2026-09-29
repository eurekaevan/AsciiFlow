#!/usr/bin/env python3
"""Independent contract checks for audit-only domain and formula tools."""
import importlib.util
import math
from pathlib import Path
import sys
import unittest
from decimal import Decimal as D

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("audit", Path(__file__).with_name("audit-c2a.py"))
audit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(audit)


class FeasibilityTests(unittest.TestCase):
    def test_white_interval_for_multiple_primaries(self):
        for primaries in audit.PRIMARIES.values():
            for y, expected in ((-1, False), (0, True), (50, True), (100, True),
                                (math.nextafter(100, math.inf), False), (103.64, False)):
                self.assertEqual(audit.feasibility(primaries, 100, ((0, 1),) * 3, y)["feasible"], expected)

    def test_asymmetric_box(self):
        ranges = ((.1, .8), (.2, .9), (.3, 1))
        weights = audit.luminance_weights(audit.PRIMARIES["bt709"])
        low = 100 * sum(w * p[0] for w, p in zip(weights, ranges))
        high = 100 * sum(w * p[1] for w, p in zip(weights, ranges))
        for y, expected in ((low - .01, False), (low, True), ((low + high) / 2, True),
                            (high, True), (high + .01, False)):
            self.assertEqual(audit.feasibility(audit.PRIMARIES["bt709"], 100, ranges, y)["feasible"], expected)

    def test_custom_primaries_white_rounding_both_directions(self):
        # Valid custom gamuts whose ordinary normalized float sums lie below
        # and above one: neither may shift nominal white or accept above peak.
        gamuts = (
            ((.6610953156605954, .33127496207697166),
             (.3166299057781137, .578882741497408),
             (.17832244198198655, .0639935239531424)),
            ((.6009592273218368, .3010521562987478),
             (.32267301174289115, .5702939064509546),
             (.12302520739609066, .04468648539175216)),
        )
        for primaries in gamuts:
            for y, expected in ((100, True), (math.nextafter(100, math.inf), False)):
                result = audit.feasibility(primaries, 100, ((0, 1),) * 3, y)
                self.assertEqual(result["maximum_y_nits"], 100)
                self.assertEqual(result["feasible"], expected)

    def test_invalid_geometry_and_nonfinite(self):
        for primaries in (((.3, .3),) * 3, ((.3, 0),) * 3):
            with self.assertRaises(ValueError):
                audit.luminance_weights(primaries)
        for peak, y in ((0, 50), (float("inf"), 50), (100, float("nan"))):
            with self.assertRaises(ValueError):
                audit.feasibility(audit.PRIMARIES["bt709"], peak, ((0, 1),) * 3, y)
        with self.assertRaises(ValueError):
            audit.feasibility(audit.PRIMARIES["bt709"], 100, ((1, 0),) * 3, 50)
        with self.assertRaises(ValueError):
            audit.feasibility(audit.PRIMARIES["bt709"], 100, ((0, 1e308),) * 3, 50)

    def test_known_primary_derived_bt709_weights(self):
        weights = audit.luminance_weights(audit.PRIMARIES["bt709"])
        for actual, expected in zip(weights, (.212639005871510, .715168678767756, .072192315360734)):
            self.assertAlmostEqual(actual, expected, places=14)


class FormulaTests(unittest.TestCase):
    def test_printed_contradiction_not_weakened(self):
        self.assertAlmostEqual(float(audit.printed_rolloff(D("1.5"), D(".5"), D(".2"))), 19 / 6)
        self.assertNotEqual(audit.printed_rolloff(D("1.5"), D(".5"), D(".2")), 1)
        with self.assertRaises(ArithmeticError):
            audit.printed_rolloff(D("1.2"), D(".2"), D(".2"))

    def test_derived_candidate_endpoints_including_equal_parameters(self):
        for alpha in (D(".1"), D(".2"), D(".5")):
            self.assertEqual(audit.bezier_diagnostic(D(0), alpha, D(".2")), (D(".8"), D(".8")))
            self.assertEqual(audit.bezier_diagnostic(D(1), alpha, D(".2")), (1 + alpha, D(1)))


class TaxonomyTests(unittest.TestCase):
    def test_classes_and_negative_unknown(self):
        cases = (((.5, .5, .5), "inside_source_effective_gamut"),
                 ((0, 0, 1.1), "class_a_outside_source_y_feasible"),
                 ((1.1, 1.1, 1.1), "class_b_y_above_peak"),
                 ((-.1, 0, 0), "negative_y_unclassified"))
        for rgb, expected in cases:
            self.assertEqual(audit.classify(rgb)[0], expected)
        self.assertEqual(audit.classify((-.1, 0, 0), signed=True)[0], "y_below_zero")


if __name__ == "__main__":
    unittest.main()
