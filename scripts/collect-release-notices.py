#!/usr/bin/env python3
"""Collect the locked Linux CLI dependency notices; never bundle native libraries."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, help="new notice directory")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    command = ["cargo", "tree", "--locked", "--offline", "--target",
               "x86_64-unknown-linux-gnu", "-p", "asciiflow-cli", "-e", "normal",
               "--prefix", "none", "--format", "{p}\t{l}"]
    tree = subprocess.check_output(command, cwd=root, text=True)
    cache = Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo")) / "registry/src"
    packages = {}
    for line in tree.splitlines():
        package, license_name = line.split("\t", 1)
        match = re.match(r"([^ ]+) v([^ ]+)", package)
        if not match:
            raise RuntimeError(f"unrecognized Cargo package: {package}")
        name, version = match.groups()
        if name.startswith("asciiflow-"):
            continue
        packages[(name, version)] = license_name.removesuffix(" (*)")
    # Fail rather than overwrite stale evidence from a different source identity.
    args.output.mkdir(parents=True, exist_ok=False)
    records = []
    for (name, version), declared in sorted(packages.items()):
        matches = list(cache.glob(f"*/{name}-{version}"))
        if len(matches) != 1:
            raise RuntimeError(f"expected one cached source for {name} {version}: {matches}")
        source = matches[0]
        destination = args.output / "licenses" / f"{name}-{version}"
        destination.mkdir(parents=True)
        texts = [path for path in source.iterdir()
                 if path.is_file() and path.name.upper().startswith(("LICENSE", "COPYING", "NOTICE"))]
        files = []
        for path in sorted(texts):
            target = destination / path.name
            shutil.copyfile(path, target)
            files.append({"path": str(target.relative_to(args.output)),
                          "sha256": hashlib.sha256(target.read_bytes()).hexdigest()})
        if not texts:
            if declared != "WTFPL" or name != "ffmpeg-sys-next":
                raise RuntimeError(f"no packaged license text for {name} ({declared})")
            # This upstream crate declares WTFPL but ships no license file. Preserve
            # that fact; do not invent its copyright attribution or license edition.
        records.append({"name": name, "version": version, "declared_license": declared,
                        "manifest_sha256": hashlib.sha256((source / "Cargo.toml").read_bytes()).hexdigest(),
                        "license_texts": files,
                        "missing_packaged_text": not bool(texts)})
    manifest = {"schema_version": 1, "scope": "locked Linux CLI normal-dependency graph; includes proc macros",
                "command": command, "cargo_lock_sha256": hashlib.sha256((root / "Cargo.lock").read_bytes()).hexdigest(),
                "native_libraries_bundled": False, "packages": records}
    (args.output / "rust-license-inventory.json").write_text(json.dumps(manifest, indent=2) + "\n")
    rows = ["# Rust dependency notices", "", "Generated from Cargo.lock by scripts/collect-release-notices.py.",
            "This inventory does not replace the native-runtime and combined-executable distribution notice.", "",
            "| Crate | Version | Declared license | Texts |", "| --- | --- | --- | --- |"]
    for row in records:
        links = ", ".join(f"[{Path(item['path']).name}]({item['path']})" for item in row["license_texts"])
        rows.append(f"| {row['name']} | {row['version']} | {row['declared_license']} | {links or 'Not packaged upstream; declaration retained'} |")
    (args.output / "RUST_DEPENDENCIES.md").write_text("\n".join(rows) + "\n")
    print(json.dumps({"packages": len(records), "output": str(args.output), "native_libraries_bundled": False}))


if __name__ == "__main__":
    main()
