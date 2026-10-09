#!/usr/bin/env python3
"""Conservatively summarize resource telemetry from JSONL soak samples."""
import argparse
import json
import math
from pathlib import Path
import statistics
import sys
import re


MIN_PROGRESS_POINTS = 9  # three non-overlapping windows, each with at least 3 points
WINDOW_COUNT = 3
MIN_WINDOW_POINTS = 3


def _number(value):
    return isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(value)


def _counter(value):
    return isinstance(value, int) and not isinstance(value, bool) and value >= 0


def _percentile95(values):
    ordered = sorted(values)
    return ordered[max(0, math.ceil(0.95 * len(ordered)) - 1)]


def _slope(points):
    """Least-squares metric units per processed frame."""
    if len(points) < 2:
        return None
    xs = [point[0] for point in points]
    ys = [point[1] for point in points]
    mean_x, mean_y = statistics.fmean(xs), statistics.fmean(ys)
    denominator = sum((x - mean_x) ** 2 for x in xs)
    return sum((x - mean_x) * (y - mean_y) for x, y in points) / denominator if denominator else None


def _extract(sample, errors, index):
    if not isinstance(sample, dict):
        errors.append(f"sample {index}: expected an object")
        return None
    phase = sample.get("phase")
    if not isinstance(phase, str) or not phase.strip():
        errors.append(f"sample {index}: phase must be a non-empty string")
        phase = "unknown"
    phase = re.sub(r"[^a-z]+", "-", phase.strip().lower().replace("_", "-" )).strip("-")
    frames = sample.get("frames_processed")
    if not _counter(frames):
        errors.append(f"sample {index}: frames_processed must be a non-negative integer")
        return None
    values = {"rss_kib": sample.get("rss_kib"), "fd_count": sample.get("fd_count")}
    for key, value in values.items():
        if value is not None and not _counter(value):
            errors.append(f"sample {index}: {key} must be a non-negative integer or null")
            values[key] = None

    resources = sample.get("resources", {})
    queues = sample.get("queues", {})
    if not isinstance(resources, dict) or not isinstance(queues, dict):
        errors.append(f"sample {index}: resources and queues must be objects")
        resources, queues = {}, {}
    for name, resource in resources.items():
        if not isinstance(resource, dict):
            errors.append(f"sample {index}: resource {name!r} must be an object")
            continue
        for field in ("active_count", "active_bytes", "peak_count", "peak_bytes"):
            value = resource.get(field)
            if value is not None and not _counter(value):
                errors.append(f"sample {index}: resources.{name}.{field} must be a non-negative integer or null")
        for active, peak in (("active_count", "peak_count"), ("active_bytes", "peak_bytes")):
            if _counter(resource.get(active)) and _counter(resource.get(peak)) and resource[peak] < resource[active]:
                errors.append(f"sample {index}: resources.{name}.{peak} is below {active}")
        for field in ("active_count", "active_bytes"):
            values[f"resources.{name}.{field}"] = resource.get(field)
    for name, queue in queues.items():
        if not isinstance(queue, dict):
            errors.append(f"sample {index}: queue {name!r} must be an object")
            continue
        depth, capacity, peak = (queue.get(key) for key in ("depth", "capacity", "peak_depth"))
        for key, value in (("depth", depth), ("capacity", capacity), ("peak_depth", peak)):
            if value is not None and not _counter(value):
                errors.append(f"sample {index}: queues.{name}.{key} must be a non-negative integer or null")
        if _counter(depth) and _counter(capacity) and depth > capacity:
            errors.append(f"sample {index}: queues.{name}.depth exceeds capacity")
        if _counter(depth) and _counter(peak) and peak < depth:
            errors.append(f"sample {index}: queues.{name}.peak_depth is below depth")
        values[f"queues.{name}.depth"] = depth
    accounting = sample.get("accounting_errors", [])
    if not isinstance(accounting, list):
        errors.append(f"sample {index}: accounting_errors must be an array")
    elif accounting:
        errors.extend(f"sample {index}: accounting error: {item}" for item in accounting)
    return phase, frames, values, sample.get("expected_cache_growth", {})


def _windows(points):
    size = len(points) // WINDOW_COUNT
    return [points[i * size:(i + 1) * size] if i < WINDOW_COUNT - 1
            else points[i * size:] for i in range(WINDOW_COUNT)]


def analyze(samples):
    """Return a JSON-serializable report. Missing fields stay unavailable."""
    errors = []
    grouped = {}
    boundary = {"initial": [], "pre-finalization": [], "post-cleanup": []}
    all_observations = []
    attributions = {}
    for index, sample in enumerate(samples, 1):
        extracted = _extract(sample, errors, index)
        if extracted is None:
            continue
        phase, frames, values, attribution = extracted
        observation = {"phase": phase, "frames": frames, "values": values, "index": index}
        all_observations.append(observation)
        if phase in boundary:
            boundary[phase].append(observation)
        # Cleanup measurements have a different lifecycle meaning and must not
        # contaminate the runtime trend at an identical processed-frame count.
        if phase not in ("initial", "pre-finalization", "post-cleanup"):
            grouped.setdefault(frames, {}).setdefault(phase, []).append(values)
        if isinstance(attribution, dict):
            attributions.update(attribution)
        elif attribution:
            errors.append(f"sample {index}: expected_cache_growth must be an object")

    points = []
    for frames in sorted(grouped):
        phase_rows = grouped[frames]
        # At a repeated progress count, prefer a runtime progress sample over
        # the post-init baseline. Repeated rows within the selected phase are
        # combined field-by-field using the median.
        preferred = "progress" if "progress" in phase_rows else sorted(phase_rows)[-1]
        rows = phase_rows[preferred]
        names = set().union(*(row.keys() for row in rows))
        merged = {}
        for name in names:
            known = [row[name] for row in rows if name in row and _number(row[name])]
            merged[name] = statistics.median(known) if known else None
        points.append((frames, merged))

    progress = len(points)
    windows = _windows(points) if progress >= MIN_PROGRESS_POINTS else []
    names = sorted(set().union(*(observation["values"].keys() for observation in all_observations))) if all_observations else []
    metrics = {}
    growth_evidence = []
    for name in names:
        series = [(frame, values[name]) for frame, values in points if _number(values.get(name))]
        values = [value for _, value in series]
        initial_observation = (boundary["initial"][-1] if boundary["initial"] else None)
        if initial_observation is None:
            initial_observation = next((item for item in all_observations if item["phase"] == "post-init"), None)
        final_observation = (boundary["post-cleanup"][-1] if boundary["post-cleanup"] else
                             boundary["pre-finalization"][-1] if boundary["pre-finalization"] else None)
        initial_value = initial_observation["values"].get(name) if initial_observation else (values[0] if values else None)
        final_value = final_observation["values"].get(name) if final_observation else (values[-1] if values else None)
        available_values = [item["values"].get(name) for item in all_observations]
        whole_run_values = [value for value in available_values if _number(value)]
        if not values and not whole_run_values:
            metrics[name] = {"available": False, "initial": initial_value, "final": final_value}
            continue
        steady = [(frame, row[name]) for frame, row in windows[-1] if _number(row.get(name))] if windows else []
        window_series = [[(frame, row[name]) for frame, row in window if _number(row.get(name))]
                         for window in windows]
        slopes = [_slope(window) if len(window) >= 2 else None for window in window_series]
        medians = [statistics.median(value for _, value in window) if window else None
                   for window in window_series]
        floor = 2.0 if name == "fd_count" else (1.0 if name.endswith(".active_count") or name.endswith(".depth") else 0.0)
        relative_floor = max(floor, abs(medians[0]) * 0.05) if medians and medians[0] is not None else floor
        sufficiently_sampled = (len(window_series) == WINDOW_COUNT
                                and all(len(window) >= MIN_WINDOW_POINTS for window in window_series))
        persistent = (sufficiently_sampled and len(medians) == 3 and all(value is not None for value in medians)
                      and medians[1] > medians[0] and medians[2] > medians[1]
                      and medians[2] - medians[0] > relative_floor
                      and all(slope is not None and slope > 0 for slope in slopes))
        attribution = attributions.get(name)
        evidence = {"metric": name, "window_medians": medians,
                    "window_slopes_per_frame": slopes,
                    "persistent_growth": persistent,
                    "expected_cache_attribution": attribution if isinstance(attribution, str) else None}
        if persistent and not isinstance(attribution, str):
            growth_evidence.append(evidence)
        metrics[name] = {
            "available": True,
            "initial": initial_value,
            "warmup_peak": max((value for _, value in window_series[0]), default=None) if window_series else None,
            "whole_run_peak": max(whole_run_values) if whole_run_values else None,
            "steady_median": statistics.median(value for _, value in steady) if steady else None,
            "steady_p95": _percentile95([value for _, value in steady]) if steady else None,
            "final": final_value,
            "trend_observations": len(series),
            "window_observations": [len(window) for window in window_series],
            "trend_data_sufficient": sufficiently_sampled,
            "early_middle_late_slopes_per_frame": slopes,
            "windows": medians,
            "expected_cache_attribution": attribution if isinstance(attribution, str) else None,
        }

    insufficient_metrics = [name for name, metric in metrics.items()
                            if metric.get("available") and not metric.get("trend_data_sufficient")]
    if progress < MIN_PROGRESS_POINTS:
        classification = "Unresolved"
        reason = f"requires at least {MIN_PROGRESS_POINTS} unique progress points for three windows of at least {MIN_WINDOW_POINTS}"
    elif errors:
        classification = "Unresolved"
        reason = "telemetry contains schema, counter-invariant, or accounting errors"
    elif insufficient_metrics:
        classification = "Unresolved"
        reason = "insufficient runtime trend samples in one or more monitored metrics: " + ", ".join(insufficient_metrics)
    elif growth_evidence:
        classification = "LeakSuspected"
        reason = "one or more unattributed metrics rise across all three windows beyond the conservative noise floor"
    else:
        observed = [metric for metric in metrics.values() if metric.get("available")]
        if not observed:
            classification, reason = "Unresolved", "no recognized resource metrics were available"
        else:
            stable_late = []
            warmup = False
            insufficient = []
            for name, metric in metrics.items():
                medians = metric.get("windows", [])
                if not metric.get("available"):
                    continue
                if not metric.get("trend_data_sufficient"):
                    insufficient.append(name)
                    continue
                if len(medians) != 3 or any(value is None for value in medians):
                    insufficient.append(name)
                    continue
                floor = 2.0 if name == "fd_count" else (1.0 if name.endswith(".active_count") or name.endswith(".depth") else 0.0)
                tolerance = max(floor, abs(medians[0]) * 0.05)
                late_is_stable = abs(medians[2] - medians[1]) <= tolerance
                stable_late.append(late_is_stable)
                warmup |= late_is_stable and abs(medians[1] - medians[0]) > tolerance
            if insufficient:
                classification, reason = "Unresolved", "insufficient runtime trend samples in one or more monitored metrics: " + ", ".join(insufficient)
            elif not stable_late or not all(stable_late):
                classification, reason = "Unresolved", "window variation is neither stable nor persistent enough to classify"
            else:
                classification = "WarmupThenStable" if warmup else "Stable"
                reason = "no unattributed metric meets the persistent-growth rule and all late windows are stable"

    return {
        "classification": classification,
        "reason": reason,
        "qualification": "ShortStabilityOnly" if classification in ("Stable", "WarmupThenStable") else "Unresolved",
        "resource_growth_is_not_automatic_leak": True,
        "classification_rules": {
            "minimum_unique_progress_points": MIN_PROGRESS_POINTS,
            "windows": WINDOW_COUNT,
            "minimum_points_per_window": MIN_WINDOW_POINTS,
            "persistent_growth": "all three window medians strictly increase, all three least-squares slopes are positive, and late-minus-early exceeds max(metric floor, 5% of early median)",
            "metric_floors": {"fd_count": 2, "active_count_or_queue_depth": 1, "other_metrics": 0},
            "stable_late": "absolute middle-to-late median difference is at most max(metric floor, 5% of early median)",
            "leak_label": "LeakSuspected only; trend is not proof of a leak",
            "rss_growth_alone": "must satisfy persistent-growth rule and is not an automatic leak",
        },
        "unique_progress_points": progress,
        "phase_sample_counts": {phase: sum(item["phase"] == phase for item in all_observations)
                                for phase in ("initial", "post-init", "progress", "pre-finalization", "post-cleanup")},
        "windows": {"count": WINDOW_COUNT, "minimum_points_each": MIN_WINDOW_POINTS,
                    "ranges_frames": [[window[0][0], window[-1][0]] for window in windows]},
        "metrics": metrics,
        "growth_evidence": growth_evidence,
        "accounting_and_invariant_errors": errors,
        "unknown_fields": "ignored; absent or null known values remain unavailable, never zero",
        "expected_cache_growth_attributions": attributions,
    }


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True, help="JSONL telemetry samples")
    parser.add_argument("--output", type=Path, required=True, help="new JSON report path; existing files are never overwritten")
    args = parser.parse_args(argv)
    try:
        samples = []
        with args.input.open(encoding="utf-8") as source:
            for line_number, line in enumerate(source, 1):
                if not line.strip():
                    continue
                try:
                    samples.append(json.loads(line))
                except json.JSONDecodeError as error:
                    raise ValueError(f"invalid JSON at line {line_number}: {error.msg}") from error
        report = analyze(samples)
        with args.output.open("x", encoding="utf-8") as output:
            json.dump(report, output, indent=2, sort_keys=True)
            output.write("\n")
    except (OSError, ValueError) as error:
        parser.error(str(error))
    print(f"{report['classification']}: {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
