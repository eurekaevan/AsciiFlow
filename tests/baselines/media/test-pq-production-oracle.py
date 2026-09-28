#!/usr/bin/env python3
"""Negative controls for the retained PQ output metadata gate."""
import importlib.util
from pathlib import Path
import unittest
import sys
import json
from types import SimpleNamespace
from unittest.mock import patch

sys.dont_write_bytecode = True

spec = importlib.util.spec_from_file_location("oracle", Path(__file__).with_name("verify-pq-production.py"))
oracle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(oracle)


class OutputSignalTests(unittest.TestCase):
    def canonical(self):
        return {"color_primaries": "bt2020", "color_transfer": "smpte2084",
                "color_space": "bt2020nc", "color_range": "tv"}

    def test_valid_signal(self):
        oracle.check_color(self.canonical())

    def test_wrong_primaries_missing_transfer_full_range_fail(self):
        for field, value in [("color_primaries", "bt709"), ("color_transfer", None), ("color_range", "pc")]:
            with self.subTest(field=field):
                signal = self.canonical()
                if value is None:
                    del signal[field]
                else:
                    signal[field] = value
                with self.assertRaises(AssertionError):
                    oracle.check_color(signal)

    def test_source_static_metadata_leak_fails(self):
        for kind in ["Mastering display metadata", "Content light level metadata"]:
            with self.subTest(kind=kind):
                signal = self.canonical()
                signal["side_data_list"] = [{"side_data_type": kind}]
                with self.assertRaises(AssertionError):
                    oracle.check_color(signal)


class InputIdentityTests(unittest.TestCase):
    def test_identity_json_cannot_attest_changed_input_bytes(self):
        identity = {"encoding": {"outputs": [
            {"file": "input.mp4", "bytes": 123, "sha256": "canonical"},
        ]}}
        with patch.object(oracle.Path, "read_text", return_value=json.dumps(identity)), \
             patch.object(oracle.Path, "stat", return_value=SimpleNamespace(st_size=123)), \
             patch.object(oracle, "file_hash", return_value="changed"):
            with self.assertRaisesRegex(AssertionError, "input SHA-256 changed"):
                oracle.canonical_identity(Path("fixtures"))


if __name__ == "__main__":
    unittest.main()
