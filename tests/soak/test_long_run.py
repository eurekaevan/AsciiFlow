"""Unit tests for the D-1B long-run harness boundaries."""
import contextlib
import importlib.util
import io
import os
from pathlib import Path
import sys
import unittest
from unittest.mock import Mock, patch


SCRIPT = Path(__file__).with_name("long-run.py")
SPEC = importlib.util.spec_from_file_location("long_run", SCRIPT)
long_run = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(long_run)
from preflight import verify_source_path

class LongPathIdentity(unittest.TestCase):
    def test_rejects_sdr_lookalike_for_pq_to_sdr_path(self):
        diagnostic = {
            "input_requirements": {"codec": "Av1", "bit_depth": 10, "dynamic_range": "HdrPq",
                                   "width": 1920, "height": 1080,
                                   "frame_rate": {"numerator": 50, "denominator": 1}},
            "selected_plan": {"color_processing": "HdrPqToSdrBt709"},
        }
        verify_source_path("pq-to-sdr", {"kind": "pq-to-sdr"}, diagnostic)
        with self.assertRaises(AssertionError):
            verify_source_path("pq-to-sdr", {"kind": "sdr"}, diagnostic)
        diagnostic["input_requirements"]["dynamic_range"] = "Sdr"
        with self.assertRaises(AssertionError):
            verify_source_path("pq-to-sdr", {"kind": "pq-to-sdr"}, diagnostic)
        diagnostic["input_requirements"]["dynamic_range"] = "HdrPq"
        diagnostic["selected_plan"]["color_processing"] = "Sdr"
        with self.assertRaises(AssertionError):
            verify_source_path("pq-to-sdr", {"kind": "pq-to-sdr"}, diagnostic)

class LongRunHarness(unittest.TestCase):
    def invoke(self, *args, environ=None):
        argv = ["long-run.py", "--output", "unused", "--binary", "unused-binary", *args]
        output = io.StringIO()
        with patch.object(sys, "argv", argv), contextlib.redirect_stderr(output):
            if environ is None:
                status = long_run.main()
            else:
                with patch.dict(os.environ, environ, clear=True):
                    status = long_run.main()
        return status, output.getvalue()

    def configured_run(self, results, finish_status=0):
        run = Mock()
        run.out = Path("unused")
        run.results = results
        run.finish.return_value = finish_status

        def gate(name, category, action, required):
            action()
            run.results.append({"name": name, "result": "PASS"})

        run.gate.side_effect = gate
        return run

    def test_failed_campaign_status_is_preserved(self):
        run = self.configured_run([], finish_status=7)
        with patch.object(long_run, "Run", return_value=run), patch.object(long_run, "save"), \
                patch.object(long_run, "qualify"):
            status, _ = self.invoke()
        self.assertEqual(status, 7)
        self.assertEqual(run.gate.call_count, 3)

    def test_skipped_path_does_not_return_success(self):
        run = self.configured_run([{"name": "sdr", "result": "SKIPPED"}])

        def gate(name, category, action, required):
            if name == "sdr":
                run.results[0]["name"] = name
            else:
                action()
                run.results.append({"name": name, "result": "PASS"})

        run.gate.side_effect = gate
        with patch.object(long_run, "Run", return_value=run), patch.object(long_run, "save"), \
                patch.object(long_run, "qualify"):
            status, _ = self.invoke()
        self.assertEqual(status, 2)
        self.assertEqual(run.gate.call_count, 3)

    def test_runs_all_three_required_paths_without_validation(self):
        run = self.configured_run([])
        qualify = Mock()
        with patch.object(long_run, "Run", return_value=run), \
                patch.object(long_run, "qualify", qualify), patch.object(long_run, "save"):
            status, _ = self.invoke()

        self.assertEqual(status, 0)
        self.assertEqual(
            [entry.args[:2] for entry in run.gate.call_args_list],
            [("sdr", "hardware-long-run"),
             ("pq-preserve", "hardware-long-run"),
             ("pq-to-sdr", "hardware-long-run")],
        )
        self.assertEqual(qualify.call_count, 3)
        for entry, kind in zip(qualify.call_args_list, ("sdr", "pq-preserve", "pq-to-sdr")):
            self.assertEqual(entry.args[1:4], (kind, 100000, False))
            self.assertEqual(entry.kwargs, {
                "validation": False,
                "qualification": "LongRunPathEvidenceNotAutomaticSeal",
                "audio_tracks": {"sdr": 1, "pq-preserve": 0, "pq-to-sdr": 2}[kind],
            })

    def test_invalid_watchdog_values_are_rejected(self):
        for value in ("0", "-1", "nan", "inf", "-inf"):
            with self.subTest(value=value):
                with patch.object(long_run, "Run") as runner:
                    with self.assertRaises(SystemExit) as raised:
                        self.invoke("--watchdog-seconds", value)
                self.assertEqual(raised.exception.code, 2)
                runner.assert_not_called()

    def test_allocator_or_loader_environment_is_rejected(self):
        for key in ("MALLOC_ARENA_MAX", "GLIBC_TUNABLES", "LD_PRELOAD"):
            with self.subTest(key=key):
                with patch.object(long_run, "Run") as runner:
                    with self.assertRaises(SystemExit) as raised:
                        self.invoke(environ={key: "set"})
                self.assertEqual(raised.exception.code, 2)
                runner.assert_not_called()

    def test_preflight_limit_remains_10000_frames(self):
        import preflight

        with patch.object(sys, "argv", ["preflight.py", "--output", "unused", "--frames", "10001"]), \
                patch.object(preflight, "Run") as runner, \
                contextlib.redirect_stderr(io.StringIO()):
            with self.assertRaises(SystemExit) as raised:
                preflight.main()
        self.assertEqual(raised.exception.code, 2)
        runner.assert_not_called()


if __name__ == "__main__":
    unittest.main()
