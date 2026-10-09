"""Contract tests for long-run resource summaries and AAC-copy oracles."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest


HERE = Path(__file__).parent


def load_module(name, filename):
    spec = importlib.util.spec_from_file_location(name, HERE / filename)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


SUMMARY = load_module("long_summary", "long_summary.py")
AUDIO = load_module("long_audio", "long_audio.py")


def resource_rows():
    rows = [{
        "phase": "initial", "frames_processed": 0, "rss_kib": 100,
        "fd_count": 8, "accounting_errors": [],
    }]
    for frame in (10000, 25000, 50000, 75000, 100000):
        rows.append({
            "phase": "progress", "frames_processed": frame,
            "rss_kib": 100 + frame // 1000, "fd_count": 8,
            "accounting_errors": [],
        })
    rows.append({
        "phase": "post-cleanup", "frames_processed": 100000,
        "rss_kib": 120, "fd_count": 8, "accounting_errors": [],
        "resources": {"frames": {"active_count": 0}},
        "queues": {"decode": {"peak_depth": 3, "capacity": 4}},
        "mux": {"packets_written": 100000},
    })
    return rows


def process_rows():
    return [
        {"frames_processed": frame, "elapsed_seconds": frame / 1000,
         "rss_kib": 110 + frame // 1000}
        for frame in (0, 10000, 45000, 50000, 55000, 90000, 100000)
    ]


class LongSummaryTests(unittest.TestCase):
    def test_mux_counts_include_all_copied_audio_packets(self):
        for counts, total in (([], 100000), ([93751], 193751), ([93751, 93751], 287502)):
            SUMMARY.verify_mux_counts({"frames_processed": 100000, "packets_processed": total}, 100000, counts)

    def test_video_only_mux_assumption_rejects_audio_job(self):
        with self.assertRaises(AssertionError):
            SUMMARY.verify_mux_counts({"frames_processed": 100000, "packets_processed": 100000}, 100000, [93751])

    def test_inexact_mux_count_is_not_accepted(self):
        with self.assertRaises(AssertionError):
            SUMMARY.verify_mux_counts({"frames_processed": 100000, "packets_processed": 193750}, 100000, [93751])

    def test_rejects_lifecycle_fd_mismatch(self):
        rows = resource_rows()
        rows[-1]["fd_count"] += 1
        with self.assertRaises(AssertionError):
            SUMMARY.summarize(rows, process_rows())

    def test_rejects_active_resources_after_cleanup(self):
        rows = resource_rows()
        rows[-1]["resources"]["frames"]["active_count"] = 1
        with self.assertRaises(AssertionError):
            SUMMARY.summarize(rows, process_rows())

    def test_rejects_queue_peak_above_capacity(self):
        rows = resource_rows()
        rows[-1]["queues"]["decode"].update(peak_depth=5, capacity=4)
        with self.assertRaises(AssertionError):
            SUMMARY.summarize(rows, process_rows())

    def test_unused_audio_channel_keeps_unavailable_peak(self):
        rows = resource_rows()
        rows[-1]["queues"]["audio"] = {
            "capacity": 16, "peak_depth": None, "boundary_observations": 0,
        }
        result = SUMMARY.summarize(rows, process_rows())
        self.assertIsNone(result["queue_observations"]["audio"]["peak_depth"])
        rows[-1]["queues"]["audio"]["boundary_observations"] = 1
        with self.assertRaises(AssertionError):
            SUMMARY.summarize(rows, process_rows())

    def test_reports_milestones_and_correlated_throughput(self):
        report = SUMMARY.summarize(resource_rows(), process_rows())
        self.assertEqual(
            set(report["milestones"]),
            {"0", "10000", "25000", "50000", "75000", "100000"},
        )
        self.assertEqual(
            report["milestones"]["50000"]["nearest_external_observation"]["frames_processed"],
            50000,
        )
        self.assertEqual(report["throughput"]["first_10k"]["fps"], 1000)
        self.assertEqual(report["throughput"]["middle_10k"]["observed_start_frame"], 45000)
        self.assertEqual(report["throughput"]["middle_10k"]["observed_end_frame"], 55000)
        self.assertEqual(report["throughput"]["last_10k"]["fps"], 1000)


def probe(stream_title="Original title", packet_hash="sha256:packet-1", pts=0):
    stream = {
        "index": 1, "codec_name": "aac", "sample_rate": "48000",
        "channels": 2, "channel_layout": "stereo", "time_base": "1/48000",
        "disposition": {"default": 1}, "tags": {"title": stream_title},
    }
    return {
        "streams": [stream],
        "packets": [{
            "stream_index": 1, "data_hash": packet_hash, "size": "16",
            "pts": str(pts), "dts": str(pts), "duration": "1024",
        }],
    }


class MockRun:
    def __init__(self, directory, probes):
        self.out = Path(directory)
        self.probes = iter(probes)
        self.commands = []

    def checked(self, label, argv):
        log = f"{len(self.commands):03d}.json"
        (self.out / log).write_text(json.dumps(next(self.probes)))
        self.commands.append({"label": label, "argv": argv, "log": log})


class LongAudioTests(unittest.TestCase):
    def assert_oracle_rejects(self, source, output):
        with tempfile.TemporaryDirectory() as directory:
            run = MockRun(directory, [source, output])
            with self.assertRaises(AssertionError):
                AUDIO.verify_audio(run, "long-audio", "source.mp4", "output.mp4", 1)

    def test_rejects_packet_payload_mismatch(self):
        self.assert_oracle_rejects(probe(), probe(packet_hash="sha256:different"))

    def test_rejects_packet_timestamp_mismatch(self):
        self.assert_oracle_rejects(probe(), probe(pts=1024))

    def test_rejects_stream_title_mismatch(self):
        self.assert_oracle_rejects(probe(), probe(stream_title="Changed title"))


if __name__ == "__main__":
    unittest.main()
