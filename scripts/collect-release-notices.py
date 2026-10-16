#!/usr/bin/env python3
"""Collect the locked application dependency notices; never bundle native libraries."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tomllib


# These exact locked releases omit workspace-level license files from their crates.
# Retain original texts locally; verify the crate-recorded commit and fixed hashes.
UPSTREAM_NOTICES = {
    "accesskit-0.17.1": ("AccessKit/accesskit", "405d578cdfd8496ee79020d24cfaac8c29a8a48f", "MIT OR Apache-2.0", {
        "LICENSE-MIT": "23f18e03dc49df91622fe2a76176497404e46ced8a715d9d2b67a7446571cca3",
        "LICENSE-APACHE": "62c7a1e35f56406896d7aa7ca52d0cc0d272ac022b5d2796e7d6905db8a3636a",
        "LICENSE.chromium": "845022e0c1db1abb41a6ba4cd3c4b674ec290f3359d9d3c78ae558d4c0ed9308",
    }),
    "accesskit_winit-0.23.1": ("AccessKit/accesskit", "405d578cdfd8496ee79020d24cfaac8c29a8a48f", "Apache-2.0", {
        "LICENSE-APACHE": "62c7a1e35f56406896d7aa7ca52d0cc0d272ac022b5d2796e7d6905db8a3636a",
    }),
    "atspi-proxies-0.6.0": ("odilia-app/atspi", "f7976706c3c8cff7117fd50d64b4f14693e81e72", "Apache-2.0 OR MIT", {
        "LICENSE-APACHE2.txt": "50e6751797c50dedd75ef1b8a0d9e42f5f8472e9fbce91f34718e9f97b0c780a",
        "LICENSE-MIT.txt": "a94bf521d21b7db984c8b141b6f9341bbedbe24e617c1a55d952f8ef613ef1cd",
    }),
    "zbus_xml-4.0.0": ("dbus2/zbus", "25876f399b068749c337374505c35e92dc9e5872", "MIT", {
        "LICENSE-MIT": "23f18e03dc49df91622fe2a76176497404e46ced8a715d9d2b67a7446571cca3",
    }),
    "egui-0.31.1": ("emilk/egui", "1669e52a7ccfc3489c1b0999b9ed48894a0b3887", "MIT OR Apache-2.0", {
        "LICENSE-MIT": "95ca92f5f8ea5231f1580b3a2a799e8260af3114b900e1def5355a7f44bcf60c",
        "LICENSE-APACHE": "8173d5c29b4f956d532781d2b86e4e30f83e6b7878dce18c919451d6ba707c90",
    }),
    "profiling-1.0.18": ("aclysma/profiling", "8271551172eb6fa4cba47369aedd93790c623df9", "MIT OR Apache-2.0", {
        "LICENSE-MIT": "c8167fdeeed46d3f244d3f85c5bf998ce889343691c32be2c61a8bc4b5c08333",
        "LICENSE-APACHE": "10d30a673cd5e9349bdc02aeb48f14b3386d27d0da32df8f0a555d4aa16aa551",
    }),
    "clipboard-win-5.4.1": ("DoumanAsh/clipboard-win", "3b27cf2bfd1adcfa6e0264eb51c1025ddaf0f342", "BSL-1.0", {
        "LICENSE": "c9bff75738922193e67fa726fa225535870d2aa1059f91452c411736284ad566",
    }),
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, help="new notice directory")
    parser.add_argument("--target", default="x86_64-unknown-linux-gnu")
    parser.add_argument("--features", default="", help="Cargo features used for the packaged binaries")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    command = ["cargo", "tree", "--locked", "--offline", "--target",
               args.target, "-p", "asciiflow-cli", "-e", "normal",
               "--prefix", "none", "--format", "{p}"]
    if args.features:
        command.extend(["--features", args.features])
    tree = subprocess.check_output(command, cwd=root, text=True)
    cache = Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo")) / "registry/src"
    packages = set()
    for line in tree.splitlines():
        match = re.match(r"([^ ]+) v([^ ]+)", line)
        if not match:
            raise RuntimeError(f"unrecognized Cargo package: {line}")
        name, version = match.groups()
        if name.startswith("asciiflow-"):
            continue
        packages.add((name, version))
    # Fail rather than overwrite stale evidence from a different source identity.
    args.output.mkdir(parents=True, exist_ok=False)
    records = []
    for name, version in sorted(packages):
        matches = list(cache.glob(f"*/{name}-{version}"))
        if len(matches) != 1:
            raise RuntimeError(f"expected one cached source for {name} {version}: {matches}")
        source = matches[0]
        # Read metadata directly: Cargo's display adds repeat markers to tree lines.
        manifest_bytes = (source / "Cargo.toml").read_bytes()
        declared = tomllib.loads(manifest_bytes.decode("utf-8"))["package"].get("license", "")
        destination = args.output / "licenses" / f"{name}-{version}"
        destination.mkdir(parents=True)
        texts = [path for path in source.rglob("*")
                 if path.is_file() and path.name.upper().startswith(("LICENSE", "COPYING", "NOTICE", "OFL", "UFL"))]
        if name == "epaint_default_fonts":
            # These upstream font notices do not use conventional license names.
            texts.extend(source / relative for relative in (
                "fonts/Hack-Regular.txt", "fonts/emoji-icon-font-mit-license.txt"))
        files = []
        for path in sorted(texts):
            target = destination / path.relative_to(source)
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(path, target)
            files.append({"path": str(target.relative_to(args.output)),
                          "sha256": hashlib.sha256(target.read_bytes()).hexdigest()})
        upstream = None
        if name in {"ecolor", "eframe", "egui", "egui-winit", "egui_glow", "emath", "epaint"} and version == "0.31.1":
            upstream = "egui-0.31.1"
        elif (name, version) in {
            ("accesskit", "0.17.1"), ("accesskit_atspi_common", "0.10.1"),
            ("accesskit_consumer", "0.26.0"), ("accesskit_unix", "0.13.1"),
            ("accesskit_windows", "0.24.1"),
        }:
            upstream = "accesskit-0.17.1"
        elif (name, version) in {
            ("profiling", "1.0.18"), ("clipboard-win", "5.4.1"),
            ("accesskit_winit", "0.23.1"), ("atspi-proxies", "0.6.0"),
            ("zbus_xml", "4.0.0"),
        }:
            upstream = f"{name}-{version}"
        if not texts and upstream:
            repository, commit, license_id, expected = UPSTREAM_NOTICES[upstream]
            assert declared == license_id, (name, declared)
            vcs = json.loads((source / ".cargo_vcs_info.json").read_text())
            assert vcs["git"]["sha1"] == commit, (name, vcs)
            for filename, digest in expected.items():
                url = f"https://raw.githubusercontent.com/{repository}/{commit}/{filename}"
                content = (root / "third_party/rust-licenses" / upstream / filename).read_bytes()
                assert hashlib.sha256(content).hexdigest() == digest, url
                target = destination / filename
                target.write_bytes(content)
                files.append({"path": str(target.relative_to(args.output)),
                              "sha256": digest, "upstream_url": url})
        if not files:
            if declared != "WTFPL" or name != "ffmpeg-sys-next":
                raise RuntimeError(f"no packaged license text for {name} ({declared})")
            # This upstream crate declares WTFPL but ships no license file. Preserve
            # that fact; do not invent its copyright attribution or license edition.
        records.append({"name": name, "version": version, "declared_license": declared,
                        "manifest_sha256": hashlib.sha256(manifest_bytes).hexdigest(),
                        "license_texts": files,
                        "missing_packaged_text": not bool(texts),
                        "missing_collected_text": not bool(files)})
    manifest = {"schema_version": 1, "scope": "locked application normal-dependency graph; includes proc macros", "target": args.target,
                "features": args.features,
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
