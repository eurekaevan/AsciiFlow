"""Synthetic ffprobe timing contract tests; no media fixtures are needed."""

from __future__ import annotations

import unittest

from timing import analyze, compare


def video_document(time_base: str, pts: list[int], durations: list[int | None],
                   *, rate: str, packets: list[dict] | None = None) -> dict:
    frames = [{"media_type": "video", "stream_index": 0, "pts": point,
               "duration": duration, "key_frame": int(index == 0)}
              for index, (point, duration) in enumerate(zip(pts, durations))]
    if packets is None:
        packets = [{"stream_index": 0, "pts": point, "dts": point, "duration": duration}
                   for point, duration in zip(pts, durations)]
    return {"streams": [{"index": 0, "codec_type": "video", "time_base": time_base,
                         "avg_frame_rate": rate, "r_frame_rate": rate}],
            "frames": frames, "packets": packets or []}


class TimingTests(unittest.TestCase):
    def test_long_rational_cfr_has_no_accumulated_drift(self) -> None:
        for rate, time_base, tick_step, count in (
            ("24000/1001", "1/24000", 1001, 2400),
            ("30000/1001", "1/90000", 3003, 3000),
        ):
            points = [index * tick_step for index in range(count)]
            durations = [tick_step] * count
            document = video_document(time_base, points, durations, rate=rate)
            report = compare(document, document, rate, "cfr_zero")
            self.assertEqual(report["video"][0]["max_frame_pts_error"], "0/1")
            self.assertEqual(report["source_output_duration_error"], "0/1")

    def test_nonzero_origin_is_preserved_with_only_rescale_tick_tolerance(self) -> None:
        source = video_document("1/1000", [5000, 5042, 5083, 5125], [42] * 4, rate="24/1")
        output = video_document("1/90000", [450000, 453750, 457500, 461250], [3750] * 4,
                                rate="24/1")
        report = compare(source, output, "24/1", "cfr_origin")
        self.assertEqual(report["video"][0]["expected_origin"], "5/1")
        self.assertEqual(report["video"][0]["max_frame_pts_error"], "0/1")
        self.assertTrue(report["source_origin_preserved"])

    def test_packet_dts_reorder_and_negative_dts_are_legal(self) -> None:
        packets = [{"stream_index": 0, "pts": pts, "dts": dts, "duration": 1}
                   for pts, dts in zip((0, 3, 1, 2), (-2, -1, 0, 1))]
        document = video_document("1/24", [0, 1, 2, 3], [1] * 4, rate="24/1", packets=packets)
        report = compare(document, document, "24/1", "cfr_zero")["video"][0]
        self.assertTrue(report["packet_dts_monotonic"])
        self.assertTrue(report["packet_pts_dts_reordering_observed"])
        self.assertTrue(report["negative_packet_dts_observed"])

    def test_vfr_retime_is_reported_without_source_timing_claim(self) -> None:
        source = video_document("1/1000", [0, 50, 120], [40, 70, 40], rate="25/1")
        output = video_document("1/1000", [0, 40, 80], [40, 40, 40], rate="25/1")
        report = compare(source, output, "25/1", "retimed_unqualified")
        self.assertFalse(report["source_duration_preserved"])
        self.assertTrue(report["source_origin_preserved"])
        self.assertEqual(report["source_output_duration_error"], "1/25")
        self.assertEqual(report["time_unit"], "seconds")

    def test_audio_first_and_end_are_preserved_with_track_tick_tolerance(self) -> None:
        def with_audio(time_base: str, pts: list[int], packet_durations: list[int]) -> dict:
            doc = video_document("1/24", [0, 1], [1, 1], rate="24/1")
            doc["streams"].append({"index": 1, "codec_type": "audio", "time_base": time_base})
            doc["packets"].extend({"stream_index": 1, "pts": point, "dts": point,
                                   "duration": duration}
                                  for point, duration in zip(pts, packet_durations))
            return doc

        source = with_audio("1/48000", [0, 4800], [4800, 4800])
        output = with_audio("1/44100", [1, 4411], [4410, 4410])
        report = compare(source, output, "24/1", "cfr_zero")
        self.assertEqual(report["audio"][0]["first_delta"], "1/44100")
        self.assertEqual(report["audio"][0]["end_delta"], "1/44100")
        self.assertEqual(report["audio"][0]["audio_video_end_delta_error"], "1/44100")

    def test_audio_video_end_delta_drift_is_reported_and_audio_end_guard_rejects(self) -> None:
        source = video_document("1/24", [0, 1], [1, 1], rate="24/1")
        output = video_document("1/24", [0, 1], [1, 1], rate="24/1")
        source["streams"].append({"index": 1, "codec_type": "audio", "time_base": "1/48000"})
        output["streams"].append({"index": 1, "codec_type": "audio", "time_base": "1/48000"})
        source["packets"].append({"stream_index": 1, "pts": 0, "dts": 0, "duration": 4800})
        output["packets"].append({"stream_index": 1, "pts": 0, "dts": 0, "duration": 9600})
        with self.assertRaisesRegex(ValueError, "audio end timing changed"):
            compare(source, output, "24/1", "cfr_zero")

    def test_missing_last_video_duration_is_derived_only_from_stream_rate(self) -> None:
        document = video_document("1/1000", [0, 42], [42, None], rate="24000/1001")
        track = analyze(document)["video"][0]
        self.assertTrue(track["last_frame_duration_derived"])
        self.assertEqual(track["frame_durations"][-1], "1001/24000")
        self.assertEqual(track["duration"], "2009/24000")

    def test_output_must_have_all_frames_and_strictly_increasing_pts(self) -> None:
        source = video_document("1/24", [0, 1, 2], [1, 1, 1], rate="24/1")
        with self.assertRaisesRegex(ValueError, "frame count changed"):
            compare(source, video_document("1/24", [0, 1], [1, 1], rate="24/1"),
                    "24/1", "cfr_zero")
        with self.assertRaisesRegex(ValueError, "strictly increasing"):
            compare(source, video_document("1/24", [0, 1, 1], [1, 1, 1], rate="24/1"),
                    "24/1", "cfr_zero")

    def test_origin_policy_requires_source_origin(self) -> None:
        source = video_document("1/24", [0, 1], [1, 1], rate="24/1")
        source["frames"] = [{"media_type": "video", "stream_index": 0,
                             "duration": 1}, {"media_type": "video", "stream_index": 0,
                                               "duration": 1}]
        with self.assertRaisesRegex(ValueError, "requires a source video presentation origin"):
            compare(source, video_document("1/24", [0, 1], [1, 1], rate="24/1"),
                    "24/1", "cfr_origin")

    def test_output_requires_present_monotonic_dts_and_cfr_durations(self) -> None:
        source = video_document("1/24", [0, 1], [1, 1], rate="24/1")
        missing_dts = video_document("1/24", [0, 1], [1, 1], rate="24/1",
                                     packets=[{"stream_index": 0, "pts": 0, "duration": 1}])
        with self.assertRaisesRegex(ValueError, "must all have decode timestamps"):
            compare(source, missing_dts, "24/1", "cfr_zero")
        off_duration = video_document("1/24", [0, 1], [3, 3], rate="24/1")
        with self.assertRaisesRegex(ValueError, "more than one output tick"):
            compare(source, off_duration, "24/1", "cfr_zero")

    def test_cfr_policy_requires_source_duration_but_retime_can_report_unknown(self) -> None:
        source = video_document("1/1000", [0, 42], [None, None], rate="0/0")
        output = video_document("1/24", [0, 1], [1, 1], rate="24/1")
        with self.assertRaisesRegex(ValueError, "requires known source and output durations"):
            compare(source, output, "24/1", "cfr_zero")
        report = compare(source, output, "24/1", "retimed_unqualified")
        self.assertIsNone(report["source_output_duration_error"])

    def test_rational_outputs_never_use_decimal_floats(self) -> None:
        document = video_document("1/1000", [0, 40], [40, 40], rate="25/1")
        track = analyze(document)["video"][0]
        self.assertEqual(track["time_base"], "1/1000")
        self.assertEqual(track["frame_pts"], ["0/1", "1/25"])
        self.assertEqual(track["duration"], "2/25")
        self.assertEqual(track["time_unit"], "seconds")
        self.assertIsInstance(track["frame_pts"][1], str)

    def test_zero_policy_reports_grid_origin_separately_from_measured_source_origin(self) -> None:
        source = video_document("1/1000", [1, 41], [40, 40], rate="25/1")
        output = video_document("1/25", [0, 1], [1, 1], rate="25/1")
        report = compare(source, output, "25/1", "cfr_zero")
        track = report["video"][0]
        self.assertEqual(track["policy_grid_origin"], "0/1")
        self.assertEqual(track["source_first"], "1/1000")
        self.assertTrue(track["source_origin_preserved"])
        self.assertEqual(track["source_origin_preservation_tolerance"], "1/25")


if __name__ == "__main__":
    unittest.main()
