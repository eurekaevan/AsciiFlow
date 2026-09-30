//! Opt-in C4B checks of the real CLI and Intel VAAPI/Vulkan production path.
//! FFmpeg software decoding is exclusively a post-conversion output oracle.
#[allow(dead_code)]
mod audio_support;

use audio_support::{Process, Stream, Workspace, audio, inspect, same_audio, success};
use std::{
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::{Mutex, MutexGuard},
};

static HARDWARE_TEST: Mutex<()> = Mutex::new(());

fn hardware_lock() -> MutexGuard<'static, ()> {
    // The lock serializes independent child processes, not shared test state.
    // Preserve an earlier test failure without turning it into unrelated failures.
    HARDWARE_TEST
        .lock()
        .unwrap_or_else(|error| error.into_inner())
}

fn require_environment() {
    assert_eq!(
        std::env::var("ASCIIFLOW_C4B_PRODUCTION").as_deref(),
        Ok("1"),
        "set ASCIIFLOW_C4B_PRODUCTION=1 only on qualified Intel VAAPI/Vulkan hardware"
    );
}

fn fixture(codec: &str) -> PathBuf {
    let root = std::env::var_os("ASCIIFLOW_C4B_FIXTURE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/codecs")
        });
    let path = root.join(format!("{codec}-main10-pq-c3-legal-v1.mp4"));
    assert!(
        path.is_file(),
        "missing C3 legal fixture: {}",
        path.display()
    );
    path
}

fn ffmpeg() -> Command {
    Command::new(std::env::var_os("ASCIIFLOW_FIXTURE_FFMPEG").unwrap_or_else(|| "ffmpeg".into()))
}

fn with_audio(ws: &Workspace, input: &Path, tracks: usize) -> PathBuf {
    assert!((1..=2).contains(&tracks));
    let version = ffmpeg().arg("-version").output().unwrap();
    success(&version);
    assert!(
        String::from_utf8_lossy(&version.stdout).starts_with("ffmpeg version 8.1.3 "),
        "AAC fixture mux requires fixed FFmpeg 8.1.3"
    );
    let path = ws.0.join(format!("c4b-{tracks}-aac.mp4"));
    let mut cmd = ffmpeg();
    cmd.args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y", "-i"])
        .arg(input);
    for frequency in [440, 880].into_iter().take(tracks) {
        cmd.args(["-f", "lavfi", "-i"]).arg(format!(
            "sine=frequency={frequency}:sample_rate=48000:duration=6"
        ));
    }
    cmd.args(["-map", "0:v:0"]);
    for track in 1..=tracks {
        cmd.args(["-map", &format!("{track}:a:0")]);
    }
    cmd.args([
        "-c:v",
        "copy",
        "-c:a",
        "aac",
        "-b:a",
        "96k",
        "-threads",
        "1",
        "-metadata:s:a:0",
        "language=eng",
        "-disposition:a:0",
        "default",
    ]);
    if tracks == 2 {
        cmd.args(["-metadata:s:a:1", "language=jpn", "-disposition:a:1", "0"]);
    }
    success(&cmd.arg(&path).output().unwrap());
    // Muxing must retain the qualified coded video byte-for-byte.
    let before = inspect(input);
    let after = inspect(&path);
    let packets = |streams: Vec<Stream>| {
        streams
            .into_iter()
            .find(|s| s.video)
            .unwrap()
            .packets
            .into_iter()
            .map(|p| p.data)
            .collect::<Vec<_>>()
    };
    assert_eq!(packets(before), packets(after));
    path
}

// Keep each qualification matrix axis explicit at the call site.
#[allow(clippy::too_many_arguments)]
fn command(
    input: &Path,
    output: &Path,
    codec: &str,
    depth: &str,
    range: &str,
    audio_policy: &str,
    font: &Path,
    color: bool,
    frames: u32,
    automatic: bool,
) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_asciiflow"));
    cmd.arg(input).arg(output).args([
        "--output-codec",
        codec,
        "--output-bit-depth",
        depth,
        "--output-dynamic-range",
        range,
        "--width",
        "80",
        "--charset",
        "standard",
        "--audio",
        audio_policy,
        "--color",
        if color { "true" } else { "false" },
        "--no-progress",
        "--verbose",
    ]);
    if !automatic {
        cmd.args([
            "--backend",
            "vulkan",
            "--decode",
            "vaapi",
            "--encode",
            "vaapi",
            "--vaapi-vulkan-input-interop",
            "on",
            "--vaapi-vulkan-output-interop",
            "on",
        ]);
    }
    cmd.args(["--hw-device", "/dev/dri/renderD128", "--font"])
        .arg(font)
        .arg("--max-frames")
        .arg(frames.to_string())
        .env("ASCIIFLOW_VULKAN_VALIDATION", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd
}

fn completed(cmd: &mut Command) -> Output {
    let result = Process::start(cmd).finish();
    success(&result);
    assert_hardware_path(&result);
    result
}

fn assert_hardware_path(result: &Output) {
    let plan = String::from_utf8_lossy(&result.stdout);
    for required in [
        "GPU + DMA-BUF input/output",
        "VAAPI decode",
        "VAAPI encode",
        "fallback count 0",
    ] {
        assert!(plan.contains(required), "missing {required}: {plan}");
    }
    for forbidden in [
        "software decode",
        "software encode",
        "CPU ASCII",
        "hwdownload",
        "Replanned:",
    ] {
        assert!(
            !plan.contains(forbidden),
            "unexpected fallback {forbidden}: {plan}"
        );
    }
    let diagnostics = String::from_utf8_lossy(&result.stderr);
    assert!(
        !diagnostics.contains("VUID-") && !diagnostics.contains("Validation Error"),
        "{diagnostics}"
    );
}

fn assert_video(path: &Path, codec: &str, depth: &str, range: &str, frames: usize) {
    let probe = Command::new("ffprobe").args(["-v", "error", "-select_streams", "v:0",
        "-count_frames", "-show_entries", "stream=codec_name,width,height,pix_fmt,color_range,color_space,color_transfer,color_primaries,r_frame_rate,nb_read_frames",
        "-of", "default=noprint_wrappers=1"]).arg(path).output().unwrap();
    success(&probe);
    let metadata = String::from_utf8(probe.stdout).unwrap();
    let expected = if range == "sdr" {
        [
            "color_space=bt709",
            "color_transfer=bt709",
            "color_primaries=bt709",
        ]
    } else {
        [
            "color_space=bt2020nc",
            "color_transfer=smpte2084",
            "color_primaries=bt2020",
        ]
    };
    let pix_fmt = if depth == "8" {
        "pix_fmt=yuv420p"
    } else {
        "pix_fmt=yuv420p10le"
    };
    for value in [
        format!("codec_name={codec}"),
        "width=1920".into(),
        "height=1080".into(),
        "r_frame_rate=50/1".into(),
        "color_range=tv".into(),
        pix_fmt.into(),
        format!("nb_read_frames={frames}"),
    ]
    .into_iter()
    .chain(expected.into_iter().map(str::to_owned))
    {
        assert!(
            metadata.lines().any(|line| line == value),
            "missing {value}: {metadata}"
        );
    }
    success(
        &ffmpeg()
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-nostdin",
                "-threads",
                "1",
                "-i",
            ])
            .arg(path)
            .args(["-map", "0:v:0", "-f", "null", "-"])
            .output()
            .unwrap(),
    );
    let streams = inspect(path);
    let video = streams.iter().find(|s| s.video).unwrap();
    assert_eq!(video.packets.len(), frames);
    for pair in video.packets.windows(2) {
        assert!(pair[1].pts > pair[0].pts);
    }
}

fn assert_audio_exact(before: &[Stream], after: &[Stream]) {
    assert_eq!(
        before
            .iter()
            .map(|s| (s.video, s.audio))
            .collect::<Vec<_>>(),
        after.iter().map(|s| (s.video, s.audio)).collect::<Vec<_>>(),
        "stream order changed"
    );
    for (input, output) in audio(before).into_iter().zip(audio(after)) {
        same_audio(input, output);
        for (a, b) in input.packets.iter().zip(&output.packets) {
            for (x, y) in [(a.pts, b.pts), (a.dts, b.dts), (a.duration, b.duration)] {
                assert_eq!(
                    i128::from(x) * i128::from(input.num) * i128::from(output.den),
                    i128::from(y) * i128::from(output.num) * i128::from(input.den),
                    "audio timestamp changed"
                );
            }
        }
    }
}

#[test]
#[ignore = "requires ASCIIFLOW_C4B_PRODUCTION=1, C3 legal fixtures, fixed FFmpeg and qualified Intel full interop"]
fn c4b_single_dual_aac_auto_copy_none_and_preserve_sdr() {
    let _serial = hardware_lock();
    require_environment();
    for input_codec in ["hevc", "av1"] {
        for tracks in [1, 2] {
            let source = Workspace::new();
            let input = with_audio(&source, &fixture(input_codec), tracks);
            let before = inspect(&input);
            for (codec, depth, range) in [
                ("h264", "8", "sdr"),
                ("hevc", "10", "sdr"),
                (input_codec, "10", "preserve"),
            ] {
                for policy in ["auto", "copy", "none"] {
                    let ws = Workspace::new();
                    completed(&mut command(
                        &input,
                        &ws.output(),
                        codec,
                        depth,
                        range,
                        policy,
                        Path::new("builtin-8x8"),
                        true,
                        300,
                        false,
                    ));
                    assert_video(&ws.output(), codec, depth, range, 300);
                    let after = inspect(&ws.output());
                    if policy == "none" {
                        assert!(audio(&after).is_empty());
                    } else {
                        assert_eq!(audio(&after).len(), tracks);
                        assert_audio_exact(&before, &after);
                        audio_support::assert_audio_decodes(&ws.output());
                    }
                    ws.assert_no_staging();
                }
            }
        }
    }
}

#[test]
#[ignore = "requires ASCIIFLOW_C4B_PRODUCTION=1, C3 legal fixtures, FreeType and qualified Intel full interop"]
fn c4b_auto_selection_builtin_freetype_color_mono_nv12_p010() {
    let _serial = hardware_lock();
    require_environment();
    let font = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/fonts/Inconsolata-Regular.ttf");
    assert!(font.is_file());
    for input_codec in ["hevc", "av1"] {
        for (codec, depth) in [("h264", "8"), ("hevc", "10"), ("av1", "10")] {
            for font in [Path::new("builtin-8x8"), font.as_path()] {
                for color in [true, false] {
                    let ws = Workspace::new();
                    completed(&mut command(
                        &fixture(input_codec),
                        &ws.output(),
                        codec,
                        depth,
                        "sdr",
                        "none",
                        font,
                        color,
                        3,
                        true,
                    ));
                    assert_video(&ws.output(), codec, depth, "sdr", 3);
                    ws.assert_no_staging();
                }
            }
        }
    }
}

#[test]
#[ignore = "requires ASCIIFLOW_C4B_PRODUCTION=1 and C3 legal fixtures"]
fn c4b_default_preserve_rejects_pq_h264_before_output_staging() {
    let _serial = hardware_lock();
    require_environment();
    for codec in ["hevc", "av1"] {
        let ws = Workspace::new();
        std::fs::write(ws.output(), b"existing-output").unwrap();
        let result = Process::start(
            Command::new(env!("CARGO_BIN_EXE_asciiflow"))
                .arg(fixture(codec))
                .arg(ws.output())
                .args(["--output-codec", "h264", "--no-progress"])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped()),
        )
        .finish();
        assert!(!result.status.success());
        let diagnostics = String::from_utf8_lossy(&result.stderr);
        assert!(
            diagnostics.contains("PQ") || diagnostics.contains("pq"),
            "{diagnostics}"
        );
        assert_eq!(std::fs::read(ws.output()).unwrap(), b"existing-output");
        ws.assert_no_staging();
    }
}

#[test]
#[ignore = "requires ASCIIFLOW_C4B_PRODUCTION=1, fixed FFmpeg x265 and qualified Intel full interop"]
fn c4b_late_source_highlight_fails_before_black_glyph_can_hide_it() {
    let _serial = hardware_lock();
    require_environment();
    let source = Workspace::new();
    let input = source.0.join("late-out-of-domain-pq.mp4");
    let version = ffmpeg().arg("-version").output().unwrap();
    success(&version);
    assert!(String::from_utf8_lossy(&version.stdout).starts_with("ffmpeg version 8.1.3 "));
    let filter = "nullsrc=s=128x128:r=50,format=yuv420p10le,geq=lum='if(eq(N,10)*lt(X,2)*lt(Y,2),940,64)':cb=512:cr=512";
    success(&ffmpeg().args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y",
        "-f", "lavfi", "-i", filter, "-frames:v", "20", "-an", "-c:v", "libx265",
        "-preset", "ultrafast", "-threads", "1", "-x265-params",
        "lossless=1:bframes=0:keyint=20:min-keyint=20:open-gop=0:pools=none:frame-threads=1:wpp=0:log-level=error",
        "-pix_fmt", "yuv420p10le", "-color_primaries", "bt2020", "-color_trc", "smpte2084",
        "-colorspace", "bt2020nc", "-color_range", "tv", "-chroma_sample_location", "left",
        "-bsf:v", "hevc_metadata=video_full_range_flag=0:colour_primaries=9:transfer_characteristics=16:matrix_coefficients=9:chroma_sample_loc_type=0"])
        .arg(&input).output().unwrap());
    for (codec, depth) in [("h264", "8"), ("hevc", "10")] {
        let build = |output: &Path, frames: u32| {
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_asciiflow"));
            cmd.arg(&input)
                .arg(output)
                .args([
                    "--backend",
                    "vulkan",
                    "--decode",
                    "vaapi",
                    "--encode",
                    "vaapi",
                    "--output-codec",
                    codec,
                    "--output-bit-depth",
                    depth,
                    "--output-dynamic-range",
                    "sdr",
                    "--vaapi-vulkan-input-interop",
                    "on",
                    "--vaapi-vulkan-output-interop",
                    "on",
                    "--hw-device",
                    "/dev/dri/renderD128",
                    "--width",
                    "1",
                    "--charset",
                    " ",
                    "--font",
                    "builtin-8x8",
                    "--color",
                    "true",
                    "--audio",
                    "none",
                    "--no-progress",
                    "--verbose",
                ])
                .arg("--max-frames")
                .arg(frames.to_string())
                .env("ASCIIFLOW_VULKAN_VALIDATION", "1")
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            cmd
        };
        // Frames 0–9 are legal black. Prove those succeed with the identical
        // black glyph configuration before attributing any failure to frame 10.
        let legal = Workspace::new();
        completed(&mut build(&legal.output(), 10));
        assert_eq!(
            inspect(&legal.output())
                .iter()
                .find(|s| s.video)
                .unwrap()
                .packets
                .len(),
            10
        );
        legal.assert_no_staging();
        let ws = Workspace::new();
        std::fs::write(ws.output(), b"existing-output").unwrap();
        let mut cmd = build(&ws.output(), 20);
        let result = Process::start(&mut cmd).finish();
        assert!(!result.status.success());
        let diagnostic = String::from_utf8_lossy(&result.stderr);
        assert!(
            diagnostic.contains("0–1000")
                && diagnostic
                    .contains("pipeline output interop runtime: process output DMA-BUF failed")
                && diagnostic.contains("C-3 invalid GPU input/arithmetic diagnostics"),
            "{diagnostic}"
        );
        assert!(
            !diagnostic.contains("VUID-") && !diagnostic.contains("Validation Error"),
            "{diagnostic}"
        );
        assert_eq!(std::fs::read(ws.output()).unwrap(), b"existing-output");
        ws.assert_no_staging();
    }
}

#[test]
#[ignore = "requires ASCIIFLOW_C4B_PRODUCTION=1, C3 legal fixtures and qualified Intel full interop"]
fn c4b_final_commit_failure_preserves_directory_destination_and_removes_staging() {
    let _serial = hardware_lock();
    require_environment();
    for (codec, depth) in [("h264", "8"), ("hevc", "10")] {
        let ws = Workspace::new();
        std::fs::create_dir(ws.output()).unwrap();
        let sentinel = ws.output().join("sentinel");
        std::fs::write(&sentinel, b"existing-directory-content").unwrap();
        let result = Process::start(&mut command(
            &fixture("hevc"),
            &ws.output(),
            codec,
            depth,
            "sdr",
            "none",
            Path::new("builtin-8x8"),
            true,
            3,
            false,
        ))
        .finish();
        assert!(!result.status.success());
        assert_hardware_path(&result);
        let diagnostic = String::from_utf8_lossy(&result.stderr);
        // commit_output runs only after successful pipeline drain and encoder
        // finalization. A nonempty directory deterministically rejects rename.
        assert!(
            diagnostic.contains("failed to atomically commit output"),
            "{diagnostic}"
        );
        assert!(ws.output().is_dir());
        assert_eq!(
            std::fs::read(&sentinel).unwrap(),
            b"existing-directory-content"
        );
        assert_eq!(std::fs::read_dir(ws.output()).unwrap().count(), 1);
        ws.assert_no_staging();
    }
}

#[test]
#[ignore = "requires ASCIIFLOW_C4B_PRODUCTION=1 and C3 legal fixtures for real CLI planner rejection"]
fn c4b_forced_unqualified_paths_and_h264_ten_bit_fail_before_staging() {
    let _serial = hardware_lock();
    require_environment();
    for (option, value, expected) in [
        ("--backend", "cpu", "HDR PQ→SDR requires"),
        ("--decode", "software", "HDR PQ→SDR requires"),
        ("--encode", "software", "HDR PQ→SDR requires"),
        (
            "--output-bit-depth",
            "10",
            "10-bit H.264 output is not implemented; use HEVC or AV1",
        ),
    ] {
        let ws = Workspace::new();
        std::fs::write(ws.output(), b"existing-output").unwrap();
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_asciiflow"));
        cmd.arg(fixture("hevc"))
            .arg(ws.output())
            .args([
                "--output-codec",
                "h264",
                "--output-dynamic-range",
                "sdr",
                "--audio",
                "none",
                "--max-frames",
                "3",
                "--no-progress",
            ])
            .args([option, value])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let result = Process::start(&mut cmd).finish();
        assert!(!result.status.success());
        let diagnostic = String::from_utf8_lossy(&result.stderr);
        assert!(
            diagnostic.contains(expected),
            "{option} {value}: {diagnostic}"
        );
        assert!(
            !String::from_utf8_lossy(&result.stdout).contains("Media plan"),
            "unqualified request reached production execution"
        );
        assert_eq!(std::fs::read(ws.output()).unwrap(), b"existing-output");
        ws.assert_no_staging();
    }
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "requires ASCIIFLOW_C4B_PRODUCTION=1, C3 fixtures, Intel full interop and SIGINT lifecycle validation"]
fn c4b_sigint_nv12_p010_preserves_destination_and_releases_process_resources() {
    use std::time::{Duration, Instant};
    let _serial = hardware_lock();
    require_environment();
    let source = Workspace::new();
    let input = with_audio(&source, &fixture("hevc"), 1);
    let long = source.0.join("c4b-loop.mp4");
    success(
        &ffmpeg()
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-nostdin",
                "-y",
                "-stream_loop",
                "9",
                "-i",
            ])
            .arg(&input)
            .args(["-map", "0", "-c", "copy"])
            .arg(&long)
            .output()
            .unwrap(),
    );
    for (codec, depth) in [("h264", "8"), ("hevc", "10")] {
        let ws = Workspace::new();
        std::fs::write(ws.output(), b"existing-output").unwrap();
        let mut cmd = command(
            &long,
            &ws.output(),
            codec,
            depth,
            "sdr",
            "copy",
            Path::new("builtin-8x8"),
            true,
            3000,
            false,
        );
        let process = Process::start(&mut cmd);
        let pid = process.pid();
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let writing = std::fs::read_dir(&ws.0)
                .unwrap()
                .map(Result::unwrap)
                .any(|entry| {
                    entry
                        .file_name()
                        .to_string_lossy()
                        .contains("asciiflow-part")
                        && entry.metadata().unwrap().len() > 8192
                });
            if writing {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "C4B did not enter real packet writing"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        let fd_path = PathBuf::from(format!("/proc/{pid}/fd"));
        let open_fds = std::fs::read_dir(&fd_path).unwrap().count();
        assert!(open_fds > 3, "expected live production process descriptors");
        assert_eq!(unsafe { libc::kill(pid as i32, libc::SIGINT) }, 0);
        let result = process.finish();
        assert_eq!(
            result.status.code(),
            Some(130),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(std::fs::read(ws.output()).unwrap(), b"existing-output");
        ws.assert_no_staging();
        // Reaped process disappearance proves its descriptor lifetime ended;
        // this deliberately makes no claim about in-process FD leak freedom.
        assert!(
            !fd_path.exists(),
            "cancelled process retains its descriptor table"
        );
        eprintln!(
            "C4B {codec}/{depth}: {open_fds} live FDs; SIGINT exit 130; reaped FD table absent"
        );
        let followup = Workspace::new();
        completed(&mut command(
            &input,
            &followup.output(),
            codec,
            depth,
            "sdr",
            "copy",
            Path::new("builtin-8x8"),
            true,
            3,
            false,
        ));
        assert_video(&followup.output(), codec, depth, "sdr", 3);
        followup.assert_no_staging();
    }
}
