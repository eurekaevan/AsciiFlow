#!/usr/bin/env python3
"""Low-cost native mux qualification evidence; never changes media oracles."""
import argparse
import base64
from collections import Counter
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "corpus"))
from portability import oracle_tiers


def sha(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def inspect(path):
    probe = json.loads(subprocess.check_output([
        "ffprobe", "-v", "error", "-show_packets", "-show_streams",
        "-show_data_hash", "sha256", "-of", "json", str(path)], text=True))
    fields = ("stream_index", "pts", "dts", "duration", "flags", "size", "data_hash")
    packets = [{key: packet.get(key) for key in fields} for packet in probe["packets"]]
    local = {str(stream["index"]): [p for p in packets if p["stream_index"] == stream["index"]]
             for stream in probe["streams"]}
    for index, records in local.items():
        assert all(b["dts"] >= a["dts"] for a, b in zip(records, records[1:])), index
    encoded = json.dumps(packets, sort_keys=True, separators=(",", ":")).encode()
    return {"path": str(path), "bytes": path.stat().st_size, "sha256": sha(path),
            "packet_order_sha256": hashlib.sha256(encoded).hexdigest(),
            "packet_count": len(packets), "packets": packets, "per_stream": local,
            "stream_sequence": [p["stream_index"] for p in packets],
            "stream_time_bases": {str(s["index"]): s["time_base"] for s in probe["streams"]}}


def summarize(records):
    reference = records[0]
    assert all(r["per_stream"] == reference["per_stream"] for r in records), "upstream packet content/timing changed"
    variants = Counter(r["packet_order_sha256"] for r in records)
    divergence = {}
    for r in records:
        indices = [i for i, (a, b) in enumerate(zip(reference["packets"], r["packets"])) if a != b]
        if indices:
            divergence[r["packet_order_sha256"]] = {"first": min(indices), "last": max(indices)}
    return {"runs": len(records), "variant_counts": dict(variants),
            "divergent_regions_against_first": divergence, "per_stream_identical": True}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("operation", choices=("production", "replay-summary", "strict-oracles", "materialize-captures"))
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--runs", type=int, default=30)
    parser.add_argument("--directory", type=Path)
    args = parser.parse_args()
    if args.operation == "materialize-captures":
        args.output.mkdir(parents=True, exist_ok=False)
        expected = {"variant-a": "c33ac221eeb792790c2113bb9674586a05711672baa04177676750ec97b7adfa",
                    "variant-b": "32ea1a2fcb11e2ac328ed726bfc10536b0032ca18078b21fa61da962dcfeafda"}
        for name, identity in expected.items():
            source = Path(__file__).resolve().parent / "mux-captures" / (name + ".mp4.base64")
            data = base64.b64decode("".join(source.read_text().split()), validate=True)
            assert hashlib.sha256(data).hexdigest() == identity, name
            (args.output / (name + ".mp4")).write_bytes(data)
        print("Historical native packet/codec-parameter captures materialized; both SHA-256 identities verified")
    elif args.operation == "production":
        if not args.binary or args.runs < 1:
            parser.error("production requires --binary and positive --runs")
        args.output.mkdir(parents=True, exist_ok=False)
        baseline = json.loads(Path("tests/portability/baseline.json").read_text())
        original = next(c["argv"] for c in baseline["runs"]["canonical-verified"]["commands"]
                        if c["id"] == "runtime-real-aac-44100-mono-mp4")
        records = []
        for i in range(args.runs):
            output = args.output / f"run-{i:03}.mp4"
            trace = args.output / f"run-{i:03}-trace.jsonl"
            argv = list(original)
            argv[0], argv[2] = str(args.binary.resolve()), str(output.resolve())
            argv[argv.index("--diagnostic-report") + 1] = str((args.output / f"run-{i:03}-runtime.json").resolve())
            env = dict(os.environ, ASCIIFLOW_MUX_TRACE=str(trace.resolve()))
            result = subprocess.run(argv, env=env, capture_output=True, text=True, timeout=120)
            (args.output / f"run-{i:03}.log").write_text(result.stdout + result.stderr)
            if result.returncode:
                raise RuntimeError(f"run {i}: {result.returncode}: {result.stderr}")
            record = inspect(output)
            record.update(argv=argv, trace={"path": str(trace), "sha256": sha(trace)})
            records.append(record)
        document = {"binary": {"path": str(args.binary), "sha256": sha(args.binary)},
                    "summary": summarize(records), "records": records}
        (args.output / "results.json").write_text(json.dumps(document, indent=2) + "\n")
        print(json.dumps(document["summary"], indent=2))
    elif args.operation == "replay-summary":
        if not args.directory:
            parser.error("replay-summary requires --directory")
        records = [inspect(p) for p in sorted(args.directory.glob("*.mp4")) if not p.name.startswith("stress-")]
        fixed = [r for r in records if Path(r["path"]).name.startswith("fixed-")]
        document = {"fixed": summarize(fixed), "all_patterns": summarize(records), "records": records,
                    "stress": [inspect(p) for p in sorted(args.directory.glob("stress-*.mp4"))]}
        with args.output.open("x") as stream:
            json.dump(document, stream, indent=2)
        print(json.dumps({k: v for k, v in document.items() if k not in {"records", "stress"}}, indent=2))
    else:
        if not args.directory:
            parser.error("strict-oracles requires --directory")
        paths = sorted(args.directory.glob("fixed-*.mp4")) or sorted(args.directory.glob("run-*.mp4"))
        if len(paths) < 2:
            parser.error("strict-oracles requires at least two output files")
        command = ["cargo", "test", "--release", "-p", "asciiflow-media", "--test", "media_regression",
                   "compare_portability_pair_from_env", "--", "--ignored", "--exact", "--nocapture"]
        records = []
        for candidate in paths:
            env = dict(os.environ, ASCIIFLOW_REGRESSION_REFERENCE=str(paths[0].resolve()),
                       ASCIIFLOW_REGRESSION_CANDIDATE=str(candidate.resolve()), ASCIIFLOW_REGRESSION_EXACT_BUILD="attested")
            result = subprocess.run(command, env=env, capture_output=True, text=True, timeout=120)
            log = args.directory / (candidate.stem + "-strict-oracle.log")
            log.write_text(result.stdout + result.stderr)
            tiers = oracle_tiers(log.read_text())
            records.append({"reference": str(paths[0]), "candidate": str(candidate), "argv": command,
                            "exit_code": result.returncode, "tiers": tiers, "log_sha256": sha(log)})
            assert result.returncode == 0 and len(tiers) == 5, log
        with args.output.open("x") as stream:
            json.dump({"runs": len(records), "records": records}, stream, indent=2)
        print(f"{len(records)} strict same-stack oracles PASS")


if __name__ == "__main__":
    main()
