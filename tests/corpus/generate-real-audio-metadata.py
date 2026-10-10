#!/usr/bin/env python3
"""Generate real-media audio/metadata candidates, never promote them into a qualified corpus.

Requires FFmpeg/ffprobe 8.1.3. All media is local and deterministic; every
subprocess has a watchdog. Inventory records observations, not argv promises.
"""

import argparse
import ctypes
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess


ROOT = Path(__file__).resolve().parents[2]
SUBTITLE = "1\n00:00:00,200 --> 00:00:01,200\nRetained metadata subtitle\n"


def identity(path):
    return {"path": str(path), "bytes": path.stat().st_size,
            "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}


def save(path, value):
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("out_dir", type=Path, help="directory that must not exist")
    parser.add_argument("--ffmpeg", default="ffmpeg")
    parser.add_argument("--ffprobe", default="ffprobe")
    parser.add_argument("--watchdog-seconds", type=int, default=180)
    args = parser.parse_args()
    if args.watchdog_seconds <= 0:
        parser.error("watchdog must be positive")
    out = args.out_dir.resolve()
    out.mkdir(parents=False, exist_ok=False)
    commands = []
    inventory = {"stage": "real-media-candidates", "qualification": "candidate-only",
                 "script": identity(Path(__file__).resolve()), "tools": {},
                 "sources": [], "artifacts": [], "limitations": [], "commands": commands}

    def run(argv):
        record = {"argv": [str(x) for x in argv], "cwd": str(ROOT)}
        commands.append(record)
        try:
            result = subprocess.run(record["argv"], cwd=ROOT, env={**os.environ, "LC_ALL": "C"},
                                    capture_output=True, text=True, timeout=args.watchdog_seconds)
        except subprocess.TimeoutExpired:
            record["failure"] = "watchdog-expired"
            raise
        stdout_bytes = result.stdout.encode("utf-8")
        record.update(returncode=result.returncode, stdout_bytes=len(stdout_bytes),
                      stdout_sha256=hashlib.sha256(stdout_bytes).hexdigest(), stderr=result.stderr)
        if result.returncode:
            raise RuntimeError(f"Command failed ({result.returncode}): {record['argv']}\n{result.stderr}")
        return result.stdout

    try:
        for name, requested in (("ffmpeg", args.ffmpeg), ("ffprobe", args.ffprobe)):
            executable = Path(shutil.which(requested) or requested).resolve(strict=True)
            version = run([executable, "-version"])
            if not version.startswith(f"{name} version 8.1.3 "):
                raise RuntimeError(f"Expected {name} 8.1.3, got {version.splitlines()[0]}")
            inventory["tools"][name] = {**identity(executable), "version_and_configuration": version}
        ffmpeg = inventory["tools"]["ffmpeg"]["path"]
        ffprobe = inventory["tools"]["ffprobe"]["path"]

        # Query the same FFmpeg ABI used by the production audio planner. A
        # codec name alone does not establish whether the MP4 muxer accepts it.
        libformat = ctypes.CDLL("libavformat.so.62")
        libcodec = ctypes.CDLL("libavcodec.so.62")

        class CodecDescriptor(ctypes.Structure):
            _fields_ = [("id", ctypes.c_int), ("type", ctypes.c_int)]

        libcodec.avcodec_descriptor_get_by_name.argtypes = [ctypes.c_char_p]
        libcodec.avcodec_descriptor_get_by_name.restype = ctypes.POINTER(CodecDescriptor)
        libformat.av_guess_format.argtypes = [ctypes.c_char_p, ctypes.c_char_p, ctypes.c_char_p]
        libformat.av_guess_format.restype = ctypes.c_void_p
        libformat.avformat_query_codec.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int]
        libformat.avformat_query_codec.restype = ctypes.c_int
        mp4_format = libformat.av_guess_format(b"mp4", None, None)
        if not mp4_format:
            raise RuntimeError("loaded FFmpeg library has no MP4 muxer")
        loaded_paths = {Path(line.split()[-1]) for line in Path("/proc/self/maps").read_text().splitlines()
                        if len(line.split()) >= 6 and line.split()[-1].startswith("/")}
        inventory["audio_copy_query_libraries"] = [identity(path) for path in sorted(loaded_paths)
                                                   if path.name.startswith(("libavformat.so.62", "libavcodec.so.62"))]

        def copy_eligibility(codec):
            descriptor = libcodec.avcodec_descriptor_get_by_name(codec.encode("ascii"))
            if not descriptor:
                raise RuntimeError(f"no FFmpeg codec descriptor: {codec}")
            result = libformat.avformat_query_codec(mp4_format, descriptor.contents.id, 0)
            return {"codec": codec, "codec_id": descriptor.contents.id, "target_container": "mp4",
                    "standard_compliance": 0, "query_result": result, "eligible": result > 0,
                    "scope": "FFmpeg MP4 muxer codec query; does not qualify conversion or audio preservation"}

        def generate(name, inputs, options, purpose):
            target = out / name
            run([ffmpeg, "-nostdin", "-hide_banner", "-loglevel", "error", "-n",
                 *inputs, *options, "-map_metadata", "-1", "-fflags", "+bitexact", target])
            # Separate frames/packets so consumers need not handle mixed JSON arrays.
            probes = {}
            for kind, flags in (("streams", ["-show_format", "-show_streams"]),
                                ("frames", ["-show_frames"]), ("packets", ["-show_packets"])):
                probe = json.loads(run([ffprobe, "-v", "error", *flags, "-of", "json", target]))
                if "format" in probe:
                    probe["format"]["filename"] = name
                probe_path = out / f"{name}.{kind}.json"
                save(probe_path, probe)
                probes[kind] = {**identity(probe_path), "path": probe_path.name}
            observed = json.loads((out / f"{name}.streams.json").read_text())
            frames = json.loads((out / f"{name}.frames.json").read_text())["frames"]
            entry = {**identity(target), "path": name, "purpose": purpose,
                     "probes": probes, "observed_streams": observed["streams"],
                     "observed_video_frame_colors": [dict(values) for values in sorted({
                         tuple(sorted((key, frame.get(key, "unknown")) for key in
                                      ("color_range", "color_space", "color_transfer", "color_primaries")))
                         for frame in frames if frame.get("media_type") == "video"})],
                     "observed_frame_side_data_types": sorted({side["side_data_type"]
                         for frame in frames for side in frame.get("side_data_list", [])})}
            entry["audio_copy_eligibility"] = [copy_eligibility(stream["codec_name"])
                                               for stream in observed["streams"] if stream["codec_type"] == "audio"]
            inventory["artifacts"].append(entry)
            return entry

        def video(duration="2", size="128x96", offset="0", sar="1"):
            return ["-f", "lavfi", "-i",
                    f"testsrc2=size={size}:rate=25:duration={duration},setsar={sar},setpts=PTS+{offset}/TB"]

        def audio(rate=48000, channels=2, duration="2", offset="0", frequency=440):
            return ["-f", "lavfi", "-i",
                    f"sine=frequency={frequency}:sample_rate={rate}:duration={duration},"
                    f"aformat=channel_layouts={'mono' if channels == 1 else 'stereo'},asetpts=PTS+{offset}/TB"]

        h264 = ["-c:v", "libx264", "-preset", "fast", "-profile:v", "high", "-pix_fmt", "yuv420p",
                "-threads:v", "1", "-x264-params", "threads=1:lookahead_threads=1:scenecut=0",
                "-bf", "2", "-g", "25", "-color_primaries", "bt709", "-color_trc", "bt709",
                "-colorspace", "bt709", "-color_range", "tv"]
        aac = ["-c:a", "aac", "-b:a", "96k", "-threads:a", "1", "-flags:a", "+bitexact"]
        dual_tags = ["-metadata:s:a:0", "language=eng", "-metadata:s:a:0", "title=English tone",
                     "-metadata:s:a:1", "language=jpn", "-metadata:s:a:1", "title=Japanese tone",
                     "-disposition:a:0", "default", "-disposition:a:1", "0"]
        for name, rate, channels, size in (("aac-44100-mono.mp4", 44100, 1, "128x96"),
                                            ("aac-48000-stereo.mp4", 48000, 2, "128x128")):
            generate(name, video(size=size) + audio(rate, channels),
                     ["-map", "0:v", "-map", "1:a", *h264, *aac], "single AAC stream")
        for name, vd, ad, vo, ao in (("audio-shorter.mp4", "3", "1", "0", "0"),
                                    ("audio-longer.mp4", "1", "3", "0", "0"),
                                    ("video-offset.mp4", "2", "2", "2", "0"),
                                    ("audio-offset.mp4", "2", "2", "0", "2"),
                                    ("both-offset.mp4", "2", "2", "2", "2")):
            generate(name, video(vd, offset=vo) + audio(duration=ad, offset=ao),
                     ["-map", "0:v", "-map", "1:a", *h264, *aac, "-copyts"],
                     "duration/start-time mismatch; inspect actual packet and stream timestamps")
        for name, duration, extra in (("dual-aac.mp4", "2", []),
                                     ("fragmented-aac.mp4", "2", ["-movflags", "+frag_keyframe+empty_moov"]),
                                     ("medium-dual-aac.mp4", "120", [])):
            generate(name, video(duration) + audio(duration=duration) + audio(duration=duration, frequency=660),
                     ["-map", "0:v", "-map", "1:a", "-map", "2:a", *h264, *aac, *dual_tags, *extra],
                     "dual AAC languages, dispositions and titles; MP4 may discard title tags")
        for name, codec_options in (("h264-aac.mkv", h264),
                                    ("hevc-aac.mkv", ["-c:v", "libx265", "-pix_fmt", "yuv420p", "-preset", "ultrafast",
                                                       "-x265-params", "pools=none:frame-threads=1:wpp=0:log-level=error",
                                                       "-color_primaries", "bt709", "-color_trc", "bt709", "-colorspace", "bt709", "-color_range", "tv"]),
                                    ("av1-aac.mkv", ["-c:v", "libaom-av1", "-cpu-used", "8", "-threads:v", "1",
                                                      "-row-mt", "0", "-pix_fmt", "yuv420p", "-color_primaries", "bt709",
                                                      "-color_trc", "bt709", "-colorspace", "bt709", "-color_range", "tv"]),
                                    ("h264-aac.mov", h264)):
            generate(name, video("0.2") + audio(duration="0.2"),
                     ["-map", "0:v", "-map", "1:a", *codec_options, *aac], "container/codec audio-copy candidate")
        flac = generate("flac-copy-candidate.mkv", video() + audio(),
                 ["-map", "0:v", "-map", "1:a", *h264, "-c:a", "flac", "-threads:a", "1"],
                 "FLAC copy candidate: actual MP4 muxer accepts this codec; no production restriction inferred")
        speex = generate("unsupported-speex.mkv", video() + audio(rate=16000, channels=1),
                 ["-map", "0:v", "-map", "1:a", *h264, "-c:a", "libspeex", "-threads:a", "1"],
                 "Speex Matroska source: strict MP4 audio-copy rejection candidate, pending actual CLI inspection")
        if not flac["audio_copy_eligibility"][0]["eligible"] or speex["audio_copy_eligibility"][0]["eligible"]:
            raise RuntimeError("FFmpeg MP4 audio compatibility facts changed: expected FLAC accepted and Speex rejected")
        subtitle = out / "subtitle.srt"
        subtitle.write_text(SUBTITLE, encoding="utf-8")
        inventory["sources"].append(identity(subtitle))
        generate("subtitle.mp4", video() + audio() + ["-i", str(subtitle)],
                 ["-map", "0:v", "-map", "1:a", "-map", "2:s", *h264, *aac, "-c:s", "mov_text"],
                 "extra real subtitle stream")
        generate("sar-2-1.mp4", video(sar="2/1") + audio(),
                 ["-map", "0:v", "-map", "1:a", *h264, *aac], "non-square sample aspect ratio")
        base = out / "aac-44100-mono.mp4"
        generate("rotation-90.mp4", ["-display_rotation:v:0", "90", "-i", base],
                 ["-map", "0", "-c", "copy"], "real display matrix side data")

        fixtures = ROOT / "tests/fixtures/codecs"
        for name, source in (("pq-no-static.mp4", "hevc-main10-pq-c3-legal-v1.mp4"),
                             ("pq-static-64x64.mp4", "hevc-main10-pq-static-metadata.mp4"),
                             ("retained-bt709-pq-conflict.mp4", "hevc-main10-pq-bt709-conflict.mp4")):
            src = fixtures / source
            inventory["sources"].append(identity(src))
            generate(name, ["-i", src], ["-map", "0:v:0", "-an", "-c:v", "copy", "-t", "0.2"],
                     "retained fixture trim; stream/frame/static metadata evidence in probes")
        # Reuse the legal canonical pixels at their qualified probe geometry.
        # Mastering values exactly match the retained 64x64 static fixture;
        # this is another source candidate, never production qualification.
        static_1080 = generate("pq-static.mp4", ["-i", fixtures / "hevc-main10-pq-c3-legal-v1.mp4"],
                               ["-map", "0:v:0", "-an", "-frames:v", "10", "-r", "50",
                                "-c:v", "libx265", "-pix_fmt", "yuv420p10le", "-preset", "ultrafast", "-threads:v", "1",
                                "-color_primaries", "bt2020", "-color_trc", "smpte2084", "-colorspace", "bt2020nc",
                                "-color_range", "tv", "-chroma_sample_location", "left",
                                "-x265-params", "pools=none:frame-threads=1:wpp=0:log-level=error:lossless=1:"
                                "bframes=0:keyint=10:min-keyint=10:open-gop=0:"
                                "colorprim=9:transfer=16:colormatrix=9:range=limited:chromaloc=0:"
                                "master-display=G(13250,34500)B(7500,3000)R(34000,16000)"
                                "WP(15635,16450)L(10000000,50):max-cll=1000,400:hdr10=1"],
                               "1920x1080 PQ10 static metadata candidate from retained C3 legal canonical pixels")
        static_stream = static_1080["observed_streams"][0]
        expected_static_colors = {"color_primaries": "bt2020", "color_transfer": "smpte2084",
                                  "color_space": "bt2020nc", "color_range": "tv"}
        if ((static_stream["width"], static_stream["height"], static_stream["pix_fmt"], static_stream["r_frame_rate"])
                != (1920, 1080, "yuv420p10le", "50/1")
                or any(static_stream.get(key) != value for key, value in expected_static_colors.items())):
            raise RuntimeError("1080p static PQ candidate geometry/color did not survive encoding")
        static_frames = json.loads((out / "pq-static.mp4.frames.json").read_text())["frames"]
        if len(static_frames) != 10 or any(not {"Mastering display metadata", "Content light level metadata"}.issubset(
                {side["side_data_type"] for side in frame.get("side_data_list", [])}) for frame in static_frames):
            raise RuntimeError("1080p static PQ candidate did not retain both static metadata records on all ten decoded frames")
        src = fixtures / "hevc-main10-canonical-v1.mp4"
        inventory["sources"].append(identity(src))
        fields = (("primaries", "colour_primaries", "color_primaries"),
                  ("transfer", "transfer_characteristics", "color_transfer"),
                  ("matrix", "matrix_coefficients", "color_space"))
        for label, header, field in fields:
            container = {"primaries": "-color_primaries", "transfer": "-color_trc", "matrix": "-colorspace"}[label]
            entry = generate(f"missing-{label}.mp4", ["-i", src],
                             ["-map", "0:v:0", "-an", "-c:v", "copy", "-t", "0.2",
                              "-bsf:v", f"hevc_metadata={header}=2", container, "unknown"],
                             f"STRICT 10-bit missing {field} candidate")
            if entry["observed_streams"][0].get(field, "unknown") != "unknown":
                raise RuntimeError(f"{label} missing-field construction did not survive mux")
        entry = generate("missing-range-attempt.mp4", ["-i", src],
                         ["-map", "0:v:0", "-an", "-c:v", "copy", "-t", "0.2", "-color_range", "unknown"],
                         "range removal attempt; candidate only if probes actually report unknown")
        if entry["observed_streams"][0].get("color_range") in ("tv", "pc"):
            inventory["limitations"].append("Missing range blocked: hevc_metadata cannot remove video_signal_type_present_flag; container unknown preserves bitstream range.")
        conflict = generate("stream-frame-conflict-attempt.mp4", ["-i", fixtures / "hevc-main10-pq-c3-legal-v1.mp4"],
                            ["-map", "0:v:0", "-an", "-c:v", "copy", "-t", "0.2",
                             "-color_primaries", "bt709", "-color_trc", "bt709", "-colorspace", "bt709"],
                            "container BT.709 vs retained PQ header attempt; compare observed stream/frame fields")
        stream_colors = {key: conflict["observed_streams"][0].get(key, "unknown") for key in
                         ("color_range", "color_space", "color_transfer", "color_primaries")}
        if all(colors == stream_colors for colors in conflict["observed_video_frame_colors"]):
            inventory["limitations"].append("Stream/frame conflict attempt did not survive probing: decoded frame and stream colors agree; do not promote as a conflict case.")
        static = fixtures / "hevc-main10-pq-static-metadata.mp4"
        entry = generate("sdr-with-hdr-sei.mp4", ["-i", static],
                         ["-map", "0:v:0", "-an", "-c:v", "copy", "-t", "0.2",
                          "-bsf:v", "hevc_metadata=colour_primaries=1:transfer_characteristics=1:matrix_coefficients=1",
                          "-color_primaries", "bt709", "-color_trc", "bt709", "-colorspace", "bt709"],
                         "SDR color header with retained mastering/MaxCLL SEI; no new HDR encode")
        dual = next(artifact for artifact in inventory["artifacts"] if artifact["path"] == "dual-aac.mp4")
        if any("title" not in stream.get("tags", {}) for stream in dual["observed_streams"]
               if stream.get("codec_type") == "audio"):
            inventory["limitations"].append("MP4 dual-audio title is absent as a title tag; this muxer exposes the requested text as the name tag. Read observed tags rather than assuming title survived.")
        inventory["completion"] = "generated"
    finally:
        save(out / "inventory.json", inventory)


if __name__ == "__main__":
    main()
