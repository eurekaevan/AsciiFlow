#!/usr/bin/env python3
"""Capture an environment or diff its structured capability evidence, not run media."""
import argparse
import json
from pathlib import Path
import sys

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "corpus"))
from portability import StackSetupError, capability_diff, capture


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="operation", required=True)
    current = sub.add_parser("capture")
    current.add_argument("--stack-id", required=True)
    current.add_argument("--binary", type=Path, required=True)
    current.add_argument("--prefix", type=Path)
    current.add_argument("--selectors", type=Path, help="schema-v2 explicit child environment JSON")
    current.add_argument("--provenance", type=Path)
    diff = sub.add_parser("diff")
    diff.add_argument("reference", type=Path)
    diff.add_argument("candidate", type=Path)
    args = parser.parse_args()
    if args.operation == "capture":
        try:
            document = capture(args.stack_id, args.binary, args.prefix,
                               json.loads(args.selectors.read_text()) if args.selectors else None,
                               json.loads(args.provenance.read_text()) if args.provenance else None)
        except StackSetupError as error:
            print(json.dumps({"status": "SetupFailed", "reason": str(error), "native_probes": error.probes}, indent=2, sort_keys=True))
            return 2
    else:
        document = {"schema_version": 1, "differences": capability_diff(
            json.loads(args.reference.read_text()), json.loads(args.candidate.read_text()))}
    print(json.dumps(document, indent=2, sort_keys=True))


if __name__ == "__main__":
    raise SystemExit(main())
