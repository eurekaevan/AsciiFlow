#!/usr/bin/env python3
"""Actual SPS/VUI mutation negatives for the additive H.264 driver contract."""
import argparse
import json
from pathlib import Path
import subprocess
import sys

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tests/corpus"))
from portability import digest, source_identity
from stack_environment import environment


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", required=True, type=Path)
    parser.add_argument("--candidate", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    reference = args.reference.resolve(strict=True)
    candidate = args.candidate.resolve(strict=True)
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    source = source_identity()
    env = environment()
    commands = []

    def command(label, argv, selected_env=env):
        result = subprocess.run([str(part) for part in argv], cwd=ROOT,
                                env=selected_env, capture_output=True, text=True,
                                timeout=600, check=False)
        log = output / f"{label}.log"
        with log.open("x") as stream:
            stream.write(result.stdout + result.stderr)
        record = {"argv": [str(part) for part in argv], "exit_code": result.returncode,
                  "log": str(log), "log_sha256": digest(log)}
        commands.append(record)
        return result

    version = command("tool-version", ["ffmpeg", "-version"])
    assert version.returncode == 0 and version.stdout.startswith("ffmpeg version 8.1.3")
    records = []
    for name, change in (("primaries", "colour_primaries=9"),
                         ("transfer", "transfer_characteristics=16"),
                         ("matrix", "matrix_coefficients=9"),
                         ("range", "video_full_range_flag=1"),
                         ("timing", "tick_rate=200")):
        mutated = output / f"{name}.mp4"
        result = command(name + "-generate", ["ffmpeg", "-nostdin", "-v", "error", "-n",
            "-i", candidate, "-map", "0:v:0", "-c:v", "copy", "-bsf:v", "h264_metadata=" + change,
            "-an", mutated])
        assert result.returncode == 0, f"{name}: generation failed"
        probe = command(name + "-syntax", ["ffmpeg", "-nostdin", "-loglevel", "info", "-i", mutated,
            "-map", "0:v:0", "-c:v", "copy", "-bsf:v", "h264_mp4toannexb,trace_headers", "-f", "null", "-"])
        assert probe.returncode == 0, f"{name}: actual syntax parse failed"
        selected = dict(env, ASCIIFLOW_REGRESSION_REFERENCE=str(reference),
                        ASCIIFLOW_REGRESSION_CANDIDATE=str(mutated),
                        ASCIIFLOW_PORTABILITY_REFERENCE_IHD_VERSION="26.1.5",
                        ASCIIFLOW_PORTABILITY_CANDIDATE_IHD_VERSION="25.4.6")
        oracle = command(name + "-oracle", ["cargo", "test", "--offline", "--release", "-p", "asciiflow-media",
            "--test", "h264_driver_portability", "compare_h264_driver_pair_from_env", "--", "--ignored", "--exact", "--nocapture"], selected)
        log = oracle.stdout + oracle.stderr
        assert oracle.returncode != 0 and "Tier 1B-P cross-driver portability: FAIL" in log, \
            f"{name}: mutation was not rejected by the actual portability gate"
        records.append({"mutation": name, "bitstream_filter": change,
                        "artifact": str(mutated), "sha256": digest(mutated),
                        "result": "EXPECTED FAIL", "syntax_log": commands[-2], "oracle": commands[-1]})
    assert source_identity() == source, "source changed during mutation verification"
    receipt = {"source": source, "reference_sha256": digest(reference),
               "candidate_sha256": digest(candidate), "commands": commands,
               "mutations": records, "result": "PASS"}
    with (output / "summary.json").open("x") as stream:
        json.dump(receipt, stream, indent=2, sort_keys=True)
        stream.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
