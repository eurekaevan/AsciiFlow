"""A skipped hardware campaign is not a passing preflight."""
import unittest
from unittest.mock import patch

import preflight


class PreflightOutcome(unittest.TestCase):
    def test_exit_status_preserves_failed_or_skipped_gates(self):
        for result, runner_status, expected in (("PASS", 0, 0), ("SKIPPED", 0, 2), ("FAILED", 1, 1)):
            with self.subTest(result=result), patch("preflight.Run") as runner, patch(
                "sys.argv", ["preflight.py", "--output", "unused-mocked-output"]
            ):
                run = runner.return_value
                run.results = [{"result": result} for _ in range(3)]
                run.finish.return_value = runner_status
                self.assertEqual(preflight.main(), expected)
                self.assertEqual(run.gate.call_count, 3)


if __name__ == "__main__":
    unittest.main()
