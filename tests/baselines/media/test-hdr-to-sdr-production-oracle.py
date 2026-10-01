#!/usr/bin/env python3
"""Offline negative controls; these tests do not qualify production hardware."""
import copy
import importlib.util
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("oracle", Path(__file__).with_name("verify-hdr-to-sdr-production.py"))
oracle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(oracle)


class SdrOracleTests(unittest.TestCase):
    def signal(self):
        return {"color_primaries": "bt709", "color_transfer": "bt709",
                "color_space": "bt709", "color_range": "tv"}

    def frames(self, depth=8):
        stream = dict(self.signal(), codec_name="hevc", profile="Main 10" if depth == 10 else "Main", width=1920, height=1080,
                      pix_fmt="yuv420p10le" if depth == 10 else "yuv420p",
                      avg_frame_rate="50/1", time_base="1/50000", chroma_location="left")
        frames = [dict(stream, best_effort_timestamp=index * 1000) for index in range(300)]
        return stream, frames

    def test_all_depths_and_timestamps(self):
        for depth in (8, 10):
            oracle.check_frames(*self.frames(depth), "hevc", depth)

    def test_codec_profile_contract(self):
        for codec, depth, profile in (("h264", 8, "High"), ("hevc", 8, "Main"),
                                      ("hevc", 10, "Main 10"), ("av1", 8, "Main"), ("av1", 10, "Main")):
            with self.subTest(codec=codec, depth=depth):
                stream, frames = self.frames(depth)
                stream.update(codec_name=codec, profile=profile)
                oracle.check_frames(stream, frames, codec, depth)
                stream["profile"] = "wrong-profile"
                with self.assertRaises(AssertionError):
                    oracle.check_frames(stream, frames, codec, depth)

    def test_production_source_selection_includes_new_sources_not_fixtures(self):
        for filename in ("crates/asciiflow-vulkan/src/new_c4a.rs", "apps/asciiflow-cli/src/main.rs",
                         "crates/asciiflow-vulkan/build.rs", "crates/asciiflow-core/Cargo.toml",
                         "shaders/src/new_sdr.comp"):
            self.assertTrue(oracle.is_production_source(filename), filename)
        for filename in ("crates/asciiflow-vulkan/tests/hardware.rs", "apps/asciiflow-cli/tests/test.rs",
                         "crates/asciiflow-core/target/src/scratch.rs", "tests/fixtures/codecs/huge.mp4"):
            self.assertFalse(oracle.is_production_source(filename), filename)

    def test_build_identity_records_untracked_source_and_machine_versions(self):
        files = "crates/asciiflow-vulkan/src/new_c4a.rs\ncrates/asciiflow-vulkan/tests/test.rs\nshaders/src/new.comp\n"
        packages = "ffmpeg 0:8.1.3-1.x86_64\nffmpeg-libs 0:8.1.3-1.x86_64\nMesa-libvulkan 0:26-1.x86_64\nintel-media-driver 0:26-1.x86_64\nSPIRV-Tools 0:2026-1.x86_64\nunrelated 0:1-1.x86_64\n"
        with patch.object(oracle, "run", side_effect=[files, packages, "Linux test-kernel\n"]), \
             patch.object(oracle.shutil, "which", return_value="/usr/bin/rpm"), \
             patch.object(oracle.Path, "is_file", return_value=True), \
             patch.object(oracle, "file_hash", side_effect=lambda path: "sha:" + str(path)):
            identity = oracle.build_identity(Path("binary"))
        self.assertIn("crates/asciiflow-vulkan/src/new_c4a.rs", identity["source_sha256"])
        self.assertNotIn("crates/asciiflow-vulkan/tests/test.rs", identity["source_sha256"])
        self.assertIn("Cargo.lock", identity["source_sha256"])
        self.assertEqual(identity["kernel"], "Linux test-kernel")
        self.assertEqual(len(identity["rpm"]["packages"]), 5)

    def test_signal_mutations_and_missing_fields_fail(self):
        for key, invalid in (("color_primaries", "bt2020"), ("color_transfer", "smpte2084"),
                             ("color_space", "bt2020nc"), ("color_range", "pc")):
            for value in (invalid, None):
                with self.subTest(key=key, value=value):
                    signal = self.signal()
                    if value is None:
                        del signal[key]
                    else:
                        signal[key] = value
                    with self.assertRaises(AssertionError):
                        oracle.check_color(signal)

    def test_hdr_metadata_leaks_fail(self):
        for kind in ("Mastering display metadata", "Content light level metadata",
                     "HDR Dynamic Metadata SMPTE2094-40 (HDR10+)", "DOVI configuration record"):
            with self.subTest(kind=kind), self.assertRaises(AssertionError):
                oracle.check_color(dict(self.signal(), side_data_list=[{"side_data_type": kind}]))

    def test_full_frame_count_geometry_depth_color_and_timestamp_gate(self):
        mutations = (("width", 1918), ("height", 1078), ("pix_fmt", "yuv420p10le"),
                     ("color_transfer", "smpte2084"), ("chroma_location", "center"),
                     ("best_effort_timestamp", 299001))
        for key, invalid in mutations:
            with self.subTest(key=key):
                stream, frames = self.frames()
                frames[-1][key] = invalid
                with self.assertRaises(AssertionError):
                    oracle.check_frames(stream, frames, "hevc", 8)
        stream, frames = self.frames()
        with self.assertRaises(AssertionError):
            oracle.check_frames(stream, frames[:-1], "hevc", 8)

    def record(self):
        return {"packets": ["packet"], "stream": {"signal": "sdr"},
                "elementary_stream": {"signal": "sdr"}, "frames": ["frame"],
                "decoded_framehash": ["hash"], "identity": {"sha256": "one"}}

    def test_each_authoritative_tier_remains_required(self):
        for field in ("packets", "stream", "elementary_stream", "frames", "decoded_framehash"):
            with self.subTest(field=field):
                records = [self.record() for _ in range(3)]
                records[-1][field] = "changed"
                with self.assertRaises(AssertionError):
                    oracle.compare_runs(records)

    def test_whole_file_hash_gate_is_optional_but_retained_when_established(self):
        records = [self.record() for _ in range(3)]
        records[-1]["identity"]["sha256"] = "different-container"
        self.assertFalse(oracle.compare_runs(records)["tier3"])
        report = {"input_identity": {"manifest": {"sha256": "input"}}, "build_identity": {"binary_sha256": "build"},
                  "profiles": {"hevc-to-hevc-8": {"runs": [self.record()], "tiers": {"tier3": True}}}}
        changed = copy.deepcopy(report)
        changed["profiles"]["hevc-to-hevc-8"]["runs"][0]["identity"]["sha256"] = "changed"
        with self.assertRaises(AssertionError):
            oracle.check_baseline(changed, report)
        report["profiles"]["hevc-to-hevc-8"]["tiers"]["tier3"] = False
        oracle.check_baseline(changed, report)

    def test_tier3_retains_exact_build_scope(self):
        baseline = {"input_identity": {"manifest": {"sha256": "input"}},
                    "build_identity": {"source_sha256": {"new-c4a.rs": "original"}},
                    "profiles": {"hevc-to-hevc-8": {"runs": [self.record()], "tiers": {"tier3": True}}}}
        changed = copy.deepcopy(baseline)
        changed["build_identity"]["source_sha256"]["new-c4a.rs"] = "changed"
        with self.assertRaisesRegex(AssertionError, "exact build scope changed"):
            oracle.check_baseline(changed, baseline)

    def test_artifact_comparison_keeps_all_tiers_without_transferring_build_scope(self):
        baseline = {"input_identity": {"manifest": {"sha256": "input"}},
                    "build_identity": {"source": "old"},
                    "profiles": {"hevc-to-hevc-8": {"runs": [self.record()], "tiers": {"tier3": True}}}}
        candidate = copy.deepcopy(baseline)
        candidate["build_identity"] = {"source": "new"}
        oracle.check_retained_artifacts(candidate, baseline)
        for field in ("packets", "stream", "elementary_stream", "frames", "decoded_framehash", "identity"):
            changed = copy.deepcopy(candidate)
            changed["profiles"]["hevc-to-hevc-8"]["runs"][0][field] = {"sha256": "changed"} if field == "identity" else "changed"
            with self.subTest(field=field), self.assertRaises(AssertionError):
                oracle.check_retained_artifacts(changed, baseline)


if __name__ == "__main__":
    unittest.main()
