"""Bounded offline contract tests for RealMediaMixin, not GPU qualification."""

from __future__ import annotations

import copy
from contextlib import redirect_stdout
from fractions import Fraction
import hashlib
import io
import json
from pathlib import Path
import tempfile
import unittest
from types import SimpleNamespace
from unittest.mock import patch

from real_media import RealMediaMixin, characterize_audio_limits, normalized_probe, probe_digest, _manifest, audio_titles
from run import Run


class TestableRun(RealMediaMixin, Run):
    """Uses the production mixin and Run helpers with only command execution mocked."""


PROBE = {
    "format": {"filename": "/tmp/source/input.mp4", "format_name": "mov,mp4"},
    "streams": [{"index": 0, "codec_type": "video", "codec_name": "h264",
                 "time_base": "1/25", "avg_frame_rate": "25/1", "width": 128, "height": 96}],
    "frames": [{"stream_index": 0, "media_type": "video", "pts": 0, "duration": 1}],
    "packets": [{"stream_index": 0, "pts": 0, "dts": 0, "duration": 1}],
}


def supported_plan():
    return {
        "input_requirements": {"dynamic_range": "Sdr", "frame_rate": "25/1"},
        "selected_plan": {"backend": "Cpu", "decode": "Software", "encode": "Software",
                          "color_processing": "Sdr", "hardware_input_interop": False,
                          "hardware_output_interop": False,
                          "output": {"codec": "H264", "bit_depth": 8}},
        "plan_scope": "selected_by_planner",
        "capabilities": {"processing": {"cpu": {"state": "Supported"}},
                         "media": {"software_decode": {"state": "Supported"},
                                   "software_encode": {"state": "Supported"}}},
        "failure": None,
    }


def source_fixture(cache: Path, *, expected_probe: str | None = None,
                   runtime: str = "Pass", failure_stage: str | None = None,
                   root_cause: str | None = None) -> dict:
    media_path = cache / "input.mp4"
    data = b"identity checked mock input"
    media_path.write_bytes(data)
    source = {"kind": "generated", "path": "generated/input.mp4", "byte_size": len(data),
              "sha256": hashlib.sha256(data).hexdigest(), "generation_group": "mock-video",
              "exact_command": ["python3", "mock-generator.py", "{output_directory}"]}
    if expected_probe is not None:
        source["probe_sha256"] = expected_probe
    return {
        "id": "mock-fixture", "source": source,
        "request": {"dynamic_range": "sdr", "codec": "H264", "bit_depth": 8,
                    "route": "portable", "audio": "none"},
        "media": _manifest.describe(PROBE, "input.mp4")[0],
        "expected": {"classification": "Sdr", "status": "Unqualified", "runtime": runtime,
                     "failure_stage": failure_stage, "root_cause": root_cause},
    }


def make_run(folder: str, cache: Path) -> TestableRun:
    run = TestableRun.__new__(TestableRun)
    run.args = SimpleNamespace(device="/mock/renderD128", generated_inputs=cache)
    run.out = Path(folder)
    run.commands = []
    run.fixture_setups = []
    run.real_generation_groups = {}
    run.results = []
    run.binary = Path("/mock/asciiflow")
    run.skip_hardware = None
    return run


class RealMediaTests(unittest.TestCase):
    def test_audio_title_comparison_preserves_semantics_not_container_key(self):
        def probe(tags):
            return {"streams": [{"codec_type": "audio", "tags": tags}]}
        self.assertEqual(audio_titles(probe({"name": "English tone"})),
                         audio_titles(probe({"TITLE": "English tone"})))
        self.assertNotEqual(audio_titles(probe({"name": "English tone"})), audio_titles(probe({})))

    def test_normalized_probe_changes_only_filename_and_digest_is_path_independent(self) -> None:
        original = copy.deepcopy(PROBE)
        relocated = copy.deepcopy(PROBE)
        relocated["format"]["filename"] = "/another/root/input.mp4"
        self.assertEqual(normalized_probe(original)["format"]["filename"], "input.mp4")
        self.assertEqual(probe_digest(original), probe_digest(relocated))
        self.assertEqual(PROBE, original)
        self.assertEqual(original["format"]["format_name"], normalized_probe(original)["format"]["format_name"])

    def test_generation_group_is_invoked_once_and_reuses_recipe_identity(self) -> None:
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            out, cache = root / "run", root / "cache"
            out.mkdir()
            cache.mkdir()
            run = make_run(str(out), cache)
            contents = {"input-a.mp4": b"A", "input-b.mp4": b"B"}
            source_hashes = {name: hashlib.sha256(data).hexdigest() for name, data in contents.items()}

            def mock_command(label, argv, env=None, timeout=None, stderr_log=None):
                self.assertTrue(label.startswith("generate-group-"))
                directory = Path(argv[-1])
                directory.mkdir(parents=True, exist_ok=False)
                for name, data in contents.items():
                    (directory / name).write_bytes(data)
                run.commands.append({"id": label, "argv": argv, "exit_code": 0,
                                     "log": "mock-command.log", "mocked": True})
                return True

            run.command = mock_command
            recipe = ["python3", "mock-generator.py", "{output_directory}"]
            fixtures = []
            for name in contents:
                fixtures.append({"id": name, "source": {
                    "kind": "generated", "path": f"generated/{name}",
                    "byte_size": len(contents[name]), "sha256": source_hashes[name],
                    "generation_group": "shared-video", "exact_command": recipe}})
            first = run.fixture_path(fixtures[0])
            second = run.fixture_path(fixtures[1])
            self.assertEqual(first.read_bytes(), b"A")
            self.assertEqual(second.read_bytes(), b"B")
            self.assertEqual([command["id"] for command in run.commands], ["generate-group-shared-video"])
            self.assertEqual(len(run.fixture_setups), 2)

    def test_generated_cache_identity_mismatch_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            out, cache = root / "run", root / "cache"
            out.mkdir()
            cache.mkdir()
            run = make_run(str(out), cache)
            fixture = source_fixture(cache)
            (cache / "input.mp4").write_bytes(b"changed mock input")
            run.command = lambda *args, **kwargs: self.fail("identity check must precede all commands")
            with self.assertRaisesRegex(ValueError, "identity mismatch"):
                run.fixture_path(fixture)

    def test_conflicting_generation_group_recipe_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            out = root / "run"
            out.mkdir()
            run = make_run(str(out), None)
            run.checked = lambda name, argv, env=None: Path(argv[-1]).mkdir(parents=True)
            base = {"kind": "generated", "path": "generated/input.mp4", "byte_size": 1,
                    "sha256": hashlib.sha256(b"x").hexdigest(), "generation_group": "group",
                    "exact_command": ["python3", "one.py", "{output_directory}"]}
            fixture = {"id": "first", "source": base}
            run.fixture_path = RealMediaMixin.fixture_path.__get__(run, TestableRun)
            # The first recipe emits its claimed file; the second recipe conflicts before use.
            def checked(name, argv, env=None, timeout=None):
                directory = Path(argv[-1])
                directory.mkdir(parents=True, exist_ok=True)
                (directory / "input.mp4").write_bytes(b"x")
                return {"command": name}
            run.checked = checked
            run.fixture_path(fixture)
            changed = {"id": "second", "source": {**base,
                       "exact_command": ["python3", "two.py", "{output_directory}"]}}
            with self.assertRaisesRegex(ValueError, "conflicting recipes"):
                run.fixture_path(changed)

    def _install_fixture_command(self, run: TestableRun, *, plan_success: bool,
                                 runtime_success: bool, plan_document: dict | None = None,
                                 runtime_document: dict | None = None):
        """Replace only Run.command; all orchestration and sentinel checks remain real."""
        commands = []

        def mock_command(label, argv, env=None, timeout=None, stderr_log=None):
            commands.append(label)
            log = run.out / f"command-{len(run.commands):03d}.log"
            if "-input-probe" in label or "-output-probe" in label:
                log.write_text(json.dumps(PROBE), encoding="utf-8")
                if stderr_log:
                    Path(stderr_log).write_text("", encoding="utf-8")
                success = True
            elif label.startswith("plan-"):
                diagnostic = plan_document or supported_plan()
                Path(argv[argv.index("--diagnostic-report") + 1]).write_text(
                    json.dumps(diagnostic), encoding="utf-8")
                success = plan_success
            elif label.startswith("runtime-"):
                diagnostic = runtime_document or {
                    **supported_plan(), "failure": {"stage": "ProcessingRuntime", "category": "MockFailure"}}
                Path(argv[argv.index("--diagnostic-report") + 1]).write_text(
                    json.dumps(diagnostic), encoding="utf-8")
                success = runtime_success
                if success:
                    Path(argv[2]).write_bytes(b"mock converted output")
            else:
                self.fail(f"unexpected mocked command label: {label}")
            run.commands.append({"id": label, "argv": [str(value) for value in argv],
                                 "exit_code": 0 if success else 1, "log": log.name,
                                 "watchdog": "completed", "mocked": True})
            return success

        run.command = mock_command
        return commands

    def test_source_probe_mismatch_stops_before_plan_or_conversion(self) -> None:
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            out, cache = root / "run", root / "cache"
            out.mkdir()
            cache.mkdir()
            run = make_run(str(out), cache)
            fixture = source_fixture(cache, expected_probe="0" * 64)
            commands = self._install_fixture_command(run, plan_success=True, runtime_success=True)
            with self.assertRaisesRegex(ValueError, "actual full probe identity mismatch"):
                run.fixture(fixture)
            self.assertEqual(commands, ["mock-fixture-input-probe"])
            self.assertFalse((out / "mock-fixture-output.mp4").exists())

    def test_media_facts_mismatch_stops_before_production(self) -> None:
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            out, cache = root / "run", root / "cache"
            out.mkdir()
            cache.mkdir()
            run = make_run(str(out), cache)
            fixture = source_fixture(cache)
            fixture["media"]["width"] += 2
            commands = self._install_fixture_command(run, plan_success=True, runtime_success=True)
            with self.assertRaisesRegex(ValueError, "actual media facts mismatch"):
                run.fixture(fixture)
            self.assertEqual(commands, ["mock-fixture-input-probe"])

    def test_fatal_plan_mismatch_still_executes_and_inspects_sentinel_safety(self) -> None:
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            out, cache = root / "run", root / "cache"
            out.mkdir()
            cache.mkdir()
            run = make_run(str(out), cache)
            fixture = source_fixture(cache, runtime="RejectAtPlanning",
                                     failure_stage="Planning", root_cause="PolicyRejected")
            commands = self._install_fixture_command(run, plan_success=True, runtime_success=False)
            with self.assertRaisesRegex(ValueError, "unexpected planning outcome"):
                run.fixture(fixture)
            self.assertEqual(commands, ["mock-fixture-input-probe", "plan-mock-fixture", "runtime-mock-fixture"])
            transaction = json.loads((out / "mock-fixture-transaction.json").read_text())
            self.assertTrue(transaction["sentinel_preserved"])
            self.assertEqual(transaction["staging_files"], [])
            self.assertIn(b"failure must preserve", (out / "mock-fixture-output.mp4").read_bytes())

    def test_positive_plan_alone_does_not_pass_without_runtime_conversion(self) -> None:
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            out, cache = root / "run", root / "cache"
            out.mkdir()
            cache.mkdir()
            run = make_run(str(out), cache)
            fixture = source_fixture(cache, runtime="Pass")
            commands = self._install_fixture_command(run, plan_success=True, runtime_success=False)
            with self.assertRaisesRegex(ValueError, "unexpected conversion outcome"):
                run.fixture(fixture)
            self.assertEqual(commands, ["mock-fixture-input-probe", "plan-mock-fixture", "runtime-mock-fixture"])
            self.assertFalse((out / "mock-fixture-output-probe.json").exists())

    def test_unexpected_negative_success_fails_when_target_changes(self) -> None:
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            out, cache = root / "run", root / "cache"
            out.mkdir()
            cache.mkdir()
            run = make_run(str(out), cache)
            fixture = source_fixture(cache, runtime="RejectAtInitialization",
                                     failure_stage="Initialization", root_cause="MockFailure")
            commands = self._install_fixture_command(run, plan_success=True, runtime_success=True)
            with self.assertRaisesRegex(AssertionError, "fatal conversion changed existing destination"):
                run.fixture(fixture)
            self.assertEqual(commands, ["mock-fixture-input-probe", "plan-mock-fixture", "runtime-mock-fixture"])
            self.assertEqual((out / "mock-fixture-output.mp4").read_bytes(), b"mock converted output")

    @staticmethod
    def _audio_probe(*, time_base="1/1000", language=None, default=0,
                     packet_count=2, durations=(21, 21)):
        tags = {} if language is None else {"language": language}
        packets = []
        for index in range(packet_count):
            packet = {"stream_index": 1, "pts": index * 21, "dts": index * 21,
                      "size": 8 + index, "data_hash": f"SHA256:packet-{index}"}
            duration = durations[index] if index < len(durations) else None
            if duration is not None:
                packet["duration"] = duration
            packets.append(packet)
        return {"streams": [{"index": 1, "codec_type": "audio", "codec_name": "aac",
                             "sample_rate": "48000", "channels": 2, "channel_layout": "stereo",
                             "time_base": time_base, "tags": tags,
                             "disposition": {"default": default}}],
                "packets": packets}

    def test_registered_millisecond_aac_duration_keeps_old_strict_failure_visible(self) -> None:
        source = self._audio_probe(time_base="1/1000")
        output = self._audio_probe(time_base="1/48000", durations=(1024, 1024))
        for index, (before, after) in enumerate(zip(source["packets"], output["packets"])):
            after["pts"] = before["pts"] * 48
            after["dts"] = before["dts"] * 48
        report = characterize_audio_limits(source, output, {"millisecond_duration"})[0]
        self.assertEqual(report["qualification"], "Unqualified; original strict oracle failure retained")
        self.assertEqual(report["payload_sha256_size_order"], "identical")
        self.assertEqual([item["packet"] for item in report["strict_duration_mismatches"]], [0, 1])
        mismatch = Fraction(report["strict_duration_mismatches"][0]["difference_seconds"])
        output_tick = Fraction(output["streams"][0]["time_base"])
        self.assertGreater(mismatch, output_tick)  # The unchanged Rust oracle remains a real failure.
        self.assertLessEqual(mismatch, Fraction(report["observation_bound_seconds"]))

    def test_audio_payload_order_count_rate_layout_and_rational_timestamps_stay_strict(self) -> None:
        source = self._audio_probe()
        mutations = []

        payload = copy.deepcopy(source)
        payload["packets"][0]["data_hash"] = "SHA256:changed"
        mutations.append(("payload hash", payload))
        packet_size = copy.deepcopy(source)
        packet_size["packets"][0]["size"] += 1
        mutations.append(("packet size", packet_size))
        pts = copy.deepcopy(source)
        pts["packets"][0]["pts"] += 1
        mutations.append(("PTS", pts))
        dts = copy.deepcopy(source)
        dts["packets"][0]["dts"] += 1
        mutations.append(("DTS", dts))
        packet_order = copy.deepcopy(source)
        packet_order["packets"].reverse()
        mutations.append(("packet order", packet_order))
        packet_count = copy.deepcopy(source)
        packet_count["packets"].pop()
        mutations.append(("packet count", packet_count))
        for key, value in (("sample_rate", "44100"), ("channels", 1),
                           ("channel_layout", "mono"), ("codec_name", "mp3")):
            stream_change = copy.deepcopy(source)
            stream_change["streams"][0][key] = value
            mutations.append((key, stream_change))

        for label, changed in mutations:
            with self.subTest(label=label), self.assertRaises(AssertionError):
                characterize_audio_limits(source, changed, {
                    "millisecond_duration", "undefined_language", "default_disposition"})

    def test_undefined_language_and_default_changes_allow_only_registered_direction(self) -> None:
        source = self._audio_probe(language=None, default=0)
        output = self._audio_probe(language="und", default=1)
        reports = characterize_audio_limits(source, output,
                                            {"undefined_language", "default_disposition"})
        self.assertEqual(reports[0]["language"], [None, "und"])
        self.assertEqual(reports[0]["default"], [0, 1])
        for limits in (set(), {"default_disposition"}, {"undefined_language"}):
            with self.subTest(limits=limits), self.assertRaises(AssertionError):
                characterize_audio_limits(source, output, limits)

        for src_language, dst_language, src_default, dst_default in (
            ("eng", "und", 0, 0), (None, "en", 0, 0), ("eng", None, 0, 0),
            (None, None, 1, 0), (None, None, 0, 2),
        ):
            before = self._audio_probe(language=src_language, default=src_default)
            after = self._audio_probe(language=dst_language, default=dst_default)
            with self.subTest(transition=(src_language, dst_language, src_default, dst_default)), \
                    self.assertRaises(AssertionError):
                characterize_audio_limits(before, after,
                                          {"undefined_language", "default_disposition"})

    def test_missing_source_duration_is_allowed_only_on_first_packet_and_exact_interval(self) -> None:
        source = self._audio_probe(time_base="1/1000", durations=(None, 21))
        output = self._audio_probe(time_base="1/48000", durations=(1008, 1024))
        for before, after in zip(source["packets"], output["packets"]):
            after["pts"] = before["pts"] * 48
            after["dts"] = before["dts"] * 48
        report = characterize_audio_limits(
            source, output, {"missing_first_duration", "millisecond_duration"})[0]
        self.assertEqual(report["missing_source_duration_packets"], [0])
        with self.assertRaises(AssertionError):
            characterize_audio_limits(source, output, {"millisecond_duration"})

        later_missing = self._audio_probe(time_base="1/1000", durations=(21, None))
        for before, after in zip(later_missing["packets"], output["packets"]):
            after["pts"] = before["pts"] * 48
            after["dts"] = before["dts"] * 48
        with self.assertRaises(AssertionError):
            characterize_audio_limits(later_missing, output,
                                      {"missing_first_duration", "millisecond_duration"})

        wrong_interval = copy.deepcopy(output)
        wrong_interval["packets"][0]["duration"] = 1007
        with self.assertRaises(AssertionError):
            characterize_audio_limits(source, wrong_interval,
                                      {"missing_first_duration", "millisecond_duration"})

    def test_unknown_audio_limit_does_not_authorize_an_unregistered_change(self) -> None:
        source = self._audio_probe()
        changed = copy.deepcopy(source)
        changed["packets"][0]["data_hash"] = "SHA256:mutated"
        with self.assertRaises(AssertionError):
            characterize_audio_limits(source, changed, {"unexpected_limit_name"})

    def test_audio_limit_on_supported_fixture_is_rejected_by_real_scope(self) -> None:
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            out, cache = root / "run", root / "cache"
            out.mkdir()
            cache.mkdir()
            run = make_run(str(out), cache)
            input_path = cache / "input.mp4"
            source_fixture(cache)
            source_doc = copy.deepcopy(PROBE)
            source_doc["streams"].append({"index": 1, "codec_type": "audio", "codec_name": "aac",
                                          "time_base": "1/48000", "sample_rate": "48000",
                                          "channels": 2, "channel_layout": "stereo",
                                          "tags": {}, "disposition": {"default": 0}})
            source_doc["packets"].append({"stream_index": 1, "pts": 0, "dts": 0,
                                          "duration": 1024, "size": 8, "data_hash": "SHA256:a"})
            run.fixture_path = lambda fixture: input_path
            run.real_probe = lambda name, path, allow_failure=False: (copy.deepcopy(source_doc), {"path": "mock-probe"})
            run.real_decode_back = lambda *args: {"scope": "mocked"}
            from real_media import timing
            timing_compare = timing.compare
            timing.compare = lambda *args: {"mocked_timing": True}

            def mock_command(label, argv, env=None, timeout=None, stderr_log=None):
                log = out / f"command-{len(run.commands):03d}.log"
                success = True
                if label.startswith("plan-"):
                    Path(argv[argv.index("--diagnostic-report") + 1]).write_text(
                        json.dumps(supported_plan()), encoding="utf-8")
                elif label.startswith("runtime-"):
                    Path(argv[argv.index("--diagnostic-report") + 1]).write_text(
                        json.dumps({**supported_plan(), "plan_scope": "initialized_execution"}), encoding="utf-8")
                    Path(argv[2]).write_bytes(b"mock runtime output")
                elif label.startswith("audio-oracle-"):
                    success = False  # Explicit mock of unchanged strict-oracle failure.
                else:
                    self.fail(f"unexpected mocked command: {label}")
                log.write_text("mocked command output", encoding="utf-8")
                run.commands.append({"id": label, "argv": [str(value) for value in argv],
                                     "exit_code": 0 if success else 1, "log": log.name,
                                     "watchdog": "completed", "mocked": True})
                return success

            run.command = mock_command
            fixture = source_fixture(cache, runtime="Pass")
            fixture["id"] = "supported-with-invalid-audio-limit"
            fixture["media"] = _manifest.describe(source_doc, "input.mp4")[0]
            fixture["request"]["audio"] = "copy"
            fixture["expected"].update(status="Supported", blocker=None,
                                       audio_limits=["millisecond_duration"])
            try:
                with self.assertRaises(AssertionError):
                    run.fixture(fixture)
            finally:
                timing.compare = timing_compare
            self.assertEqual(run.commands[-1]["id"], "audio-oracle-supported-with-invalid-audio-limit")
            self.assertTrue(run.commands[-1]["mocked"])

    def test_unqualified_audio_oracle_failure_never_becomes_pass_surface(self) -> None:
        source = self._audio_probe(time_base="1/1000")
        output = self._audio_probe(time_base="1/48000", durations=(1024, 1024))
        for before, after in zip(source["packets"], output["packets"]):
            after["pts"] = before["pts"] * 48
            after["dts"] = before["dts"] * 48
        limitation = characterize_audio_limits(source, output, {"millisecond_duration"})
        with tempfile.TemporaryDirectory() as folder:
            run = Run.__new__(Run)
            run.out = Path(folder)
            run.args = SimpleNamespace(mode="full")
            run.results = []
            run.commands = []
            run.fixture_setups = []
            evidence = {"unqualified": True,
                        "audio_oracle": {"result": "FAILED", "scope": "unchanged strict retained audio oracle"},
                        "audio_limitation": limitation}
            self.assertTrue(run.gate("audio-candidate", "media oracle", lambda: evidence))
            self.assertEqual(run.results[0]["result"], "UNQUALIFIED")
            with redirect_stdout(io.StringIO()):
                self.assertEqual(run.finish(), 0)
            summary = json.loads((run.out / "results.json").read_text())
            self.assertEqual(summary["summary"]["media oracle"]["UNQUALIFIED"], 1)
            self.assertNotIn("PASS", summary["summary"]["media oracle"])
            self.assertEqual(summary["verified_surfaces"], [])
            self.assertEqual(summary["results"][0]["evidence"]["audio_oracle"]["result"], "FAILED")

    def _run_registered_audio_fixture(self, folder: Path, *, report_mode="correct",
                                       decode_failure=None, lose_title=False):
        out, cache = folder / "run", folder / "cache"
        out.mkdir()
        cache.mkdir()
        run = make_run(str(out), cache)
        fixture = source_fixture(cache, runtime="Pass")
        fixture["id"] = "registered-audio-limit"
        fixture["request"]["audio"] = "copy"
        fixture["expected"].update(status="Unqualified", blocker="registered observed AAC limitation",
                                   audio_limits=["undefined_language"])
        source_doc = copy.deepcopy(PROBE)
        source_doc["streams"].append({"index": 1, "codec_type": "audio", "codec_name": "aac",
                                      "time_base": "1/48000", "sample_rate": "48000",
                                      "channels": 2, "channel_layout": "stereo", "tags": {},
                                      "disposition": {"default": 0}})
        source_doc["packets"].append({"stream_index": 1, "pts": 0, "dts": 0, "duration": 1024,
                                      "size": 8, "data_hash": "SHA256:source-audio"})
        if lose_title:
            source_doc["streams"][1]["tags"]["name"] = "English tone"
        output_doc = copy.deepcopy(source_doc)
        fixture["media"] = _manifest.describe(source_doc, "input.mp4")[0]
        output_doc["streams"][1]["tags"] = {"language": "und"}
        run.fixture_path = lambda value: cache / "input.mp4"
        run.real_probe = lambda name, path, allow_failure=False: (
            copy.deepcopy(output_doc if "-output-probe" in name else source_doc), {"path": "mock-probe"})
        run.real_decode_back = lambda *args: {"scope": "mocked video decode-back"}
        command_argv = []

        def mock_command(label, argv, env=None, timeout=None, stderr_log=None):
            command_argv.append((label, [str(value) for value in argv]))
            log = out / f"command-{len(run.commands):03d}.log"
            success = True
            if label.startswith("plan-"):
                Path(argv[argv.index("--diagnostic-report") + 1]).write_text(
                    json.dumps(supported_plan()), encoding="utf-8")
            elif label.startswith("runtime-"):
                Path(argv[argv.index("--diagnostic-report") + 1]).write_text(
                    json.dumps({**supported_plan(), "plan_scope": "initialized_execution"}), encoding="utf-8")
                Path(argv[2]).write_bytes(b"mock output media")
            elif label.startswith("audio-oracle-"):
                success = False  # Bounded mock of the unchanged strict Rust audio oracle failing.
                report_path = Path(env["ASCIIFLOW_AUDIO_ORACLE_REPORT"])
                self.assertFalse(report_path.exists(), "strict report path must be fresh")
                if report_mode != "missing" and report_mode != "compiler_failure":
                    candidate = Path(env["ASCIIFLOW_AUDIO_CANDIDATE"])
                    report = {"schema_version": 1, "test": "retained_audio_pair_from_env",
                              "outcome": "STRICT_FAILURE", "code": "MetadataMismatch",
                              "reference": str(cache / "input.mp4"), "candidate": str(candidate)}
                    if report_mode == "wrong_code":
                        report["code"] = "PayloadMismatch"
                    elif report_mode == "wrong_candidate":
                        report["candidate"] = str(out / "other.mp4")
                    elif report_mode == "wrong_reference":
                        report["reference"] = str(cache / "other.mp4")
                    elif report_mode == "wrong_outcome":
                        report["outcome"] = "PASS"
                    elif report_mode == "extra_field":
                        report["message"] = "unexpected report shape"
                    report_path.write_text(json.dumps(report), encoding="utf-8")
            elif label.startswith("full-audio-decode-"):
                success = label != decode_failure
            else:
                self.fail(f"unexpected mocked command: {label}")
            log.write_text("mocked cargo compilation error" if report_mode == "compiler_failure" and
                           label.startswith("audio-oracle-") else "mocked command output", encoding="utf-8")
            run.commands.append({"id": label, "argv": [str(value) for value in argv],
                                 "exit_code": 0 if success else 1, "log": log.name,
                                 "watchdog": "completed", "mocked": True})
            return success

        run.command = mock_command
        from real_media import timing
        with patch.object(timing, "compare", return_value={"mocked_timing": True}):
            completed = run.gate("registered-audio-limit", "media oracle", lambda: run.fixture(fixture))
        return run, command_argv, completed

    def test_audio_title_loss_fails_before_registered_limitation_can_mask_it(self):
        with tempfile.TemporaryDirectory() as folder:
            run, calls, completed = self._run_registered_audio_fixture(Path(folder), lose_title=True)
            self.assertFalse(completed)
            self.assertEqual(run.results[0]["result"], "FAILED")
            self.assertIn("audio display titles", run.results[0]["reason"])
            self.assertFalse(any(label.startswith("audio-oracle-") for label, _ in calls))

    def test_registered_metadata_mismatch_requires_exact_fresh_strict_report_and_full_audio_decode(self) -> None:
        with tempfile.TemporaryDirectory() as folder:
            run, calls, completed = self._run_registered_audio_fixture(Path(folder))
            self.assertTrue(completed)
            self.assertEqual(run.results[0]["result"], "UNQUALIFIED")
            evidence = run.results[0]["evidence"]
            self.assertEqual(evidence["audio_oracle"]["result"], "FAILED")
            self.assertEqual(evidence["audio_oracle"]["structured_failure"]["code"], "MetadataMismatch")
            self.assertEqual(evidence["audio_limitation"][0]["qualification"],
                             "Unqualified; original strict oracle failure retained")
            self.assertEqual(evidence["audio_limitation_full_decode"], "source and output passed separately")
            decode_calls = [(label, argv) for label, argv in calls if label.startswith("full-audio-decode-")]
            self.assertEqual([label.rsplit("-", 1)[-1] for label, _ in decode_calls], ["source", "output"])
            for label, argv in decode_calls:
                self.assertIn("-xerror", argv)
                self.assertIn("-err_detect", argv)
                self.assertIn("explode", argv)
                self.assertIn("-map", argv)
                self.assertIn("0:a", argv)
                self.assertIn("-vn", argv)
                self.assertIn("-sn", argv)
                self.assertTrue(any(command["id"] == label and command["mocked"] for command in run.commands))

    def test_missing_compilerfailed_or_wrong_strict_report_is_failed_not_unqualified(self) -> None:
        for mode in ("missing", "compiler_failure", "wrong_code", "wrong_candidate",
                     "wrong_reference", "wrong_outcome", "extra_field"):
            with self.subTest(report_mode=mode), tempfile.TemporaryDirectory() as folder:
                run, calls, _ = self._run_registered_audio_fixture(Path(folder), report_mode=mode)
                self.assertEqual(run.results[0]["result"], "FAILED")
                self.assertNotEqual(run.results[0]["result"], "UNQUALIFIED")
                self.assertFalse(any(label.startswith("full-audio-decode-") for label, _ in calls))
                if mode == "compiler_failure":
                    oracle_command = next(command for command in run.commands
                                          if command["id"] == "audio-oracle-registered-audio-limit")
                    self.assertIn("mocked cargo compilation error",
                                  (run.out / oracle_command["log"]).read_text())

    def test_registered_metadata_mismatch_still_fails_if_either_full_audio_decode_fails(self) -> None:
        for failing_label in ("full-audio-decode-registered-audio-limit-source",
                              "full-audio-decode-registered-audio-limit-output"):
            with self.subTest(failing_label=failing_label), tempfile.TemporaryDirectory() as folder:
                run, calls, _ = self._run_registered_audio_fixture(Path(folder), decode_failure=failing_label)
                self.assertEqual(run.results[0]["result"], "FAILED")
                self.assertNotEqual(run.results[0]["result"], "UNQUALIFIED")
                self.assertEqual([label for label, _ in calls if label.startswith("full-audio-decode-")],
                                 ["full-audio-decode-registered-audio-limit-source",
                                  "full-audio-decode-registered-audio-limit-output"][:
                                  1 if failing_label.endswith("-source") else 2])

    def test_pq_decode_back_rejects_static_hdr_side_data_propagation(self) -> None:
        with tempfile.TemporaryDirectory() as folder:
            run = make_run(folder, Path(folder))
            run.checked = lambda *args, **kwargs: self.fail("poisoned static HDR metadata must reject before decode")
            fixture = {"id": "pq-static-side-data", "media": {"width": 128, "height": 96,
                                                                    "timing": {"frame_count": 1}},
                       "request": {"codec": "hevc", "bit_depth": 10, "dynamic_range": "preserve"},
                       "expected": {"classification": "HdrPq"}}
            stream = {"index": 0, "codec_type": "video", "codec_name": "hevc",
                      "profile": "Main 10", "width": 128, "height": 96,
                      "pix_fmt": "yuv420p10le", "color_primaries": "bt2020",
                      "color_transfer": "smpte2084", "color_space": "bt2020nc",
                      "color_range": "tv", "chroma_location": "left", "avg_frame_rate": "25/1",
                      "side_data_list": []}
            frame = {**stream, "side_data_list": [{"side_data_type": "Mastering display metadata"}]}
            probe = {"streams": [stream], "frames": [frame]}
            with self.assertRaises(AssertionError):
                run.real_decode_back(fixture, Path("mock-pq.mp4"), probe, "25/1")


if __name__ == "__main__":
    unittest.main()
