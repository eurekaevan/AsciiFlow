"""Tests for campaign selection and evidence guards; no media/hardware work."""
import importlib.util
from pathlib import Path
import sys
import unittest

SCRIPT = Path(__file__).with_name("failure-campaign.py")
SPEC = importlib.util.spec_from_file_location("failure_campaign", SCRIPT)
CAMPAIGN = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = CAMPAIGN
SPEC.loader.exec_module(CAMPAIGN)


class FailureCampaign(unittest.TestCase):
    def test_zero_matched_tests_is_not_success(self):
        with self.assertRaises(AssertionError):
            CAMPAIGN.assert_one_test("running 0 tests\ntest result: ok. 0 passed; 0 failed; 0 ignored;")

    def test_one_test_must_pass_and_not_be_ignored(self):
        CAMPAIGN.assert_one_test("running 1 test\ntest result: ok. 1 passed; 0 failed; 0 ignored;")
        for summary in ("0 passed; 0 failed; 1 ignored;", "0 passed; 1 failed; 0 ignored;"):
            with self.assertRaises(AssertionError):
                CAMPAIGN.assert_one_test("running 1 test\ntest result: ok. " + summary)

    def test_all_commands_have_exact_serial_release_selection(self):
        self.assertEqual(len({gate.name for gate in CAMPAIGN.GATES}), len(CAMPAIGN.GATES))
        for gate in CAMPAIGN.GATES:
            argv = CAMPAIGN.command(gate)
            self.assertIn("--release", argv)
            self.assertIn("--exact", argv)
            self.assertIn("--test-threads=1", argv)
            self.assertIn(gate.selector, argv)
            self.assertEqual("--ignored" in argv, gate.ignored)

    def test_skipped_hardware_never_exits_success(self):
        self.assertEqual(CAMPAIGN.completion_status(0, [{"result": "SKIPPED"}]), 2)
        self.assertEqual(CAMPAIGN.completion_status(1, [{"result": "FAILED"}]), 1)
        self.assertEqual(CAMPAIGN.completion_status(0, [{"result": "PASS"}]), 0)


if __name__ == "__main__":
    unittest.main()
