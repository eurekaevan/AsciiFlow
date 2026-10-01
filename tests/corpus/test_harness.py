#!/usr/bin/env python3
"""Offline controls exercise orchestration contracts, not GPU qualification."""
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from schema import validate

spec = importlib.util.spec_from_file_location("corpus_runner", Path(__file__).with_name("run.py"))
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


class HarnessTests(unittest.TestCase):
    def setUp(self):
        self.schema = runner.load(runner.ROOT / "tests/corpus/manifest.schema.json")
        self.manifest = runner.load(runner.ROOT / "tests/corpus/representative-v1.json")

    def test_manifest_validates(self):
        validate(self.manifest, self.schema)

    def test_unknown_fields_fail(self):
        for target in ("root", "fixture", "source", "expected", "media", "request"):
            document = copy.deepcopy(self.manifest)
            value = document if target == "root" else document["fixtures"][0]
            if target not in ("root", "fixture"):
                value = value[target]
            value["typo"] = True
            with self.assertRaises(ValueError):
                validate(document, self.schema)

    def test_schema_keyword_cannot_be_silently_ignored(self):
        with self.assertRaises(ValueError):
            validate({}, {"type": "object", "made_up_constraint": True})

    def test_unexpected_pass_is_failure(self):
        expected = {"runtime": "RejectAtClassification", "failure_stage": "InputProbe", "root_cause": "UnsupportedColor"}
        self.assertEqual(runner.outcome(expected, True), "FAILED")
        self.assertEqual(runner.outcome(expected, False, "InputProbe", "UnsupportedColor"), "PASS")
        self.assertEqual(runner.outcome(expected, False, "Planning", "UnsupportedColor"), "FAILED")
        self.assertEqual(runner.outcome(expected, False, "InputProbe", "Other"), "FAILED")

    def test_unexpected_reject_is_failure(self):
        self.assertEqual(runner.outcome({"runtime": "Pass"}, False), "FAILED")

    def test_positive_fixture_requires_execution_after_inspection(self):
        fixture = copy.deepcopy(self.manifest["fixtures"][0])
        with tempfile.TemporaryDirectory() as folder:
            run = runner.Run.__new__(runner.Run)
            run.out = Path(folder)
            run.binary = Path("asciiflow")
            run.args = type("Args", (), {"device": "/dev/dri/renderD128", "generated_inputs": None})()
            run.skip_hardware = None
            run.fixture_path = lambda value: runner.ROOT / "Cargo.toml"
            commands = []

            def command(label, argv):
                commands.append(argv)
                execution = "--explain-plan" not in argv
                runner.save(Path(argv[argv.index("--diagnostic-report") + 1]), {
                    "input_requirements": {"dynamic_range": fixture["expected"]["classification"]},
                    "capabilities": {"scope": "runtime_probe_not_global_qualification"},
                    "selected_plan": {"output": {"bit_depth": fixture["request"]["bit_depth"],
                                                 "codec": fixture["request"]["codec"]}},
                    "plan_scope": "selected_by_planner",
                    "failure": {"stage": "ProcessingRuntime", "category": "Vulkan"} if execution else None,
                })
                return not execution

            run.command = command
            with patch.object(runner, "assert_plan_capabilities"), self.assertRaisesRegex(ValueError, "unexpected runtime"):
                run.fixture(fixture)
            self.assertEqual(len(commands), 2)
            self.assertIn("--explain-plan", commands[0])
            self.assertNotIn("--explain-plan", commands[1])
            self.assertNotIn("--capabilities", commands[1])

    def test_selected_plan_cannot_overclaim_probed_capabilities(self):
        diagnostic = {"input_requirements": {"codec": "H264", "bit_depth": 8},
                      "selected_plan": {"backend": "Cpu", "decode": "Software", "encode": "Software",
                                        "color_processing": "Sdr", "hardware_input_interop": False,
                                        "hardware_output_interop": False,
                                        "output": {"codec": "H264", "bit_depth": 8}},
                      "capabilities": {"processing": {"cpu": {"state": "Supported"}},
                                       "media": {"software_decode": {"state": "Supported"},
                                                 "software_encode": {"state": "Supported"}}}}
        runner.assert_plan_capabilities(diagnostic)
        diagnostic["capabilities"]["processing"]["cpu"]["state"] = "NotProbed"
        with self.assertRaises(AssertionError):
            runner.assert_plan_capabilities(diagnostic)

    def test_quick_retained_rejects_before_output_creation(self):
        with tempfile.TemporaryDirectory() as folder:
            output = Path(folder) / "new-run"
            with patch("sys.argv", ["run.py", "quick", "--retained", "--output", str(output)]):
                with self.assertRaises(SystemExit) as error:
                    runner.main()
            self.assertEqual(error.exception.code, 2)
            self.assertFalse(output.exists())

    def test_invalid_schema_stops_before_fixture_execution(self):
        with tempfile.TemporaryDirectory() as folder:
            run = runner.Run.__new__(runner.Run)
            run.results = []
            run.environment = lambda: None
            run.finish = lambda: 1
            with patch("sys.argv", ["run.py", "quick", "--output", folder]), \
                 patch.object(runner, "Run", return_value=run), \
                 patch.object(runner, "validate", side_effect=ValueError("malformed manifest")):
                self.assertEqual(runner.main(), 1)
            self.assertEqual([item["id"] for item in run.results], ["corpus-schema"])

    def test_hardware_absence_is_explicit_not_host_inference(self):
        with tempfile.TemporaryDirectory() as folder:
            result = runner.hardware_availability(str(Path(folder) / "missing-render-node"))
            self.assertIn("not visible", result)
            self.assertIn("host GPU not inferred", result)

    def test_repository_paths_cannot_escape(self):
        for path in ("/etc/passwd", "../../../etc/passwd"):
            with self.assertRaises(ValueError):
                runner.repository_path(path)

    def test_generated_cache_identity_mismatch_stops_before_command(self):
        with tempfile.TemporaryDirectory() as folder:
            cache = Path(folder) / "generated-inputs"
            cache.mkdir()
            cached = cache / "input.mp4"
            cached.write_bytes(b"wrong")
            run = runner.Run.__new__(runner.Run)
            run.args = type("Args", (), {"generated_inputs": cache})()
            run.fixture_setups = []
            fixture = {"id": "cache-identity", "source": {
                "kind": "generated", "path": "generated/input.mp4",
                "byte_size": 3, "sha256": "0" * 64,
            }}
            run.validate_source_recipe = lambda value: None
            run.command = lambda *args, **kwargs: self.fail("must not invoke generator")
            with self.assertRaisesRegex(ValueError, "cached fixture setup identity mismatch"):
                run.fixture_path(fixture)
            self.assertEqual(run.fixture_setups, [])

    def test_failed_hardware_input_setup_skips_dependent_smoke_and_retained(self):
        with tempfile.TemporaryDirectory() as folder:
            output = Path(folder)

            class FakeRun:
                def __init__(self):
                    self.args = type("Args", (), {"device": "/dev/dri/renderD128"})()
                    self.out = output
                    self.results = []
                    self.skip_hardware = None
                    self.executed = []

                def environment(self):
                    pass

                def fixture_path(self, fixture):
                    raise OSError("cached SDR input missing")

                def fixture(self, fixture):
                    return {"selected_plan": {}}

                def gate(self, name, category, action, hardware=False):
                    self.executed.append(name)
                    try:
                        evidence = action()
                    except OSError as error:
                        self.results.append({"id": name, "category": category,
                                             "result": "FAILED", "reason": str(error)})
                        return False
                    self.results.append({"id": name, "category": category,
                                         "result": "PASS", "evidence": evidence})
                    return True

                def checked(self, name, argv, env=None):
                    return {"command": name}

                def smoke(self, *args):
                    self.executed.append(args[0])
                    return {}

                def retained(self, *args):
                    self.executed.append("all-retained-production")
                    return {}

                def finish(self):
                    return 1

            fake = FakeRun()
            argv = ["run.py", "hardware", "--retained", "--output", folder]
            with patch("sys.argv", argv), patch.object(runner, "Run", return_value=fake):
                self.assertEqual(runner.main(), 1)
            results = {result["id"]: result for result in fake.results}
            self.assertEqual(results["hardware-sdr-input-identity"]["result"], "FAILED")
            self.assertEqual(results["smoke-sdr"]["result"], "SKIPPED")
            self.assertIn("blocked by failed hardware-sdr-input-identity", results["smoke-sdr"]["reason"])
            self.assertEqual(results["all-retained-production"]["result"], "SKIPPED")
            self.assertIn("blocked by failed hardware-sdr-input-identity",
                          results["all-retained-production"]["reason"])
            self.assertNotIn("smoke-sdr", fake.executed)
            self.assertNotIn("all-retained-production", fake.executed)

    def test_append_only_report(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "report.json"
            runner.save(path, {"original": True})
            with self.assertRaises(FileExistsError):
                runner.save(path, {"replacement": True})
            self.assertEqual(json.loads(path.read_text()), {"original": True})

    def test_result_ids_and_expectations_are_complete(self):
        ids = [fixture["id"] for fixture in self.manifest["fixtures"]]
        self.assertEqual(len(ids), len(set(ids)))
        for fixture in self.manifest["fixtures"]:
            expected = fixture["expected"]
            if expected["runtime"] != "Pass":
                self.assertIsNotNone(expected["failure_stage"])
                self.assertIsNotNone(expected["root_cause"])


if __name__ == "__main__":
    unittest.main()
