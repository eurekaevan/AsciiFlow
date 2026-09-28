//! Opt-in production PQ CLI checks against real Intel VAAPI/Vulkan hardware.
//! Software decoding below is an output oracle, never a selected production path.
// This test target reuses only the hardware-relevant subset of the shared helpers.
#[allow(dead_code)]
mod audio_support;

use asciiflow_core::{ColorSpace, FrameSource, PixelFormat, Rational, VideoCodec, VideoProfile};
use asciiflow_media::{DecodeMode, Decoder, VaapiOptions};
use audio_support::{
    Process, Workspace, assert_audio_decodes, audio, inspect, same_audio, success,
};
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Mutex,
};

static HARDWARE_TEST: Mutex<()> = Mutex::new(());

fn canonical(codec: &str) -> PathBuf {
    let directory = std::env::var_os("ASCIIFLOW_PQ_CANONICAL_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/codecs")
        });
    let path = directory.join(format!("{codec}-main10-pq-canonical-v1.mp4"));
    assert!(
        path.is_file(),
        "missing canonical PQ input: {}",
        path.display()
    );
    path
}

fn hardware(input: &Path, output: &Path, codec: &str, audio_policy: &str) -> Command {
    hardware_with_limits(
        input,
        output,
        codec,
        audio_policy,
        Path::new("builtin-8x8"),
        300,
    )
}

fn hardware_with_limits(
    input: &Path,
    output: &Path,
    codec: &str,
    audio_policy: &str,
    font: &Path,
    max_frames: u32,
) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_asciiflow"));
    cmd.arg(input).arg(output).args([
        "--backend",
        "vulkan",
        "--decode",
        "vaapi",
        "--encode",
        "vaapi",
        "--output-codec",
        codec,
        "--output-bit-depth",
        "10",
        "--vaapi-vulkan-input-interop",
        "on",
        "--vaapi-vulkan-output-interop",
        "on",
        "--hw-device",
        "/dev/dri/renderD128",
        "--width",
        "80",
        "--charset",
        "standard",
        "--color",
        "true",
        "--audio",
        audio_policy,
        "--no-progress",
    ]);
    cmd.arg("--font")
        .arg(font)
        .arg("--max-frames")
        .arg(max_frames.to_string());
    cmd.env("ASCIIFLOW_VULKAN_VALIDATION", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd
}

fn completed(cmd: &mut Command) {
    let result = Process::start(cmd).finish();
    success(&result);
    let diagnostics = String::from_utf8_lossy(&result.stderr);
    assert!(
        !diagnostics.contains("VUID-") && !diagnostics.contains("Validation Error"),
        "{diagnostics}"
    );
}

fn assert_video(path: &Path, codec: &str, count: usize) {
    let mut decoder =
        Decoder::open_with_pq_preserve(path, DecodeMode::Software, VaapiOptions::default())
            .unwrap();
    let mut frames = 0;
    let mut previous = None;
    while let Some(frame) = decoder.next_frame().unwrap() {
        assert_eq!((frame.desc().width, frame.desc().height), (1920, 1080));
        assert_eq!(frame.desc().format, PixelFormat::P010Le);
        assert_eq!(frame.desc().color_space, ColorSpace::pq_bt2020());
        let pts = frame.pts().expect("PQ output frame missing PTS");
        if let Some(last) = previous {
            assert!(pts > last, "PQ output PTS is not strictly increasing");
        }
        previous = Some(pts);
        frames += 1;
    }
    assert_eq!(frames, count, "PQ output did not fully decode");
    assert_eq!(decoder.info().frame_rate, Rational::new(50, 1).unwrap());
    let requirements = &decoder.info().requirements;
    let expected_codec = if codec == "hevc" {
        VideoCodec::Hevc
    } else {
        VideoCodec::Av1
    };
    assert_eq!(requirements.codec, expected_codec);
    assert_eq!(requirements.bit_depth, Some(10));
    assert_eq!(
        requirements.profile,
        Some(if codec == "hevc" {
            VideoProfile::HevcMain10
        } else {
            VideoProfile::Av1Main
        })
    );
    assert_eq!(requirements.color_space, ColorSpace::pq_bt2020());
}

#[test]
#[ignore = "requires real qualified Intel VAAPI/Vulkan PQ probes"]
fn pq_auto_policies_select_only_the_full_hardware_path() {
    let _serial = HARDWARE_TEST.lock().unwrap();
    for input_codec in ["hevc", "av1"] {
        for output_codec in ["hevc", "av1"] {
            let ws = Workspace::new();
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_asciiflow"));
            cmd.arg(canonical(input_codec))
                .arg(ws.output())
                .args([
                    "--output-codec",
                    output_codec,
                    "--output-bit-depth",
                    "10",
                    "--width",
                    "80",
                    "--charset",
                    "standard",
                    "--font",
                    "builtin-8x8",
                    "--color",
                    "true",
                    "--audio",
                    "none",
                    "--max-frames",
                    "3",
                    "--no-progress",
                ])
                .env("ASCIIFLOW_VULKAN_VALIDATION", "1")
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            let result = Process::start(&mut cmd).finish();
            success(&result);
            let plan = String::from_utf8_lossy(&result.stdout);
            assert!(plan.contains("GPU + DMA-BUF input/output"), "{plan}");
            assert!(
                plan.contains("VAAPI decode") && plan.contains("VAAPI encode"),
                "{plan}"
            );
            let diagnostics = String::from_utf8_lossy(&result.stderr);
            assert!(
                !diagnostics.contains("VUID-") && !diagnostics.contains("Validation Error"),
                "{diagnostics}"
            );
            assert_video(&ws.output(), output_codec, 3);
            ws.assert_no_staging();
        }
    }
}

// AAC variants reuse the canonical coded video byte-for-byte. This creates only
// local test input, and requires the same FFmpeg executable as existing fixtures.
fn with_audio(ws: &Workspace, input: &Path, tracks: usize) -> PathBuf {
    assert!((1..=2).contains(&tracks));
    let path = ws.0.join(format!("pq-{tracks}-aac.mp4"));
    let mut cmd = Command::new("ffmpeg");
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
    cmd.arg(&path).stdout(Stdio::piped()).stderr(Stdio::piped());
    completed(&mut cmd);
    assert_eq!(audio(&inspect(&path)).len(), tracks);
    path
}

fn with_static_metadata(ws: &Workspace) -> PathBuf {
    let version = Command::new("ffmpeg").arg("-version").output().unwrap();
    success(&version);
    let version = String::from_utf8(version.stdout).unwrap();
    assert!(
        version
            .lines()
            .next()
            .unwrap()
            .starts_with("ffmpeg version 8.1.3 "),
        "static-metadata source requires fixed FFmpeg 8.1.3: {version}"
    );
    let path = ws.0.join("pq-static-source.mp4");
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y",
        "-threads", "1", "-i"]).arg(canonical("hevc"))
        .args(["-map", "0:v:0", "-an", "-frames:v", "300", "-r", "50",
            "-fps_mode", "cfr", "-pix_fmt", "yuv420p10le",
            "-color_primaries", "bt2020", "-color_trc", "smpte2084",
            "-colorspace", "bt2020nc", "-color_range", "tv",
            "-chroma_sample_location", "left", "-threads", "1",
            "-map_metadata", "-1", "-c:v", "libx265", "-preset", "ultrafast",
            "-x265-params",
            "lossless=1:bframes=0:keyint=50:min-keyint=50:open-gop=0:pools=none:frame-threads=1:wpp=0:log-level=error:master-display=G(8500,39850)B(6550,2300)R(35400,14600)WP(15635,16450)L(10000000,1):max-cll=1000,400",
            "-bsf:v",
            "hevc_metadata=video_full_range_flag=0:colour_primaries=9:transfer_characteristics=16:matrix_coefficients=9:chroma_sample_loc_type=0",
            "-video_track_timescale", "50000", "-fflags", "+bitexact",
            "-flags:v", "+bitexact", "-movflags", "+faststart"])
        .arg(&path).stdout(Stdio::piped()).stderr(Stdio::piped());
    completed(&mut cmd);
    let mut decoder =
        Decoder::open_with_pq_preserve(&path, DecodeMode::Software, VaapiOptions::default())
            .unwrap();
    assert!(decoder.next_frame().unwrap().is_some());
    let resolved = decoder.info().requirements.color_semantics.unwrap();
    let (mastering, light) = resolved.effective_static_metadata();
    let mastering = mastering.expect("encoded source lacks real mastering-display SEI");
    assert!(mastering.display_primaries.is_some());
    assert!(mastering.white_point.is_some());
    assert!(mastering.min_luminance.is_some());
    assert!(mastering.max_luminance.is_some());
    let light = light.expect("encoded source lacks real content-light SEI");
    assert_eq!(light.max_cll, Some(1000));
    assert_eq!(light.max_fall, Some(400));
    path
}

fn assert_static_metadata_absent(path: &Path, count: usize) {
    let mut decoder =
        Decoder::open_with_pq_preserve(path, DecodeMode::Software, VaapiOptions::default())
            .unwrap();
    let mut frames = 0;
    while decoder.next_frame().unwrap().is_some() {
        let color = decoder.info().requirements.color_semantics.unwrap();
        for raw in [color.stream, color.frame] {
            assert!(
                raw.mastering_display.is_none(),
                "mastering metadata leaked at frame {frames}"
            );
            assert!(
                raw.content_light.is_none(),
                "CLL/FALL metadata leaked at frame {frames}"
            );
        }
        assert_eq!(color.effective_static_metadata(), (None, None));
        frames += 1;
    }
    assert_eq!(frames, count);

    // Probe actual coded stream and all decoded frames independently. Flat output
    // gives one typed media_type line per frame without adding a JSON dependency.
    let probe = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "v:0", "-show_streams",
            "-show_frames", "-show_entries",
            "stream=codec_name:stream_side_data=side_data_type:frame=media_type:frame_side_data=side_data_type",
            "-of", "flat"]).arg(path).output().unwrap();
    success(&probe);
    let actual = String::from_utf8(probe.stdout).unwrap();
    assert_eq!(
        actual
            .lines()
            .filter(|line| line.ends_with(".media_type=\"video\""))
            .count(),
        count
    );
    let lower = actual.to_ascii_lowercase();
    assert!(!lower.contains("mastering display metadata"), "{actual}");
    assert!(!lower.contains("content light level metadata"), "{actual}");
}

#[test]
#[ignore = "requires generated 1080p/300 PQ input, fixed FFmpeg 8.1.3 x265 SEI encoding, Intel Main10 full interop and validation"]
fn pq_mastering_display_and_content_light_source_do_not_leak_into_outputs() {
    let _guard = HARDWARE_TEST.lock().unwrap();
    let source = Workspace::new();
    let input = with_static_metadata(&source);
    assert_video(&input, "hevc", 300);
    for codec in ["hevc", "av1"] {
        let ws = Workspace::new();
        completed(&mut hardware(&input, &ws.output(), codec, "none"));
        assert_video(&ws.output(), codec, 300);
        assert_static_metadata_absent(&ws.output(), 300);
        ws.assert_no_staging();
    }
}

#[test]
#[ignore = "requires generated 1080p/300 PQ fixtures, Intel iHD/ANV HEVC/AV1 Main10 and Khronos validation"]
fn pq_same_and_cross_codec_full_interop_preserves_300_frame_signal() {
    let _guard = HARDWARE_TEST.lock().unwrap();
    for input_codec in ["hevc", "av1"] {
        let input = canonical(input_codec);
        assert_video(&input, input_codec, 300);
        for output_codec in ["hevc", "av1"] {
            let ws = Workspace::new();
            completed(&mut hardware(&input, &ws.output(), output_codec, "none"));
            assert_video(&ws.output(), output_codec, 300);
            assert!(audio(&inspect(&ws.output())).is_empty());
            ws.assert_no_staging();
        }
    }
}

#[test]
#[ignore = "requires generated 1080p/300 PQ fixtures, FFmpeg AAC, real Intel full interop and Khronos validation"]
fn pq_single_and_dual_aac_auto_copy_preserve_payload_metadata_and_video() {
    let _guard = HARDWARE_TEST.lock().unwrap();
    for codec in ["hevc", "av1"] {
        let source = Workspace::new();
        for tracks in [1, 2] {
            let input = with_audio(&source, &canonical(codec), tracks);
            let input_streams = inspect(&input);
            for policy in ["auto", "copy"] {
                let ws = Workspace::new();
                completed(&mut hardware(&input, &ws.output(), codec, policy));
                assert_video(&ws.output(), codec, 300);
                let output_streams = inspect(&ws.output());
                let input_audio = audio(&input_streams);
                let output_audio = audio(&output_streams);
                assert_eq!(output_audio.len(), tracks);
                for (before, after) in input_audio.into_iter().zip(output_audio) {
                    same_audio(before, after);
                }
                assert_audio_decodes(&ws.output());
                ws.assert_no_staging();
            }
        }
    }
}

#[test]
#[ignore = "requires generated 1080p/300 PQ fixtures, Intel full interop, bundled FreeType fixture and Khronos validation"]
fn pq_freetype_keeps_geometry_metadata_and_audio_none() {
    let _guard = HARDWARE_TEST.lock().unwrap();
    let font = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/fonts/Inconsolata-Regular.ttf");
    assert!(
        font.is_file(),
        "missing FreeType fixture: {}",
        font.display()
    );
    let source = Workspace::new();
    let input = with_audio(&source, &canonical("hevc"), 1);
    for codec in ["hevc", "av1"] {
        let ws = Workspace::new();
        let mut cmd = hardware_with_limits(&input, &ws.output(), codec, "none", &font, 300);
        completed(&mut cmd);
        assert_video(&ws.output(), codec, 300);
        assert!(audio(&inspect(&ws.output())).is_empty());
        ws.assert_no_staging();
    }
}

#[cfg(target_os = "linux")]
fn cancel_preserving_output(ws: &Workspace, mut cmd: Command) {
    std::fs::write(ws.output(), b"existing-output").unwrap();
    let process = Process::start(&mut cmd);
    wait_for_packet_writing(ws);
    assert_eq!(unsafe { libc::kill(process.pid() as i32, libc::SIGINT) }, 0);
    let result = process.finish();
    assert_eq!(
        result.status.code(),
        Some(130),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(std::fs::read(ws.output()).unwrap(), b"existing-output");
    ws.assert_no_staging();
}

#[cfg(target_os = "linux")]
fn wait_for_packet_writing(ws: &Workspace) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
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
            std::time::Instant::now() < deadline,
            "PQ pipeline did not enter packet writing"
        );
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "requires generated PQ fixtures, FFmpeg remux, Intel full interop and SIGINT lifecycle validation"]
fn pq_sigint_preserves_destination_and_fresh_followup_initializes() {
    let _guard = HARDWARE_TEST.lock().unwrap();
    let source = Workspace::new();
    let input = with_audio(&source, &canonical("hevc"), 1);
    let long = source.0.join("pq-loop.mp4");
    let mut remux = Command::new("ffmpeg");
    remux
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
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    completed(&mut remux);
    for codec in ["hevc", "av1"] {
        let cancelled = Workspace::new();
        cancel_preserving_output(
            &cancelled,
            hardware_with_limits(
                &long,
                &cancelled.output(),
                codec,
                "copy",
                Path::new("builtin-8x8"),
                3000,
            ),
        );
        let followup = Workspace::new();
        let mut cmd = hardware_with_limits(
            &input,
            &followup.output(),
            codec,
            "copy",
            Path::new("builtin-8x8"),
            3,
        );
        completed(&mut cmd);
        assert_video(&followup.output(), codec, 3);
        followup.assert_no_staging();
    }
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "requires generated PQ fixtures, FFmpeg AAC, Intel full interop and Linux RLIMIT_FSIZE failure cleanup"]
fn pq_mux_write_failure_preserves_destination_and_removes_staging() {
    use std::os::unix::process::CommandExt;
    let _guard = HARDWARE_TEST.lock().unwrap();
    let source = Workspace::new();
    let input = with_audio(&source, &canonical("hevc"), 1);
    for codec in ["hevc", "av1"] {
        let ws = Workspace::new();
        std::fs::write(ws.output(), b"existing-output").unwrap();
        let mut cmd = hardware(&input, &ws.output(), codec, "copy");
        unsafe {
            cmd.pre_exec(|| {
                libc::signal(libc::SIGXFSZ, libc::SIG_IGN);
                Ok(())
            });
        }
        let process = Process::start(&mut cmd);
        // A startup-wide limit also restricts Mesa cache/memfd writes and can
        // fail Vulkan initialization instead of the intended output path.
        // Inject only after actual video packets reached transactional staging.
        wait_for_packet_writing(&ws);
        let limit = libc::rlimit {
            rlim_cur: 16384,
            rlim_max: 16384,
        };
        assert_eq!(
            unsafe {
                libc::prlimit(
                    process.pid() as i32,
                    libc::RLIMIT_FSIZE,
                    &limit,
                    std::ptr::null_mut(),
                )
            },
            0,
            "set midstream output size limit: {}",
            std::io::Error::last_os_error()
        );
        let result = process.finish();
        assert!(!result.status.success());
        let error = String::from_utf8_lossy(&result.stderr);
        assert!(
            error.contains("File too large") && !error.contains("channel disconnected"),
            "{error}"
        );
        assert_eq!(std::fs::read(ws.output()).unwrap(), b"existing-output");
        ws.assert_no_staging();
    }
}
