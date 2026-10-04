"""Exact-rational ffprobe timing analysis and CFR comparison helpers."""

from __future__ import annotations

from fractions import Fraction
from typing import Any


def _fraction(value: Any) -> Fraction | None:
    if value is None or value == "N/A":
        return None
    try:
        return Fraction(str(value))
    except (ValueError, ZeroDivisionError):
        return None


def _text(value: Fraction | None) -> str | None:
    return None if value is None else f"{value.numerator}/{value.denominator}"


def _tick_value(value: Any, time_base: Fraction | None) -> Fraction | None:
    if value is None or time_base is None:
        return None
    try:
        return int(value) * time_base
    except (TypeError, ValueError, OverflowError):
        return None


def _entries(document: dict[str, Any], plural: str, kind: str) -> list[dict[str, Any]]:
    direct = document.get(plural)
    if isinstance(direct, list):
        return [entry for entry in direct if isinstance(entry, dict)]
    combined = document.get("packets_and_frames", [])
    return [entry for entry in combined
            if isinstance(entry, dict) and entry.get("type") == kind]


def _stream_rate(stream: dict[str, Any]) -> Fraction | None:
    for name in ("avg_frame_rate", "r_frame_rate"):
        rate = _fraction(stream.get(name))
        if rate is not None and rate > 0:
            return rate
    return None


def analyze(document: dict[str, Any]) -> dict[str, list[dict[str, Any]]]:
    """Analyze ffprobe JSON, returning per-track times as exact ``n/d`` strings.

    Supports ``-show_frames -show_packets`` output (``packets_and_frames``)
    and JSON documents with separate ``frames`` and ``packets`` arrays.
    Missing frame timestamps/durations remain null. Only the final video frame
    may receive a derived duration, and only when ffprobe reports a usable rate.
    """
    streams = document.get("streams")
    if not isinstance(streams, list):
        raise ValueError("ffprobe document must contain a streams array")
    frames = _entries(document, "frames", "frame")
    packets = _entries(document, "packets", "packet")
    result: dict[str, list[dict[str, Any]]] = {"video": [], "audio": []}

    for stream in streams:
        if not isinstance(stream, dict) or stream.get("codec_type") not in result:
            continue
        try:
            index = int(stream["index"])
        except (KeyError, TypeError, ValueError):
            raise ValueError("each audio/video stream must have an integer index")
        time_base = _fraction(stream.get("time_base"))
        if time_base is None or time_base <= 0:
            raise ValueError(f"stream {index} has no valid time_base")
        kind = stream["codec_type"]
        stream_frames = [f for f in frames
                         if f.get("stream_index", index) == index and
                         f.get("media_type", kind) == kind] if kind == "video" else []
        stream_packets = [p for p in packets if p.get("stream_index") == index]

        frame_pts = [_tick_value(f.get("pts", f.get("best_effort_timestamp")), time_base)
                     for f in stream_frames]
        frame_durations = [_tick_value(f.get("duration", f.get("pkt_duration")), time_base)
                           for f in stream_frames]
        derived_last_duration = False
        if kind == "video" and frame_durations and frame_durations[-1] is None:
            rate = _stream_rate(stream)
            if rate is not None:
                frame_durations[-1] = 1 / rate
                derived_last_duration = True

        packet_pts = [_tick_value(p.get("pts"), time_base) for p in stream_packets]
        dts = [_tick_value(p.get("dts"), time_base) for p in stream_packets]
        durations = [_tick_value(p.get("duration"), time_base) for p in stream_packets]
        present_pts = [pts for pts in frame_pts if pts is not None]
        present_frame_ends = [pts + duration for pts, duration in zip(frame_pts, frame_durations)
                              if pts is not None and duration is not None]
        first = min(present_pts) if present_pts else None
        last = max(present_pts) if present_pts else None
        presentation_end = max(present_frame_ends) if present_frame_ends else None
        if kind == "audio":
            packet_starts = [pts for pts in packet_pts if pts is not None]
            packet_ends = [pts + duration for pts, duration in zip(packet_pts, durations)
                           if pts is not None and duration is not None]
            first = min(packet_starts) if packet_starts else None
            last = max(packet_starts) if packet_starts else None
            presentation_end = max(packet_ends) if packet_ends else None
        track = {
            "index": index,
            "time_unit": "seconds",
            "time_base": _text(time_base),
            "frame_pts": [_text(value) for value in frame_pts],
            "frame_durations": [_text(value) for value in frame_durations],
            "packet_pts": [_text(value) for value in packet_pts],
            "dts": [_text(value) for value in dts],
            "durations": [_text(value) for value in durations],
            "first": _text(first),
            "last": _text(last),
            "presentation_end": _text(presentation_end),
            "duration": _text(presentation_end - first if first is not None and presentation_end is not None else None),
            "last_frame_duration_derived": derived_last_duration,
        }
        result[kind].append(track)
    return result


def _values(track: dict[str, Any], field: str) -> list[Fraction | None]:
    return [_fraction(value) for value in track[field]]


def _strictly_increasing(values: list[Fraction | None]) -> bool:
    present = [value for value in values if value is not None]
    return all(left < right for left, right in zip(present, present[1:]))


def _paired_tracks(source: list[dict[str, Any]], output: list[dict[str, Any]], kind: str):
    if len(source) != len(output):
        raise ValueError(f"{kind} track count changed: {len(source)} to {len(output)}")
    return zip(sorted(source, key=lambda track: track["index"]),
               sorted(output, key=lambda track: track["index"]))


def compare(source: dict[str, Any], output: dict[str, Any], rate: Any,
            policy: str) -> dict[str, Any]:
    """Compare source/output timing under one explicit CFR policy.

    ``cfr_zero`` maps output frame *i* to ``i/rate``. ``cfr_origin`` retains
    the source video's first presentation timestamp. ``retimed_unqualified``
    validates only the zero-based CFR output and reports the source/output
    duration change without claiming source timing preservation.
    Raises ``ValueError`` when a required timing contract is not met.
    """
    if policy not in {"cfr_zero", "cfr_origin", "retimed_unqualified"}:
        raise ValueError(f"unknown timing policy: {policy}")
    target_rate = _fraction(rate)
    if target_rate is None or target_rate <= 0:
        raise ValueError(f"invalid CFR rate: {rate!r}")
    src = analyze(source)
    dst = analyze(output)
    if not src["video"] or not dst["video"]:
        raise ValueError("source and output must each contain a video track")

    video_reports: list[dict[str, Any]] = []
    for source_track, output_track in _paired_tracks(src["video"], dst["video"], "video"):
        source_pts = _values(source_track, "frame_pts")
        output_pts = _values(output_track, "frame_pts")
        if len(source_pts) != len(output_pts):
            raise ValueError(f"video frame count changed: {len(source_pts)} to {len(output_pts)}")
        if not output_pts or any(value is None for value in output_pts):
            raise ValueError("output video frames must all have presentation timestamps")
        if not _strictly_increasing(output_pts):
            raise ValueError("output video frame presentation timestamps are not strictly increasing")
        out_tick = _fraction(output_track["time_base"])
        src_tick = _fraction(source_track["time_base"])
        assert out_tick is not None and src_tick is not None
        source_origin = _fraction(source_track["first"])
        if policy == "cfr_origin" and source_origin is None:
            raise ValueError("cfr_origin requires a source video presentation origin")
        copy_origin = policy == "cfr_origin"
        origin = source_origin if copy_origin else Fraction(0)
        tolerance = out_tick + (src_tick if copy_origin else 0)
        errors: list[Fraction] = []
        for index, actual in enumerate(output_pts):
            assert actual is not None
            expected = origin + Fraction(index, 1) / target_rate
            error = abs(actual - expected)
            errors.append(error)
            if error > tolerance:
                raise ValueError(f"output frame {index} is off CFR grid by {_text(error)} (limit {_text(tolerance)})")

        output_dts = _values(output_track, "dts")
        if not output_dts or any(value is None for value in output_dts):
            raise ValueError("output video packets must all have decode timestamps")
        packet_dts_monotonic = _strictly_increasing(output_dts)
        if not packet_dts_monotonic:
            raise ValueError("output packet DTS values are not strictly increasing")
        packet_pts = _values(output_track, "packet_pts")
        reorder_observed = any(pts is not None and dts is not None and pts != dts
                               for pts, dts in zip(packet_pts, output_dts))

        source_duration = _fraction(source_track["duration"])
        output_duration = _fraction(output_track["duration"])
        duration_error = (abs(output_duration - source_duration)
                          if output_duration is not None and source_duration is not None else None)
        source_durations = _values(source_track, "frame_durations")
        output_durations = _values(output_track, "frame_durations")
        if any(duration is None or duration <= 0 for duration in output_durations):
            raise ValueError("output video frame durations must all be present and positive")
        duration_errors = [abs(actual - Fraction(1, 1) / target_rate)
                           for actual in output_durations if actual is not None]
        if any(error > out_tick for error in duration_errors):
            raise ValueError(f"output frame duration differs from CFR duration by more than one output tick "
                             f"(limit {_text(out_tick)})")
        duration_tolerance = out_tick + src_tick
        if source_track["last_frame_duration_derived"] and source_durations and source_durations[-1] is not None:
            duration_tolerance += source_durations[-1]
        if policy != "retimed_unqualified" and (source_duration is None or output_duration is None):
            raise ValueError("CFR presentation-duration comparison requires known source and output durations")
        if policy != "retimed_unqualified" and duration_error is not None and duration_error > duration_tolerance:
            raise ValueError(f"source/output presentation duration changed by {_text(duration_error)} "
                             f"(limit {_text(duration_tolerance)})")
        duration_preserved = (policy != "retimed_unqualified" and duration_error is not None and
                              duration_error <= duration_tolerance)
        origin_tolerance = out_tick + (src_tick if copy_origin else 0)
        output_origin = _fraction(output_track["first"])
        source_origin_preserved = (
            source_origin is not None and output_origin is not None and
            abs(output_origin - source_origin) <= origin_tolerance
        )

        video_reports.append({
            "source_index": source_track["index"], "output_index": output_track["index"],
            "frame_count": len(output_pts), "expected_origin": _text(origin),
            "policy_grid_origin": _text(origin),
            "max_frame_pts_error": _text(max(errors)), "frame_pts_tolerance": _text(tolerance),
            "max_frame_duration_error": _text(max(duration_errors) if duration_errors else None),
            "frame_duration_tolerance": _text(duration_tolerance),
            "source_duration": _text(source_duration), "output_duration": _text(output_duration),
            "source_output_duration_error": _text(duration_error),
            "source_duration_tolerance": _text(duration_tolerance),
            "source_duration_preserved": duration_preserved,
            "source_origin_preserved": source_origin_preserved,
            "source_origin_preservation_tolerance": _text(origin_tolerance),
            "source_first": source_track["first"], "source_presentation_end": source_track["presentation_end"],
            "output_first": output_track["first"], "output_presentation_end": output_track["presentation_end"],
            "packet_dts_monotonic": packet_dts_monotonic,
            "packet_pts_dts_reordering_observed": reorder_observed,
            "negative_packet_dts_observed": any(value is not None and value < 0 for value in output_dts),
        })

    audio_reports: list[dict[str, Any]] = []
    source_primary_video, output_primary_video = src["video"][0], dst["video"][0]
    source_video_tick = _fraction(source_primary_video["time_base"])
    output_video_tick = _fraction(output_primary_video["time_base"])
    assert source_video_tick is not None and output_video_tick is not None
    for source_track, output_track in _paired_tracks(src["audio"], dst["audio"], "audio"):
        src_tick = _fraction(source_track["time_base"])
        out_tick = _fraction(output_track["time_base"])
        assert src_tick is not None and out_tick is not None
        tolerance = src_tick + out_tick
        first_delta = _optional_delta(source_track["first"], output_track["first"])
        end_delta = _optional_delta(source_track["presentation_end"], output_track["presentation_end"])
        for label, delta in (("first", first_delta), ("end", end_delta)):
            if delta is None:
                raise ValueError(f"audio {label} timing is unavailable for a matched track")
            if delta > tolerance:
                raise ValueError(f"audio {label} timing changed by {_text(delta)} (limit {_text(tolerance)})")
        source_end = _fraction(source_track["presentation_end"])
        source_video_end = _fraction(source_primary_video["presentation_end"])
        output_end = _fraction(output_track["presentation_end"])
        output_video_end = _fraction(output_primary_video["presentation_end"])
        if any(value is None for value in (source_end, source_video_end, output_end, output_video_end)):
            raise ValueError("audio/video presentation ends are required for A/V end-delta comparison")
        assert source_end is not None and source_video_end is not None
        assert output_end is not None and output_video_end is not None
        source_av_end_delta = source_end - source_video_end
        output_av_end_delta = output_end - output_video_end
        av_end_delta_error = abs(output_av_end_delta - source_av_end_delta)
        av_tolerance = src_tick + source_video_tick + out_tick + output_video_tick
        if av_end_delta_error > av_tolerance:
            raise ValueError(f"audio/video end delta changed by {_text(av_end_delta_error)} "
                             f"(limit {_text(av_tolerance)})")
        audio_reports.append({
            "source_index": source_track["index"], "output_index": output_track["index"],
            "source_first": source_track["first"], "output_first": output_track["first"],
            "first_delta": _text(first_delta),
            "source_presentation_end": source_track["presentation_end"],
            "output_presentation_end": output_track["presentation_end"],
            "end_delta": _text(end_delta), "tolerance": _text(tolerance),
            "source_audio_video_end_delta": _text(source_av_end_delta),
            "output_audio_video_end_delta": _text(output_av_end_delta),
            "audio_video_end_delta_error": _text(av_end_delta_error),
            "audio_video_end_delta_tolerance": _text(av_tolerance),
        })

    source_video_duration = _fraction(src["video"][0]["duration"])
    output_video_duration = _fraction(dst["video"][0]["duration"])
    source_duration_preserved = all(report["source_duration_preserved"] for report in video_reports)
    source_origin_preserved = all(report["source_origin_preserved"] for report in video_reports)
    return {
        "policy": policy, "rate": _text(target_rate), "time_unit": "seconds", "video": video_reports,
        "audio": audio_reports,
        "source_output_duration_error": _text(abs(output_video_duration - source_video_duration)
                                              if output_video_duration is not None and source_video_duration is not None else None),
        "source_duration_preserved": source_duration_preserved,
        "source_origin_preserved": source_origin_preserved,
    }


def _optional_delta(left: Any, right: Any) -> Fraction | None:
    a, b = _fraction(left), _fraction(right)
    return abs(a - b) if a is not None and b is not None else None
