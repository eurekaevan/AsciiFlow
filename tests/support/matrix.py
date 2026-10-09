#!/usr/bin/env python3
"""Render the versioned contract; never maintain a second handwritten matrix."""
import argparse
import itertools
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
BEGIN = "<!-- production-support-contract:begin -->"
END = "<!-- production-support-contract:end -->"


def profile_availability(contract, case, profile):
    """Apply distribution exclusions without changing a case's base state."""
    for exclusion in contract["release_profiles"][profile]["exclusions"]:
        if all(case.get(key) == value for key, value in exclusion["match"].items()):
            return exclusion["availability"], exclusion["reason"]
    return "Eligible", "Base conditions and actual runtime capability probes still apply."


def render(contract):
    lines = [BEGIN, f"Support contract `{contract['support_contract_version']}`. Hardware conditions and evidence: [production support contract](docs/production-support.md).", "",
             "| Dimension | Value | Support state | Conditions |", "|---|---|---|---|"]
    for name in ("input_container", "output_container", "input_color", "output_profile", "output_dynamic_range"):
        for value in contract["dimensions"].get(name, []):
            lines.append(f"| {name} | {value['value']} | {value['status']} | {value.get('conditions', '')} |")
    for name, profile in contract.get("release_profiles", {}).items():
        lines += ["", f"Release profile `{name}`: {profile['description']}"]
        lines.extend(f"- {item['reason']}" for item in profile["exclusions"])
    return "\n".join([*lines, END])


def full(contract):
    lines = ["# Production support contract", "", f"Version: {contract['support_contract_version']}. Generated from `tests/support/production-support-v1.json`; do not edit the tables manually.", "", "## Support states", ""]
    for state, definition in sorted(contract["states"].items()):
        lines.append(f"- {state}: {definition}")
    lines += ["", "## Qualification scope", ""]
    lines.extend(f"- {key}: {value}" for key, value in sorted(contract["hardware_scope"].items()))
    lines += ["", "## Production limits", "", "```json", json.dumps(contract["production_limits"], indent=2, sort_keys=True), "```"]
    lines += ["", "## Dimensions", "", "Rows classify individual dimensions, not their Cartesian product. Conditions and scenario tests govern complete paths.", "", "| Dimension | Value | State | Conditions | Evidence |", "|---|---|---|---|---|"]
    for name, values in sorted(contract["dimensions"].items()):
        for value in sorted(values, key=lambda item: item["value"]):
            lines.append(f"| {name} | {value['value']} | {value['status']} | {value.get('conditions', '')} | {', '.join(value['evidence'])} |")
    lines += ["", "## Complete scenario inventory", "",
              "Generated from the existing contract cases, not a Cartesian-product support promise. Each row retains its declared backend, decode/encode and interop requirements in the source contract. Dynamic range records input color classification and explicit output intent.", "",
              "Audio and container cells below are globally conditioned scope references, not additional qualification of each row with every audio policy or container. Only exact retained/corpus tuples qualify MP4-compatible single/dual AAC copy or video-only output; audio copy requires the existing CFR timeline. Other container/audio combinations keep their dimension states above.", "",
              "| Scenario | Input codec | Input bit depth | Dynamic range (input / output intent) | Output codec | Output bit depth | Audio | Container | Status |",
              "|---|---|---|---|---|---|---|---|---|"]
    for case in contract["cases"]:
        lines.append(f"| {case['id']} | {case['input_codec']} | {case['input_depth']} | {case['color']} / {case['output_dynamic_range']} | {case['output_codec']} | {case['output_depth']} | Global audio conditions above; not per-row qualification | MP4 only under global conditions above | {case['status']} |")
    for name, profile in contract.get("release_profiles", {}).items():
        lines += ["", f"## Release profile `{name}`", "", profile["description"], "", profile["runtime_requirement"], "", profile["retained_base_qualification"], "",
                  "Availability is a distribution overlay, not a fifth base support state. Eligible does not promote Unsupported or Unqualified base cases.", "",
                  "| Scenario | Base state | Release availability | Reason |", "|---|---|---|---|"]
        for case in contract["cases"]:
            availability, reason = profile_availability(contract, case, name)
            lines.append(f"| {case['id']} | {case['status']} | {availability} | {reason} |")
    lines += ["", "## Evidence", ""]
    for evidence in sorted(contract["evidence"], key=lambda x: x["id"]):
        lines.append(f"- `{evidence['id']}`: [{evidence['artifact']}](../{evidence['artifact']}) — {evidence['stage']}, {evidence['kind']}, {evidence['status']}.")
    return "\n".join(lines) + "\n"


def coverage(contract):
    pairs = (("input_codec", "input_depth"), ("input_codec", "color"),
             ("color", "output_codec"), ("backend", "decode"), ("output_codec", "encode"))
    report = {}
    for first, second in pairs:
        cases = contract["cases"]
        observed = sorted({(str(case[first]), str(case[second])) for case in cases})
        universe = set(itertools.product({str(case[first]) for case in cases}, {str(case[second]) for case in cases}))
        report[f"{first} x {second}"] = {"covered": observed, "not_covered": sorted(universe - set(observed)),
                                        "scope": "contract scenario inventory, not an arbitrary percentage target"}
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("readme", "full", "coverage", "check"))
    args = parser.parse_args()
    contract = json.loads((ROOT / "tests/support/production-support-v1.json").read_text())
    if args.mode == "check":
        for profile in contract.get("release_profiles", {}).values():
            for exclusion in profile["exclusions"]:
                assert exclusion["availability"] == "Excluded"
                assert exclusion["match"] and exclusion["reason"]
                assert all(key in contract["cases"][0] for key in exclusion["match"])
        assert profile_availability(contract, next(c for c in contract["cases"] if c["id"] == "portable-h264-sdr"), "lgpl-prebuilt")[0] == "Excluded"
        assert profile_availability(contract, next(c for c in contract["cases"] if c["id"] == "sdr-h264-8"), "lgpl-prebuilt")[0] == "Eligible"
        readme = (ROOT / "README.md").read_text()
        actual = readme[readme.index(BEGIN):readme.index(END) + len(END)]
        assert actual == render(contract), "README support table drift"
        assert (ROOT / "docs/production-support.md").read_text() == full(contract), "support document drift"
    elif args.mode == "coverage":
        print(json.dumps(coverage(contract), sort_keys=True, indent=2))
    else:
        print(render(contract) if args.mode == "readme" else full(contract), end="" if args.mode == "full" else "\n")


if __name__ == "__main__":
    main()
