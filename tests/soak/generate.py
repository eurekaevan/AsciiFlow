#!/usr/bin/env python3
"""Materialize a finite D-1 input; production never rewinds a live demuxer."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[2]
FIXTURES = ROOT / "tests/fixtures/codecs"


def sha(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def generate(kind, frames, directory):
    if frames < 1:
        raise ValueError("frame count must be positive")
    directory.mkdir(parents=True, exist_ok=False)
    commands = []

    def checked(argv):
        command = [str(value) for value in argv]
        result = subprocess.run(command, capture_output=True, text=True, timeout=1800)
        commands.append(dict(argv=command, exit_status=result.returncode,
                             stdout=result.stdout, stderr=result.stderr))
        (directory / "commands.json").write_text(json.dumps(commands, indent=2) + "\n")
        result.check_returncode()
        return result.stdout

    version = checked(["ffmpeg", "-version"])
    if not version.startswith("ffmpeg version 8.1.3 "):
        raise ValueError("D-1 source recipe requires fixed FFmpeg 8.1.3")
    probe_version = checked(["ffprobe", "-version"])
    if not probe_version.startswith("ffprobe version 8.1.3 "):
        raise ValueError("D-1 source probe requires fixed FFmpeg 8.1.3")
    tools = {name: dict(path=str(Path(shutil.which(name)).resolve()),
                       sha256=sha(Path(shutil.which(name)))) for name in ("ffmpeg", "ffprobe")}
    recipes = [Path(__file__).resolve()]
    if kind == "sdr":
        source = directory / "source-300.mp4"
        recipe = FIXTURES / "generate-8bit-production-baseline.sh"
        recipes.append(recipe)
        checked(["bash", recipe, source])
        primaries, transfer, matrix = "bt709", "bt709", "bt709"
    else:
        codec = "hevc" if kind == "pq-preserve" else "av1"
        source = FIXTURES / f"{codec}-main10-pq-c3-legal-v1.mp4"
        checksums = {line.split()[1].removeprefix("./"): line.split()[0]
                     for line in (FIXTURES / "SHA256SUMS").read_text().splitlines() if line.strip()}
        if sha(source) != checksums[source.name]:
            raise ValueError("checked-in legal PQ source identity changed")
        # These sources retain their existing generator/provenance alongside the
        # checked-in fixtures; this recipe records the exact materialized input.
        recipes.append(FIXTURES / "SHA256SUMS")
        primaries, transfer, matrix = "bt2020", "smpte2084", "bt2020nc"
    output = directory / "input.mp4"
    checked(["ffmpeg", "-v", "error", "-nostdin", "-n", "-stream_loop", "-1",
             "-i", source, "-map", "0:v:0", "-an", "-c:v", "copy",
             "-frames:v", frames, "-color_primaries", primaries,
             "-color_trc", transfer, "-colorspace", matrix, "-color_range", "tv",
             "-map_metadata", "-1", "-fflags", "+bitexact", "-video_track_timescale",
             "90000", output])
    probe = json.loads(checked(["ffprobe", "-v", "error", "-select_streams", "v:0",
                               "-count_packets", "-show_streams", "-of", "json", output]))
    stream = probe["streams"][0]
    expected = dict(width=1920, height=1080, avg_frame_rate="50/1",
                    color_primaries=primaries, color_transfer=transfer,
                    color_space=matrix, color_range="tv")
    for field, value in expected.items():
        if stream.get(field) != value:
            raise ValueError(f"{field}: expected {value}, got {stream.get(field)}")
    if int(stream["nb_read_packets"]) != frames or int(stream["nb_frames"]) != frames:
        raise ValueError("materialized packet/frame count mismatch")
    identity = dict(schema_version=1, kind=kind, frames=frames, media_seconds=frames / 50,
                    audio="none", status="GeneratedNotQualified",
                    source=dict(path=str(source), sha256=sha(source), bytes=source.stat().st_size),
                    generators=[dict(path=str(path), sha256=sha(path)) for path in recipes],
                    ffmpeg_version=version, ffprobe_version=probe_version, tools=tools, commands=commands,
                    output=dict(path=str(output), sha256=sha(output), bytes=output.stat().st_size),
                    probe=probe)
    (directory / "identity.json").write_text(json.dumps(identity, indent=2) + "\n")
    return identity


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("kind", choices=("sdr", "pq-preserve", "pq-to-sdr"))
    parser.add_argument("--frames", type=int, default=100000)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    identity = generate(args.kind, args.frames, args.output.resolve())
    print(json.dumps(identity["output"], indent=2))


if __name__ == "__main__":
    main()
