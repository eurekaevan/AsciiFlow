"""Simple frame-indexed long-job observations; no automatic memory verdict."""
import json


def verify_mux_counts(final, frames, audio_packet_counts):
    assert final["frames_processed"] == frames
    assert final["packets_processed"] == frames + sum(audio_packet_counts)


def summarize(rows, process_rows, wall_seconds=None):
    assert rows[0]["phase"] == "initial" and rows[-1]["phase"] == "post-cleanup"
    assert rows[0]["fd_count"] == rows[-1]["fd_count"]
    assert not any(row["accounting_errors"] for row in rows)
    final = rows[-1]
    assert final["resources"] and all(r["active_count"] == 0 for r in final["resources"].values())
    assert all(r.get("active_bytes") in (None, 0) for r in final["resources"].values())
    for queue in final["queues"].values():
        if queue.get("capacity") is not None:
            if queue["peak_depth"] is None:
                # Audio-none never operates the audio channel. Keep its peak
                # unavailable rather than inventing a measured zero.
                assert queue["boundary_observations"] == 0
            else:
                assert queue["peak_depth"] <= queue["capacity"]
    progress = {row["frames_processed"]: row for row in rows if row["phase"] in ("initial", "progress")}
    correlated = [row for row in process_rows if row.get("frames_processed") is not None]
    milestones = {}
    for frame in (0, 10000, 25000, 50000, 75000, 100000):
        row = progress.get(frame)
        if row is None:
            continue
        external = min(correlated, key=lambda value: abs(value["frames_processed"] - frame)) if correlated else None
        milestones[str(frame)] = {"rss_kib": row["rss_kib"], "fd_count": row["fd_count"],
                                  "nearest_external_observation": external}
    throughput = {}
    timed = correlated
    if wall_seconds is not None:
        timed = [{"frames_processed": 0, "elapsed_seconds": 0.0}, *correlated,
                 {"frames_processed": final["frames_processed"], "elapsed_seconds": wall_seconds}]
    for label, start, end in (("first_10k", 0, 10000), ("middle_10k", 45000, 55000), ("last_10k", 90000, 100000)):
        if not timed:
            throughput[label] = None
            continue
        a, b = [min(timed, key=lambda value: abs(value["frames_processed"] - frame)) for frame in (start, end)]
        elapsed = b["elapsed_seconds"] - a["elapsed_seconds"]
        throughput[label] = {"fps": (b["frames_processed"] - a["frames_processed"]) / elapsed if elapsed > 0 else None,
                             "observed_start_frame": a["frames_processed"], "observed_end_frame": b["frames_processed"],
                             "elapsed_seconds": elapsed}
    return {"milestones": milestones, "rss_peak_kib": max(r["rss_kib"] for r in rows),
            "rss_final_kib": final["rss_kib"],
            "fd": {"before": rows[0]["fd_count"], "peak": max(r["fd_count"] for r in rows), "after": final["fd_count"]},
            "resources_after_cleanup": final["resources"], "queue_observations": final["queues"], "mux": final["mux"],
            "throughput": throughput, "memory_verdict": "Requires media-length trend review; no arbitrary MiB gate",
            "limitations": "External PSS/Anonymous/time and flushed 1000-frame observations are correlated, not atomic; throughput endpoints include process startup/finalization when wall duration is supplied; queue observations are not exact high-water census"}


def from_files(resources, process_samples, wall_seconds=None):
    return summarize([json.loads(line) for line in resources.read_text().splitlines()],
                     [json.loads(line) for line in process_samples.read_text().splitlines()], wall_seconds)
