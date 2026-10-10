#!/usr/bin/env python3
"""Finite 100k production paths; no automatic reliability sealing."""
import argparse
import json
import math
import os
from pathlib import Path
from types import SimpleNamespace

from preflight import ROOT, Run, qualify, save


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--measurement", action="store_true")
    parser.add_argument("--inputs-from", type=Path,
                        help="reuse hash-verified finite sources from an earlier observation run")
    parser.add_argument("--watchdog-seconds", type=float, default=3600)
    args = parser.parse_args()
    if not math.isfinite(args.watchdog_seconds) or args.watchdog_seconds <= 0:
        parser.error("watchdog must be finite and positive")
    # This is the unchanged default allocator, not the abandoned plateau chase.
    if any(key.startswith(("MALLOC_", "GLIBC_TUNABLES", "LD_PRELOAD")) for key in os.environ):
        parser.error("allocator/loader tuning is forbidden for this production campaign")
    run = Run(SimpleNamespace(output=args.output, binary=args.binary,
              device="/dev/dri/renderD128", watchdog_seconds=args.watchdog_seconds,
              proc_sample_interval_seconds=1.0,
              mode="production-100k", stack=None))
    run.environment()
    save(run.out / "protocol.json", {
        "stage": "finite-production-long-run", "frames_per_path": 100000,
        "paths": ["sdr", "pq-preserve", "pq-to-sdr"],
        "measurement": args.measurement, "validation": False,
        "memory_plateau": "Persistent multi-job retention unqualified and outside single-job CLI contract; historical failures unchanged",
        "audio_tracks": {"sdr": 1, "pq-preserve": 0, "pq-to-sdr": 2},
        "scope": "Finite single-job long paths; not persistent hosting or 24x7 qualification",
        "decision": "Path PASS does not automatically qualify long-run or general reliability"})
    for kind in ("sdr", "pq-preserve", "pq-to-sdr"):
        options = {}
        if args.inputs_from:
            options["input_identity"] = json.loads(
                (args.inputs_from / ("source-" + kind) / "identity.json").read_text())
        run.gate(kind, "hardware-long-run", lambda kind=kind: qualify(
            run, kind, 100000, args.measurement, validation=False,
            qualification="LongRunPathEvidenceNotAutomaticSeal",
            audio_tracks={"sdr": 1, "pq-preserve": 0, "pq-to-sdr": 2}[kind], **options), True)
    status = run.finish()
    if status:
        return status
    return 0 if len(run.results) == 3 and all(row["result"] == "PASS" for row in run.results) else 2


if __name__ == "__main__":
    raise SystemExit(main())
