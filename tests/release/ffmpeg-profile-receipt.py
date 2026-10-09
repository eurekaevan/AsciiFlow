#!/usr/bin/env python3
"""Attest the isolated LGPL dependency, not the host's similarly named SONAMEs."""
import argparse
import ctypes
import hashlib
import json
import os
from pathlib import Path
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("prefix", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    prefix = args.prefix.resolve(strict=True)
    env = dict(os.environ, LD_LIBRARY_PATH=str(prefix / "lib"))

    def run(argv, selected=env):
        value = subprocess.run([str(x) for x in argv], env=selected, text=True,
                               capture_output=True, check=True, timeout=60)
        return value.stdout + value.stderr

    ffmpeg = prefix / "bin/ffmpeg"
    receipt = {"prefix": str(prefix), "status": "RUNNING", "libraries": {},
               "license": run([ffmpeg, "-L"]), "buildconf": run([ffmpeg, "-buildconf"])}
    assert "GNU Lesser General Public License" in " ".join(receipt["license"].split())
    closure = {}
    for path in sorted((prefix / "lib").glob("*.so.*")):
        path = path.resolve()
        if path.name in receipt["libraries"]:
            continue
        listing = run(["ldd", path])
        assert "not found" not in listing, listing
        for line in listing.splitlines():
            if "=> /" in line:
                dependency = Path(line.split("=>", 1)[1].strip().split()[0]).resolve()
                closure[str(dependency)] = hashlib.sha256(dependency.read_bytes()).hexdigest()
        receipt["libraries"][path.name] = {
            "realpath": str(path), "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
            "ldd": listing}
    assert all(not any(name in path.lower() for name in ("libx264", "libx265", "libfdk", "libxvid"))
               for path in closure), closure
    # Fresh child with the exact dependency search path; ctypes must not inherit
    # previously loaded host libav* from an in-process comparison probe.
    code = '''import ctypes,json
result={}
for component in ["avutil","avcodec","avformat"]:
 lib=ctypes.CDLL("lib"+component+".so")
 license=getattr(lib,component+"_license");license.restype=ctypes.c_char_p
 version=getattr(lib,component+"_version");version.restype=ctypes.c_uint
 result[component]={"license":license().decode(),"version_integer":version()}
print(json.dumps(result))'''
    receipt["reported_licenses"] = json.loads(run(["python3", "-c", code]))
    assert all(v["license"] == "LGPL version 2.1 or later"
               for v in receipt["reported_licenses"].values())
    required = json.loads((Path(__file__).with_name("required-ffmpeg-capabilities.json")).read_text())["production"]
    for category, option, needed in [
        ("decoders", "-decoders", required["software_video_decoders"] + ["av1", "aac"]),
        ("encoders", "-encoders", required["vaapi_video_encoders"]),
        ("demuxers", "-demuxers", required["demuxers"]),
        ("muxers", "-muxers", required["muxers"]),
        ("parsers", "-bsfs", required["bitstream_filters"]),
        ("protocols", "-protocols", required["protocols"]),
        ("hardware", "-hwaccels", ["vaapi"]),
        ("pixel_formats", "-pix_fmts", required["pixel_formats"]),
    ]:
        actual = run([ffmpeg, "-hide_banner", option])
        assert all(name in actual.split() or any(name in token.split(",") for token in actual.split())
                   for name in needed), (category, needed, actual)
        receipt[category] = actual
    assert all(name not in receipt["encoders"] for name in ("libx264", "libx265", "libfdk_aac", "libxvid"))
    host = dict(os.environ)
    host.pop("LD_LIBRARY_PATH", None)
    receipt["capability_diff"] = {}
    for category, option in [("decoders", "-decoders"), ("encoders", "-encoders"), ("demuxers", "-demuxers"), ("muxers", "-muxers")]:
        receipt["capability_diff"][category] = {"host": run(["/usr/bin/ffmpeg", "-hide_banner", option], host),
                                                 "release": receipt[category]}
    receipt["recursive_dependency_closure"] = closure
    receipt["status"] = "PASS"
    args.output.write_text(json.dumps(receipt, indent=2) + "\n")


if __name__ == "__main__":
    main()
