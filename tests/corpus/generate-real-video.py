#!/usr/bin/env python3
"""Build a deterministic, self-describing real-video fixture corpus.

Requires the pinned FFmpeg/ffprobe 8.1.3 package and Python's standard library.
The destination must be a new directory. Every emitted media file is encoded,
containerized where applicable, fully decoded, and inspected by ffprobe.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
from fractions import Fraction
import tempfile
from pathlib import Path


FFMPEG = os.environ.get("ASCIIFLOW_CORPUS_FFMPEG", "ffmpeg")
FFPROBE = os.environ.get("ASCIIFLOW_CORPUS_FFPROBE", "ffprobe")
TIMEOUT_SECONDS = 120
RATES = ((24000, 1001), (24, 1), (25, 1), (30000, 1001), (30, 1),
         (50, 1), (60000, 1001), (60, 1))
PROBE_ENTRIES: list[dict[str, object]] = []
OUTPUTS: list[dict[str, object]] = []


def digest(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()


def h264_sps_nal(data: bytes) -> bytes:
    """Return the first raw H.264 SPS NAL from an Annex B stream."""
    units = re.split(rb"\x00\x00(?:\x00)?\x01", data)
    for unit in units:
        if unit and unit[0] & 0x1F == 7:
            return unit.rstrip(b"\x00")
    raise RuntimeError("encoded Annex B segment contains no SPS NAL")


def run(argv: list[str], *, timeout: int = TIMEOUT_SECONDS) -> str:
    result = subprocess.run(argv, check=True, stdout=subprocess.PIPE,
                            stderr=subprocess.PIPE, text=True, timeout=timeout)
    return result.stdout


def version(executable: str) -> str:
    return run([executable, "-version"])


def ffmpeg_args(name: str, input_args: list[str], output_args: list[str], out: Path) -> list[str]:
    return [FFMPEG, "-nostdin", "-hide_banner", "-loglevel", "error", "-n",
            *input_args, *output_args, str(out)]


def source_args(width: int, height: int, rate: str, frames: int) -> list[str]:
    return ["-f", "lavfi", "-i", f"testsrc2=size={width}x{height}:rate={rate}",
            "-frames:v", str(frames)]


def x264_args(*, rate: str, bf: int = 0, gop: int = 48, pix: str = "yuv420p",
              extra: tuple[str, ...] = ()) -> list[str]:
    return ["-an", "-c:v", "libx264", "-threads:v", "1", "-preset", "ultrafast",
            "-crf", "28", "-pix_fmt", pix, "-r", rate, "-fps_mode", "cfr",
            "-g", str(gop), "-keyint_min", str(gop), "-sc_threshold", "0",
            "-bf", str(bf), "-x264-params", f"bframes={bf}:scenecut=0:open-gop=0",
            "-color_primaries", "bt709", "-color_trc", "bt709", "-colorspace", "bt709",
            "-color_range", "tv", "-chroma_sample_location", "left",
            "-map_metadata", "-1", "-metadata", "creation_time=1970-01-01T00:00:00Z",
            "-fflags", "+bitexact", "-flags:v", "+bitexact", *extra]


def record_media(out: Path, encode_argv: list[str], label: str, expected_frames: int | None = None,
                 *, b_frames: int | None = None, key_interval: int | None = None,
                 time_base: str | None = None, frame_rate: str | None = None,
                 max_keyframe_index: int | None = None) -> None:
    # Decode the whole stream to the null muxer; decoder errors fail generation.
    decode_argv = [FFMPEG, "-nostdin", "-hide_banner", "-v", "error", "-xerror",
                   "-i", str(out), "-map", "0:v:0", "-an", "-f", "null", "-"]
    run(decode_argv)
    probe_argv = [FFPROBE, "-v", "error", "-show_streams", "-show_format",
                  "-show_packets", "-show_frames", "-show_data_hash", "sha256",
                  "-of", "json", str(out)]
    inventory = json.loads(run(probe_argv))
    video = [s for s in inventory.get("streams", []) if s.get("codec_type") == "video"]
    if len(video) != 1:
        raise RuntimeError(f"{label}: expected one video stream, saw {len(video)}")
    entries = inventory.get("packets_and_frames", [])
    frames = [f for f in entries if f.get("type") == "frame" and f.get("media_type") == "video"]
    if expected_frames is not None and len(frames) != expected_frames:
        raise RuntimeError(f"{label}: expected {expected_frames} decoded frames, got {len(frames)}")
    pict_types = [f.get("pict_type") for f in frames]
    if b_frames is not None and (("B" in pict_types) != (b_frames > 0)):
        raise RuntimeError(f"{label}: encoded B-frame presence disagrees with requested bframes={b_frames}")
    key_indices = [i for i, frame in enumerate(frames) if frame.get("key_frame") == 1]
    if key_interval == 1 and key_indices != list(range(len(frames))):
        raise RuntimeError(f"{label}: expected every decoded frame to be a keyframe")
    if key_interval and key_interval > 1 and any(b - a != key_interval for a, b in zip(key_indices, key_indices[1:])):
        raise RuntimeError(f"{label}: observed keyframe cadence differs from {key_interval}: {key_indices}")
    if max_keyframe_index is not None and any(index > max_keyframe_index for index in key_indices):
        raise RuntimeError(f"{label}: unexpected keyframe after index {max_keyframe_index}: {key_indices}")
    actual_time_base = video[0].get("time_base")
    if time_base is not None and actual_time_base != time_base:
        raise RuntimeError(f"{label}: expected time_base={time_base}, got {actual_time_base}")
    if frame_rate is not None and video[0].get("avg_frame_rate") != frame_rate:
        raise RuntimeError(f"{label}: expected avg_frame_rate={frame_rate}, got {video[0].get('avg_frame_rate')}")
    PROBE_ENTRIES.append({"path": out.name, "argv": probe_argv, "json": inventory})
    OUTPUTS.append({"path": out.name, "bytes": out.stat().st_size, "sha256": digest(out),
                    "generator_argv": encode_argv, "full_decode_argv": decode_argv,
                    "video_stream": video[0], "decoded_video_frames": len(frames),
                    "frame_semantics": [{"pts": f.get("pts"), "duration": f.get("duration"),
                                         "key_frame": f.get("key_frame"), "pict_type": f.get("pict_type"),
                                         "width": f.get("width"), "height": f.get("height"),
                                         "pix_fmt": f.get("pix_fmt")} for f in frames],
                    "packet_count": sum(p.get("type") == "packet" and p.get("codec_type") == "video"
                                        for p in entries),
                    "format": inventory.get("format", {})})


def encode(directory: Path, stem: str, width: int, height: int, rate: str,
           frames: int, *, bf: int = 0, gop: int = 48, suffix: str = "mp4",
           pix: str = "yuv420p", mux: tuple[str, ...] = (), out_extra: tuple[str, ...] = ()) -> Path:
    out = directory / f"{stem}.{suffix}"
    argv = ffmpeg_args(stem, source_args(width, height, rate, frames),
                       ["-an", *x264_args(rate=rate, bf=bf, gop=gop, pix=pix), *mux,
                        *out_extra], out)
    run(argv)
    record_media(out, argv, stem, frames, b_frames=bf,
                 key_interval=gop if gop == 1 else None,
                 time_base=(f"1/{out_extra[out_extra.index('-video_track_timescale') + 1]}"
                            if "-video_track_timescale" in out_extra else None),
                 frame_rate=f"{Fraction(rate).numerator}/{Fraction(rate).denominator}",
                 max_keyframe_index=0 if gop >= 250 else None)
    return out


def encode_vfr(directory: Path) -> None:
    out = directory / "vfr-three-durations.mp4"
    vf = "settb=1/1000,setpts='if(eq(N,0),0,if(eq(N,1),40,if(eq(N,2),120,240)))'"
    argv = ffmpeg_args("vfr-three-durations",
                       ["-f", "lavfi", "-i", "testsrc2=size=128x96:rate=25", "-frames:v", "4"],
                       ["-vf", vf, "-an", "-c:v", "libx264", "-threads:v", "1", "-preset", "ultrafast",
                        "-crf", "28", "-pix_fmt", "yuv420p", "-enc_time_base", "1/1000",
                        "-fps_mode", "vfr", "-bf", "0", "-g", "48", "-color_primaries", "bt709",
                        "-color_trc", "bt709", "-colorspace", "bt709", "-color_range", "tv",
                        "-chroma_sample_location", "left", "-video_track_timescale", "1000",
                        "-map_metadata", "-1", "-fflags", "+bitexact", "-flags:v", "+bitexact"], out)
    run(argv)
    record_media(out, argv, "vfr-three-durations", 4, time_base="1/1000")
    frames = [f for f in PROBE_ENTRIES[-1]["json"]["packets_and_frames"] if f.get("type") == "frame"]
    durations = {f.get("duration") for f in frames}
    if len(durations) < 3:
        raise RuntimeError(f"VFR fixture does not contain three distinct durations: {durations}")


def encode_odd_av1_420(directory: Path) -> None:
    """Isolate odd geometry from unsupported chroma, using the audited recipe."""
    out = directory / "odd-geometry-av1-420.mp4"
    with tempfile.TemporaryDirectory(prefix=".odd-av1-", dir=directory) as temporary:
        source = Path(temporary) / "source.mp4"
        encode_argv = ffmpeg_args(
            "odd-geometry-av1-420-source",
            ["-f", "lavfi", "-i", "testsrc=size=129x97:rate=25:duration=0.12"],
            ["-vf", "scale=out_color_matrix=bt709:out_range=tv,format=yuv420p",
             "-frames:v", "3", "-an", "-c:v", "libaom-av1", "-usage", "realtime",
             "-cpu-used", "8", "-threads:v", "1", "-row-mt", "0", "-lag-in-frames", "0",
             "-auto-alt-ref", "0", "-pix_fmt", "yuv420p", "-color_primaries", "bt709",
             "-color_trc", "bt709", "-colorspace", "bt709", "-color_range", "tv",
             "-chroma_sample_location", "left", "-map_metadata", "-1", "-fflags", "+bitexact"], source)
        run(encode_argv)
        source_sha256 = digest(source)
        # libaom realtime did not expose every requested color field. The native
        # AV1 sequence-header filter supplies explicit, probe-verified semantics.
        argv = ffmpeg_args(
            "odd-geometry-av1-420", ["-i", str(source)],
            ["-map", "0:v:0", "-c:v", "copy", "-bsf:v",
             "av1_metadata=color_primaries=1:transfer_characteristics=1:matrix_coefficients=1:color_range=0:chroma_sample_position=vertical",
             "-map_metadata", "-1", "-fflags", "+bitexact"], out)
        run(argv)
        record_media(out, argv, "odd-geometry-av1-420", 3,
                     time_base="1/12800", frame_rate="25/1")
        OUTPUTS[-1]["source_encoding_argv"] = encode_argv
        OUTPUTS[-1]["encoded_source_sha256"] = source_sha256
    frames = [entry for entry in PROBE_ENTRIES[-1]["json"]["packets_and_frames"]
              if entry.get("type") == "frame" and entry.get("media_type") == "video"]
    for frame in frames:
        observed = tuple(frame.get(key) for key in
                         ("width", "height", "pix_fmt", "color_space", "color_primaries",
                          "color_transfer", "color_range", "chroma_location"))
        if observed != (129, 97, "yuv420p", "bt709", "bt709", "bt709", "tv", "left"):
            raise RuntimeError(f"odd-geometry-av1-420: unexpected decoded semantics {observed}")
    if [frame.get("pts") for frame in frames] != [0, 512, 1024]:
        raise RuntimeError("odd-geometry-av1-420: unexpected decoded presentation timestamps")


def encode_offset(directory: Path, stem: str, seconds: str) -> None:
    out = directory / f"{stem}.mp4"
    argv = ffmpeg_args(stem, source_args(128, 96, "24", 12),
                       ["-an", "-c:v", "libx264", "-threads:v", "1", "-preset", "ultrafast",
                        "-crf", "28", "-pix_fmt", "yuv420p", "-bf", "0", "-g", "48",
                        "-output_ts_offset", seconds, "-video_track_timescale", "1000",
                        "-map_metadata", "-1", "-fflags", "+bitexact", "-flags:v", "+bitexact"], out)
    run(argv)
    expected_start = int(Fraction(seconds) * 1000)
    record_media(out, argv, stem, 12, time_base="1/1000")
    actual = PROBE_ENTRIES[-1]["json"]["streams"][0].get("start_pts")
    if actual != expected_start:
        raise RuntimeError(f"{stem}: expected stream start_pts={expected_start}, got {actual}")


def encode_parameter_change(directory: Path, stem: str,
                            segments: tuple[tuple[int, int, str, str, str], ...],
                            verify, *, encode_options: tuple[tuple[str, ...], ...] | None = None,
                            require_distinct_sps: bool = False, semantic_note: str | None = None) -> None:
    """Concatenate actual independently encoded Annex B segments with SPS changes."""
    out = directory / f"{stem}.h264"
    argv_segments: list[list[str]] = []
    with tempfile.TemporaryDirectory(prefix=".segments-", dir=directory) as temporary:
        temp = Path(temporary)
        parts: list[bytes] = []
        for index, (width, height, pix_fmt, primaries, matrix) in enumerate(segments):
            part = temp / f"segment-{index}.h264"
            argv = [FFMPEG, "-nostdin", "-hide_banner", "-loglevel", "error", "-n",
                    "-f", "lavfi", "-i", "testsrc2=size=128x96:rate=24", "-frames:v", "4",
                    "-vf", f"scale={width}:{height}", "-an", "-c:v", "libx264", "-threads:v", "1",
                    "-preset", "ultrafast", "-crf", "28", "-pix_fmt", pix_fmt, "-bf", "0", "-g", "1",
                    "-color_primaries", primaries, "-color_trc", "bt709", "-colorspace", matrix,
                    "-color_range", "tv", *(encode_options[index] if encode_options else ()),
                    "-f", "h264", str(part)]
            run(argv)
            argv_segments.append(argv)
            parts.append(part.read_bytes())
        sps_nals = [h264_sps_nal(part) for part in parts]
        if require_distinct_sps and (len(sps_nals) != 2 or sps_nals[0] == sps_nals[1]):
            raise RuntimeError(f"{stem}: expected two byte-distinct SPS NALs")
        out.write_bytes(b"".join(parts))
    decode_argv = [FFMPEG, "-nostdin", "-hide_banner", "-v", "error", "-xerror", "-i", str(out),
                   "-map", "0:v:0", "-an", "-f", "null", "-"]
    run(decode_argv)
    probe_argv = [FFPROBE, "-v", "error", "-show_streams", "-show_format", "-show_packets",
                  "-show_frames", "-show_data_hash", "sha256", "-of", "json", str(out)]
    inventory = json.loads(run(probe_argv))
    entries = inventory.get("packets_and_frames", [])
    frames = [f for f in entries if f.get("type") == "frame" and f.get("media_type") == "video"]
    if len(frames) != 8 or not verify(frames):
        raise RuntimeError(f"{stem}: encoded SPS transition failed frame-level verification")
    stream = next((s for s in inventory.get("streams", []) if s.get("codec_type") == "video"), None)
    if stream is None:
        raise RuntimeError(f"{stem}: no video stream in probe output")
    PROBE_ENTRIES.append({"path": out.name, "argv": probe_argv, "json": inventory})
    output = {"path": out.name, "bytes": out.stat().st_size, "sha256": digest(out),
                    "generator_argv_segments": argv_segments, "full_decode_argv": decode_argv,
                    "video_stream": stream, "decoded_video_frames": len(frames),
                    "frame_semantics": [{"width": f.get("width"), "height": f.get("height"),
                                         "pix_fmt": f.get("pix_fmt"), "color_range": f.get("color_range"),
                                         "color_space": f.get("color_space"),
                                         "chroma_location": f.get("chroma_location"),
                                         "color_primaries": f.get("color_primaries"),
                                         "color_transfer": f.get("color_transfer")} for f in frames],
                    "packet_count": sum(p.get("type") == "packet" and p.get("codec_type") == "video"
                                        for p in entries), "format": inventory.get("format", {})}
    if sps_nals and require_distinct_sps:
        output["segment_sps_sha256s"] = [hashlib.sha256(nal).hexdigest() for nal in sps_nals]
        output["parameter_update"] = "H.264 level_idc change only; same dimensions, pixel format, and color metadata"
    if semantic_note:
        output["semantic_note"] = semantic_note
    OUTPUTS.append(output)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output_dir", type=Path, help="new, empty output directory")
    args = parser.parse_args()
    dest = args.output_dir.expanduser().absolute()
    if dest.exists():
        parser.error(f"refusing existing output directory: {dest}")
    if not dest.parent.is_dir():
        parser.error(f"output parent must already exist: {dest.parent}")
    ffmpeg_version = version(FFMPEG)
    ffprobe_version = version(FFPROBE)
    if not ffmpeg_version.startswith("ffmpeg version 8.1.3 ") or not ffprobe_version.startswith("ffprobe version 8.1.3 "):
        parser.error(f"requires FFmpeg/ffprobe 8.1.3; found {ffmpeg_version.splitlines()[0]!r}, {ffprobe_version.splitlines()[0]!r}")
    dest.mkdir()

    # Eight required rational CFR values, with a fixed 16-frame sample each.
    for num, den in RATES:
        rate = f"{num}/{den}"
        encode(dest, f"cfr-{num}-{den}", 128, 96, rate, 16)

    # Reordering and GOP controls are verified from decoded frames and packets.
    for bf in (0, 2, 4):
        encode(dest, f"h264-bframes-{bf}", 128, 96, "24", 24, bf=bf)
    hevc = dest / "hevc-bframes-2.mp4"
    argv = ffmpeg_args("hevc-bframes-2", source_args(128, 128, "24", 24),
                       ["-an", "-c:v", "libx265", "-threads:v", "1", "-preset", "ultrafast",
                        "-x265-params", "pools=1:frame-threads=1:bframes=2:scenecut=0",
                        "-bf", "2", "-g", "48", "-pix_fmt", "yuv420p", "-r", "24",
                        "-fps_mode", "cfr", "-color_primaries", "bt709", "-color_trc", "bt709",
                        "-colorspace", "bt709", "-color_range", "tv", "-chroma_sample_location", "left",
                        "-map_metadata", "-1", "-fflags", "+bitexact", "-flags:v", "+bitexact"], hevc)
    run(argv)
    record_media(hevc, argv, "hevc-bframes-2", 24, b_frames=2, frame_rate="24/1")

    encode(dest, "long-gop-250", 128, 96, "25", 250, gop=250)
    encode(dest, "all-intra", 128, 96, "24", 24, gop=1)
    for n in (1, 2, 3):
        encode(dest, f"short-{n}-frames", 128, 96, "24", n, gop=1)

    # Track timescales yield stream time bases; each value is asserted by probing.
    for scale in (1000, 90000, 48000, 1000000):
        encode(dest, f"timebase-{scale}", 128, 96, "24", 12,
               out_extra=("-video_track_timescale", str(scale)))

    # MP4 layout variants and Matroska exercise real mux/demux paths.
    encode(dest, "mp4-faststart", 128, 96, "24", 12, mux=("-movflags", "+faststart"))
    encode(dest, "mp4-fragmented", 128, 96, "24", 12,
           mux=("-movflags", "frag_keyframe+empty_moov+default_base_moof"))
    encode(dest, "matroska", 128, 96, "24", 12, suffix="mkv")

    for width, height, label in ((2, 2, "2x2"), (4, 4, "4x4"), (128, 96, "128x96"),
                                 (1280, 720, "720p"), (1920, 1080, "1080p")):
        encode(dest, f"size-{label}", width, height, "24", 2, gop=1)
    # Odd geometry is real H.264 4:4:4 (2x2 chroma restriction does not apply).
    # This is deliberately labelled as an input-pixel-format unsupported case.
    encode(dest, "odd-geometry-inputpixfmt-unsupported", 129, 97, "24", 2,
           gop=1, pix="yuv444p")
    encode_odd_av1_420(dest)

    encode_vfr(dest)
    encode_offset(dest, "start-pts-plus-two-seconds", "2")
    encode_offset(dest, "start-pts-large", "86400")
    encode_parameter_change(dest, "same-dimension-changed-sps",
                            ((128, 96, "yuv420p", "bt709", "bt709"),
                             (128, 96, "yuv420p", "bt470bg", "bt470bg")),
                            lambda fs: len({f.get("color_space") for f in fs}) > 1,
                            semantic_note="Color-metadata SPS mutation (BT.709 to BT.470BG); retained filename for corpus stability.")
    encode_parameter_change(dest, "same-dimension-parameter-update",
                            ((128, 96, "yuv420p", "bt709", "bt709"),
                             (128, 96, "yuv420p", "bt709", "bt709")),
                            lambda fs: len({(f.get("width"), f.get("height"), f.get("pix_fmt"),
                                             f.get("color_range"), f.get("color_space"),
                                             f.get("chroma_location")) for f in fs}) == 1,
                            encode_options=(("-level:v", "3.0"), ("-level:v", "3.1")),
                            require_distinct_sps=True)
    encode_parameter_change(dest, "midstream-resolution-change",
                            ((128, 96, "yuv420p", "bt709", "bt709"),
                             (160, 120, "yuv420p", "bt709", "bt709")),
                            lambda fs: len({(f.get("width"), f.get("height")) for f in fs}) > 1)
    encode_parameter_change(dest, "midstream-bit-depth-change",
                            ((128, 96, "yuv420p", "bt709", "bt709"),
                             (128, 96, "yuv420p10le", "bt709", "bt709")),
                            lambda fs: len({f.get("pix_fmt") for f in fs}) > 1)

    # Record unconstructed mutations explicitly. These must not be mistaken for
    # fixtures: a parser can qualify them only after a genuine conforming file exists.
    blocked = [
    ]
    script_path = Path(__file__).resolve()
    report = {"generator": str(script_path.name), "generator_sha256": digest(script_path),
              "ffmpeg_version_argv": [FFMPEG, "-version"], "ffmpeg_version": ffmpeg_version,
              "ffprobe_version_argv": [FFPROBE, "-version"], "ffprobe_version": ffprobe_version,
              "configuration": {"size": "128x96 default", "default_codec": "h264/libx264",
                                "default_pixel_format": "yuv420p", "color": "bt709 limited left",
                                "randomness": "none", "clock_metadata": "removed or fixed",
                                "dependencies": "Python standard library only"},
              "outputs": OUTPUTS, "probe_inventory": PROBE_ENTRIES,
              "not_constructed": blocked}
    report_path = dest / "inventory.json"
    report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"generated {len(OUTPUTS)} verified videos in {dest}")
    print(f"inventory sha256={digest(report_path)} bytes={report_path.stat().st_size}")
    for item in blocked:
        print(f"not constructed: {item['case']}: {item['reason']}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
