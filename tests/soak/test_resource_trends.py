"""Synthetic tests for the bounded, dependency-free resource trend analyzer."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest


SCRIPT = Path(__file__).with_name("analyze-resources.py")
SPEC = importlib.util.spec_from_file_location("analyze_resources", SCRIPT)
ANALYZER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(ANALYZER)


def samples(values, **extra):
    result = []
    for index, value in enumerate(values):
        sample = {
            "phase": "steady",
            "frames_processed": index * 100,
            "rss_kib": value,
            "fd_count": 12,
            "resources": {"frame_pool": {"active_count": 2, "active_bytes": 4096,
                                            "peak_count": 2, "peak_bytes": 4096}},
            "queues": {"decode": {"depth": 1, "capacity": 3, "peak_depth": 2}},
            "accounting_errors": [],
        }
        sample.update(extra)
        result.append(sample)
    return result


class ResourceTrends(unittest.TestCase):
    def test_plateau_is_stable_but_not_long_soak_qualification(self):
        report = ANALYZER.analyze(samples([100, 110, 120, 125, 125, 126, 125, 125, 126, 125, 125, 126]))
        self.assertEqual(report["classification"], "WarmupThenStable")
        self.assertEqual(report["qualification"], "ShortStabilityOnly")
        self.assertTrue(report["resource_growth_is_not_automatic_leak"])
        self.assertEqual(report["metrics"]["rss_kib"]["initial"], 100)
        self.assertEqual(report["metrics"]["rss_kib"]["warmup_peak"], 125)
        self.assertEqual(report["metrics"]["rss_kib"]["whole_run_peak"], 126)
        self.assertEqual(len(report["metrics"]["rss_kib"]["early_middle_late_slopes_per_frame"]), 3)

    def test_persistent_rss_and_resource_growth_is_suspected_not_proven_leak(self):
        report = ANALYZER.analyze(samples(list(range(100, 220, 10))))
        self.assertEqual(report["classification"], "LeakSuspected")
        self.assertIn("rss_kib", [item["metric"] for item in report["growth_evidence"]])
        self.assertEqual(report["qualification"], "Unresolved")

    def test_explicit_cache_attribution_is_not_guessed_or_flagged_as_leak(self):
        rows = samples(list(range(100, 220, 10)))
        for row in rows:
            row["expected_cache_growth"] = {"rss_kib": "bounded codec cache"}
        report = ANALYZER.analyze(rows)
        self.assertNotEqual(report["classification"], "LeakSuspected")
        self.assertEqual(report["metrics"]["rss_kib"]["expected_cache_attribution"], "bounded codec cache")

    def test_few_progress_points_remain_unresolved(self):
        report = ANALYZER.analyze(samples([100] * 8))
        self.assertEqual(report["classification"], "Unresolved")
        self.assertEqual(report["unique_progress_points"], 8)

    def test_missing_and_null_fields_are_unavailable_not_zero(self):
        rows = samples([100] * 10)
        for row in rows:
            row["fd_count"] = None
            row["rss_kib"] = None
            row.pop("resources")
            row.pop("queues")
        report = ANALYZER.analyze(rows)
        self.assertEqual(report["classification"], "Unresolved")
        self.assertFalse(report["metrics"]["rss_kib"]["available"])
        self.assertFalse(report["metrics"]["fd_count"]["available"])
        self.assertNotIn("resources.frame_pool.active_bytes", report["metrics"])

    def test_accounting_and_counter_invariant_failures_block_classification(self):
        rows = samples([100] * 10)
        rows[5]["accounting_errors"] = ["released twice"]
        rows[6]["queues"]["decode"]["depth"] = 4
        report = ANALYZER.analyze(rows)
        self.assertEqual(report["classification"], "Unresolved")
        self.assertTrue(any("accounting error" in error for error in report["accounting_and_invariant_errors"]))
        self.assertTrue(any("exceeds capacity" in error for error in report["accounting_and_invariant_errors"]))

    def test_duplicate_progress_samples_collapse_and_unknown_fields_are_ignored(self):
        rows = samples([100] * 10)
        rows.append(dict(rows[-1], rss_kib=999, future_field="not interpreted"))
        report = ANALYZER.analyze(rows)
        self.assertEqual(report["unique_progress_points"], 10)
        self.assertEqual(report["metrics"]["rss_kib"]["final"], 549.5)
        self.assertIn("ignored", report["unknown_fields"])

    def test_boundary_phases_do_not_merge_into_runtime_trend(self):
        rows = samples([100] * 9)
        for index, row in enumerate(rows):
            row["phase"] = "progress"
            row["frames_processed"] = index * 1000
        rows.insert(0, {**rows[0], "phase": "initial", "rss_kib": 50})
        rows.insert(1, {**rows[1], "phase": "post-init", "rss_kib": 60})
        rows.append({**rows[-1], "phase": "pre-finalization", "rss_kib": 90})
        rows.append({**rows[-1], "phase": "post-cleanup", "rss_kib": 0})
        report = ANALYZER.analyze(rows)
        metric = report["metrics"]["rss_kib"]
        self.assertEqual(report["classification"], "Stable")
        self.assertEqual(report["unique_progress_points"], 9)
        self.assertEqual(metric["initial"], 50)
        self.assertEqual(metric["final"], 0)
        self.assertEqual(metric["steady_median"], 100)
        self.assertEqual(metric["warmup_peak"], 100)
        self.assertEqual(metric["whole_run_peak"], 100)

    def test_known_metric_with_incomplete_runtime_windows_is_unresolved(self):
        rows = samples([100] * 10)
        for row in rows[3:]:
            row["rss_kib"] = None
        report = ANALYZER.analyze(rows)
        self.assertEqual(report["classification"], "Unresolved")
        self.assertFalse(report["metrics"]["rss_kib"]["trend_data_sufficient"])

    def test_cli_creates_new_report_and_preserves_existing_output(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            input_path = root / "samples.jsonl"
            output_path = root / "report.json"
            input_path.write_text("".join(json.dumps(row) + "\n" for row in samples([100] * 10)))
            self.assertEqual(ANALYZER.main(["--input", str(input_path), "--output", str(output_path)]), 0)
            output_path.write_text("keep")
            with self.assertRaises(SystemExit):
                ANALYZER.main(["--input", str(input_path), "--output", str(output_path)])
            self.assertEqual(output_path.read_text(), "keep")


if __name__ == "__main__":
    unittest.main()
