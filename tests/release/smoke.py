#!/usr/bin/env python3
"""D-2 short, actual-artifact checks; never a replacement for media qualification."""
import argparse
from fractions import Fraction
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[2]
sys.dont_write_bytecode = True
sys.path.insert(0, str(ROOT / "tests/soak"))
from generate import generate


def sha(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--second-build", action="store_true")
    parser.add_argument("--lgpl-prebuilt", action="store_true")
    parser.add_argument("--version", default="2.0.0-rc.1")
    args = parser.parse_args()
    binary, output = args.binary.resolve(), args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    receipt = dict(binary=str(binary), sha256=sha(binary), checks=[], status="RUNNING")

    def save():
        (output / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")

    def run(name, argv, expected=0, cwd=None, env=None):
        argv = [str(value) for value in argv]
        result = subprocess.run(argv, cwd=cwd, env=env, capture_output=True,
                                text=True, timeout=180)
        row = dict(name=name, argv=argv, cwd=str(cwd or Path.cwd()),
                   environment_overrides=env and {k: v for k, v in env.items()
                                                  if os.environ.get(k) != v},
                   exit_status=result.returncode, stdout=result.stdout,
                   stderr=result.stderr)
        receipt["checks"].append(row)
        save()
        assert result.returncode == expected, row
        assert "panicked at" not in result.stderr and "stack backtrace:" not in result.stderr, row
        if argv[0] == str(binary) and expected == 0:
            assert not any(term in result.stderr for term in ("ASCIIFLOW_TEST_", "resource_sample", "private_anon_kib", "fault injection")), row
            assert not any(term in result.stdout for term in ("ms/frame", "Encode characterization", "Mux characterization", "resource_sample")), row
        return result.stdout

    def probe(path, packets=False):
        flags = ["-show_packets", "-show_data_hash", "sha256"] if packets else ["-show_streams", "-show_format"]
        return json.loads(run(f"probe:{path.name}", ["ffprobe", "-v", "error", *flags,
                                                   "-of", "json", path]))

    def command(source, target, codec="h264", depth=8, dynamic="preserve", audio="none", gpu=True):
        return [binary, source, target, "--width", "80", "--charset", "standard",
                "--font", "builtin-8x8", "--color", "true", "--audio", audio,
                "--decode", "vaapi" if gpu else "software", "--backend", "vulkan" if gpu else "cpu",
                "--encode", "vaapi" if gpu else "software", "--output-codec", codec,
                "--output-bit-depth", str(depth), "--output-dynamic-range", dynamic, "--no-progress",
                *(["--vulkan-mapping", "gpu", "--hw-device", "/dev/dri/renderD128",
                   "--vaapi-vulkan-input-interop", "on", "--vaapi-vulkan-output-interop", "on"] if gpu else [])]

    def override(argv, option, value):
        argv[argv.index(option) + 1] = value
        return argv

    def reject(name, argv, target, stage, category, fragment, env=None, code=None):
        report = output / f"failure-{name}.json"
        prior = sha(target) if target.exists() else None
        run(name, argv + ["--diagnostic-report", report], expected=1, env=env)
        failure = json.loads(report.read_text())["failure"]
        assert failure["stage"] == stage and failure["category"] == category, failure
        assert fragment in failure["message"], failure
        if code:
            assert failure["code"] == code, failure
        assert (sha(target) == prior if prior else not target.exists())
        assert not list(target.parent.glob(f".{target.stem}.asciiflow-part-*.mp4"))
        receipt.setdefault("failures", {})[name] = failure
        save()

    def verify(name, source, target, depth, pq, audio):
        document = probe(target)
        streams = document["streams"]
        video = next(s for s in streams if s["codec_type"] == "video")
        expected = dict(width=1920, height=1080, nb_frames="300", avg_frame_rate="50/1",
                        codec_name="hevc" if depth == 10 else "h264",
                        pix_fmt="yuv420p10le" if depth == 10 else "yuv420p", color_range="tv",
                        color_primaries="bt2020" if pq else "bt709",
                        color_transfer="smpte2084" if pq else "bt709",
                        color_space="bt2020nc" if pq else "bt709")
        assert all(video.get(key) == value for key, value in expected.items()), (name, video)
        if depth == 10:
            assert video["profile"] == "Main 10", video
        assert Fraction(video["duration"]) == 6
        assert Fraction(document["format"]["duration"]) == 6
        assert len([s for s in streams if s["codec_type"] == "audio"]) == audio
        frames = json.loads(run(f"frames:{name}", ["ffprobe", "-v", "error", "-select_streams", "v:0",
                            "-show_frames", "-show_entries", "frame=best_effort_timestamp_time,side_data_list",
                            "-of", "json", target]))["frames"]
        assert len(frames) == 300
        assert all(Fraction(frame["best_effort_timestamp_time"]) == Fraction(index, 50)
                   for index, frame in enumerate(frames))
        if not pq:
            assert not any("Mastering display" in json.dumps(item) or "Content light" in json.dumps(item)
                           for item in [video, *frames])
        packets = probe(target, True)["packets"]
        vp = [p for p in packets if p["stream_index"] == video["index"]]
        assert len(vp) == 300
        tb = Fraction(video["time_base"])
        assert all(int(p["pts"]) * tb == Fraction(i, 50) and int(p["duration"]) * tb == Fraction(1, 50)
                   for i, p in enumerate(vp))
        assert all(int(a["dts"]) < int(b["dts"]) for a, b in zip(vp, vp[1:]))
        # Decode all video/audio, fail on any decode error, not just header probing.
        run(f"decode:{name}", ["ffmpeg", "-v", "error", "-xerror", "-i", target,
                              "-map", "0", "-f", "null", "-"])
        if audio:
            before, after = probe(source), document
            bp, ap = probe(source, True)["packets"], packets
            for index, (ins, outs) in enumerate(zip([s for s in before["streams"] if s["codec_type"] == "audio"],
                                                    [s for s in after["streams"] if s["codec_type"] == "audio"])):
                for key in ("codec_name", "sample_rate", "channels"):
                    assert ins[key] == outs[key], key
                assert ins["codec_name"] == "aac"
                assert ins.get("tags", {}).get("language") == outs.get("tags", {}).get("language")
                title = lambda stream: stream.get("tags", {}).get("title") or stream.get("tags", {}).get("name")
                assert title(ins) == title(outs) == f"Soak track {index + 1}"
                assert ins["disposition"]["default"] == outs["disposition"]["default"]
                left = [p for p in bp if p["stream_index"] == ins["index"]]
                right = [p for p in ap if p["stream_index"] == outs["index"]]
                assert len(left) == len(right)
                for a, b in zip(left, right):
                    assert a["data_hash"] == b["data_hash"]
                    for key in ("pts", "dts", "duration"):
                        assert int(a[key]) * Fraction(ins["time_base"]) == int(b[key]) * Fraction(outs["time_base"])
        receipt.setdefault("outputs", {})[name] = dict(path=str(target), sha256=sha(target),
                                                       probe=document, decoded_frames=len(frames),
                                                       timestamps="exact i/50", audio_copy="PASS" if audio else "none")
        save()

    try:
        receipt["version"] = run("version", [binary, "--version"])
        assert receipt["version"].strip() == f"asciiflow {args.version}"
        run("help", [binary, "--help"])
        recipes = [("A", "sdr", 1, "h264", 8, "preserve"),
                   ("B", "pq-preserve", 0, "hevc", 10, "preserve"),
                   ("C", "pq-to-sdr", 2, "hevc", 10, "sdr")]
        if args.second_build:
            recipes = recipes[:1]
        inputs = {}
        for name, kind, audio, codec, depth, dynamic in recipes:
            identity = generate(kind, 300, output / f"input-{name}", audio)
            source = Path(identity["output"]["path"])
            inputs[name] = source
            target = output / f"{name}.mp4"
            run(f"smoke:{name}", command(source, target, codec, depth, dynamic, "copy" if audio else "none"), cwd=output)
            verify(name, source, target, depth, name == "B", audio)
        if args.second_build:
            receipt["status"] = "PASS"
            save()
            return
        source = inputs["A"]
        target = output / "software.mp4"
        software_command = command(source, target, gpu=False)
        if args.lgpl_prebuilt:
            reject("software-h264-excluded", software_command, target, "Planning", "InvalidConfig", "excluded from the official LGPL prebuilt")
            software_command = override(software_command, "--encode", "vaapi")
            software_command += ["--hw-device", "/dev/dri/renderD128"]
        run("software-decode" if args.lgpl_prebuilt else "software", software_command, cwd="/tmp")
        verify("software", source, target, 8, False, 0)
        for index, cwd in enumerate((binary.parent, Path("/tmp"), Path.home())):
            target = output / f"cwd-{index}.mp4"
            run(f"cwd:{index}", command(source, target), cwd=cwd)
            verify(f"cwd-{index}", source, target, 8, False, 0)
        # Existing targets are preserved on failure and atomically replaced on success.
        existing = output / "existing.mp4"
        existing.write_bytes(b"D2 existing target sentinel\n")
        original = sha(existing)
        bad = output / "bad-input"
        bad.write_bytes(b"not media\n")
        for name, argv, env, stage, category, fragment in [
            ("bad-input", command(bad, existing), None, "InputProbe", "Media", "open input"),
            ("unwritable-output", command(source, Path("/proc/asciiflow-d2-no-write.mp4")), None, None, "Other", "failed to reserve temporary output"),
            ("missing-vaapi", override(command(source, output / "missing-vaapi.mp4"), "--hw-device", "/dev/dri/asciiflow-missing"), None, "Planning", "InvalidConfig", "VAAPI device unavailable"),
            ("missing-vulkan", command(source, output / "missing-vulkan.mp4"), dict(os.environ, VK_DRIVER_FILES="/nonexistent/asciiflow-icd.json", VK_ICD_FILENAMES="/nonexistent/asciiflow-icd.json"), "Planning", "InvalidConfig", "Vulkan processing unavailable"),
            ("missing-driver", command(source, output / "missing-driver.mp4"), dict(os.environ, LIBVA_DRIVER_NAME="asciiflow_missing_driver", LIBVA_DRIVERS_PATH=str(output)), "Planning", "InvalidConfig", "VAAPI device unavailable"),
            ("missing-font", override(command(source, output / "missing-font.mp4"), "--font", "/nonexistent/asciiflow-font.ttf"), None, None, "Other", "read font file"),
        ]:
            reject(name, argv, Path(argv[2]), stage, category, fragment, env)
        assert sha(existing) == original
        for name, file, extra, stage, category, fragment, code in [
            ("HLG", "hevc-main10-hlg.mp4", [], "InputProbe", "UnsupportedColor", "HDR HLG", "UnsupportedHdrHlg"),
            ("full-range", "hevc-main10-sdr-full.mp4", [], "InputProbe", "UnsupportedColor", "full-range", "UnsupportedFullRange"),
            ("bt2020-sdr", "hevc-main10-bt2020-sdr.mp4", [], "InputProbe", "UnsupportedColor", "wide-gamut SDR", "UnsupportedWideGamutSdr"),
            ("depth-conversion", "hevc-main10-canonical-v1.mp4", [], "Planning", "UnsupportedFrame", "without a conversion path", None),
            ("PQ-H264-preserve", "hevc-main10-pq-canonical-v1.mp4", [], "Planning", "InvalidConfig", "PQ preservation requires", None),
            ("H264-10", None, ["--output-bit-depth", "10"], "Planning", "InvalidConfig", "10-bit H.264 output is not implemented", None),
        ]:
            rejected = output / f"reject-{name}.mp4"
            argv = command(ROOT / "tests/fixtures/codecs" / file if file else source, rejected)
            if extra:
                argv = override(argv, extra[0], extra[1])
            reject(f"unsupported-{name}", argv, rejected, stage, category, fragment, code=code)
            assert not rejected.exists()
        run("existing-success", command(source, existing))
        verify("existing-success", source, existing, 8, False, 0)
        # Cancel a real long-enough job after it has written staging output.
        identity = generate("pq-to-sdr", 3000, output / "input-cancel", 2)
        target = output / "cancel.mp4"
        argv = [str(x) for x in command(Path(identity["output"]["path"]), target, "hevc", 10, "sdr", "copy")]
        with (output / "cancel.stdout").open("w") as stdout, (output / "cancel.stderr").open("w") as stderr:
            process = subprocess.Popen(argv, stdout=stdout, stderr=stderr, start_new_session=True)
            try:
                deadline = time.monotonic() + 60
                staging = []
                while time.monotonic() < deadline and process.poll() is None:
                    staging = list(output.glob(".cancel.asciiflow-part-*.mp4"))
                    if any(p.stat().st_size > 32768 for p in staging):
                        break
                    time.sleep(0.05)
                progress_bytes = sum(p.stat().st_size for p in staging)
                assert process.poll() is None and progress_bytes > 32768, "no demonstrated job progress before SIGINT"
                os.killpg(process.pid, signal.SIGINT)
                status = process.wait(timeout=30)
            finally:
                if process.poll() is None:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait(timeout=10)
        receipt["signal"] = dict(argv=argv, exit_status=status, observed_staging=[str(p) for p in staging], progress_bytes=progress_bytes,
                                 stderr=(output / "cancel.stderr").read_text())
        assert status == 130, receipt["signal"]
        assert not target.exists()
        assert not [p for p in staging if p.exists()]
        assert not list(output.glob(".*.asciiflow-part-*.mp4"))
        receipt["status"] = "PASS"
    except Exception as error:
        receipt["status"] = "FAIL"
        receipt["error"] = repr(error)
        raise
    finally:
        save()


if __name__ == "__main__":
    main()
