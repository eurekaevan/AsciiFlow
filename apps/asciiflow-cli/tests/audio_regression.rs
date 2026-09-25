mod audio_support;
use asciiflow_core::FrameSource;

#[test]
fn hevc_av1_software_input_retains_audio_font_and_h264_output() {
    for name in ["hevc-main8-bframes.mp4", "av1-main8-nofilmgrain.mp4"] {
        let ws = Workspace::new();
        let root = fixture("single.mp4")
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_owned();
        let input = root.join("codecs").join(name);
        let result = Process::start(
            command(&input, &ws.output(), "copy")
                .arg("--font")
                .arg(root.join("fonts/Inconsolata-Regular.ttf")),
        )
        .finish();
        success(&result);
        let output = inspect(&ws.output());
        let video = output.iter().find(|s| s.video).unwrap();
        assert_eq!(video.codec, "h264");
        assert_eq!(video.packets.len(), 36);
        same_audio(audio(&inspect(&input))[0], audio(&output)[0]);
        let mut decoder = asciiflow_media::Decoder::open(ws.output()).unwrap();
        let mut count = 0;
        while decoder.next_frame().unwrap().is_some() {
            count += 1;
        }
        assert_eq!(count, 36);
        ws.assert_no_staging();
    }
}

#[test]
fn new_codec_ten_bit_failures_do_not_touch_existing_output() {
    for name in ["hevc-main10-reject.mp4", "av1-main10-reject.mp4"] {
        let ws = Workspace::new();
        std::fs::write(ws.output(), b"existing").unwrap();
        let input = fixture("single.mp4")
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("codecs")
            .join(name);
        let result = Process::start(&mut command(&input, &ws.output(), "none")).finish();
        assert!(!result.status.success());
        assert!(
            String::from_utf8_lossy(&result.stderr).contains("without a conversion path"),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(std::fs::read(ws.output()).unwrap(), b"existing");
        ws.assert_no_staging();
    }
}

#[test]
fn ten_bit_hdr_failure_preserves_existing_output() {
    let ws = Workspace::new();
    std::fs::write(ws.output(), b"existing").unwrap();
    let input = fixture("single.mp4")
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("codecs/hevc-main10-pq-reject.mp4");
    let result = Process::start(&mut command(&input, &ws.output(), "none")).finish();
    assert!(!result.status.success());
    let error = String::from_utf8_lossy(&result.stderr);
    assert!(error.contains("HDR/BT.2020"), "{error}");
    assert_eq!(std::fs::read(ws.output()).unwrap(), b"existing");
    ws.assert_no_staging();
}

#[test]
fn explicit_main10_hdr_failure_preserves_existing_output() {
    let ws = Workspace::new();
    std::fs::write(ws.output(), b"existing").unwrap();
    let input = fixture("single.mp4")
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("codecs/hevc-main10-pq-reject.mp4");
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_asciiflow"));
    cmd.arg(&input).arg(ws.output()).args([
        "--output-codec",
        "hevc",
        "--output-bit-depth",
        "10",
        "--audio",
        "none",
        "--no-progress",
    ]);
    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let result = Process::start(&mut cmd).finish();
    assert!(!result.status.success());
    let error = String::from_utf8_lossy(&result.stderr);
    assert!(error.contains("HDR/BT.2020"), "{error}");
    assert_eq!(std::fs::read(ws.output()).unwrap(), b"existing");
    ws.assert_no_staging();
}

#[test]
fn explicit_font_failures_preserve_output_and_capabilities_remain_font_independent() {
    for name in ["missing.ttf", "Abel-Regular.ttf"] {
        let ws = Workspace::new();
        std::fs::write(ws.output(), b"existing output").unwrap();
        let font = fixture("single.mp4")
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("fonts")
            .join(name);
        let result = Process::start(
            command(&fixture("single.mp4"), &ws.output(), "copy")
                .arg("--font")
                .arg(&font),
        )
        .finish();
        assert!(!result.status.success());
        assert_eq!(std::fs::read(ws.output()).unwrap(), b"existing output");
        ws.assert_no_staging();
        let result = Process::start(
            command(&fixture("single.mp4"), &ws.output(), "copy")
                .arg("--font")
                .arg(&font)
                .arg("--capabilities"),
        )
        .finish();
        success(&result);
    }
}

#[test]
fn freetype_keeps_video_geometry_frame_count_and_audio_contract() {
    let ws = Workspace::new();
    let font = fixture("single.mp4")
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("fonts/Inconsolata-Regular.ttf");
    let result = Process::start(
        command(&fixture("single.mp4"), &ws.output(), "copy")
            .arg("--font")
            .arg(font),
    )
    .finish();
    success(&result);
    let output = inspect(&ws.output());
    same_audio(
        audio(&inspect(&fixture("single.mp4")))[0],
        audio(&output)[0],
    );
    assert_eq!(output.iter().find(|s| s.video).unwrap().packets.len(), 90);
    ws.assert_no_staging();
}
use audio_support::*;

#[test]
fn audio_copy_preserves_aac_payload_metadata_and_decodable_video() {
    let (ws, result) = run("single.mp4", "copy");
    success(&result);
    let input = inspect(&fixture("single.mp4"));
    let output = inspect(&ws.output());
    assert_eq!(output.iter().filter(|s| s.video).count(), 1);
    assert_eq!(audio(&output).len(), 1);
    let track = audio(&output)[0];
    assert_eq!(
        (
            &*track.codec,
            track.rate,
            track.channels,
            track.language.as_deref(),
            track.default
        ),
        ("aac", 48000, 1, Some("jpn"), true)
    );
    same_audio(audio(&input)[0], track);
    assert_audio_decodes(&ws.output());
    let mut decoder = asciiflow_media::Decoder::open(ws.output()).unwrap();
    let mut frames = 0;
    while decoder.next_frame().unwrap().is_some() {
        frames += 1;
    }
    assert_eq!(frames, 90);
}

#[test]
fn audio_auto_and_copy_and_repeated_runs_preserve_identical_audio() {
    let (one, result) = run("single.mp4", "copy");
    success(&result);
    let reference = inspect(&one.output());
    for policy in ["auto", "copy"] {
        let (ws, result) = run("single.mp4", policy);
        success(&result);
        same_audio(audio(&reference)[0], audio(&inspect(&ws.output()))[0]);
    }
}

#[test]
fn audio_none_produces_video_only() {
    let (ws, result) = run("single.mp4", "none");
    success(&result);
    let output = inspect(&ws.output());
    assert!(audio(&output).is_empty());
    assert_eq!(output.iter().filter(|s| s.video).count(), 1);
}

#[test]
fn video_only_input_succeeds_for_all_audio_policies() {
    for policy in ["auto", "copy", "none"] {
        let (ws, result) = run("no-audio.mp4", policy);
        success(&result);
        assert!(audio(&inspect(&ws.output())).is_empty());
    }
}

#[test]
fn multiple_audio_tracks_keep_order_identity_and_packet_routing() {
    let (ws, result) = run("multiple.mp4", "copy");
    success(&result);
    let input = inspect(&fixture("multiple.mp4"));
    let output = inspect(&ws.output());
    let tracks = audio(&output);
    assert_eq!(tracks.len(), 2);
    assert_eq!(tracks[0].language.as_deref(), Some("jpn"));
    assert_eq!(tracks[1].language.as_deref(), Some("eng"));
    assert!(tracks[0].default);
    assert!(!tracks[1].default);
    assert_ne!(tracks[0].packets[1].data, tracks[1].packets[1].data);
    for (a, b) in audio(&input).into_iter().zip(tracks) {
        same_audio(a, b);
    }
}

#[test]
fn audio_auto_skips_only_muxer_confirmed_incompatible_streams() {
    for name in ["mixed.mkv", "incompatible.mkv"] {
        let decoder = asciiflow_media::Decoder::open(fixture(name)).unwrap();
        assert!(
            decoder
                .info()
                .audio_streams
                .iter()
                .any(|s| !s.mp4_compatible),
            "fixture codec is no longer rejected by the actual muxer"
        );
        let selected = decoder
            .info()
            .audio_streams
            .iter()
            .filter(|s| s.mp4_compatible)
            .count();
        let (ws, result) = run(name, "auto");
        success(&result);
        assert_eq!(audio(&inspect(&ws.output())).len(), selected);
        let warning = String::from_utf8_lossy(&result.stderr);
        assert!(
            warning.contains("Warning")
                && warning.contains("pcm_mulaw")
                && warning.contains("skipped")
        );
    }
}

#[test]
fn audio_copy_incompatibility_preserves_destination_and_removes_staging() {
    for existing in [false, true] {
        let ws = Workspace::new();
        if existing {
            std::fs::write(ws.output(), b"existing destination").unwrap();
        }
        let result =
            Process::start(&mut command(&fixture("mixed.mkv"), &ws.output(), "copy")).finish();
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains("--audio copy"));
        if existing {
            assert_eq!(std::fs::read(ws.output()).unwrap(), b"existing destination");
        } else {
            assert!(!ws.output().exists());
        }
        ws.assert_no_staging();
    }
}

#[test]
fn audio_passthrough_preserves_relative_av_start_offset() {
    let (ws, result) = run("offset.mp4", "copy");
    success(&result);
    let delta = |streams: &[Stream]| {
        let a = audio(streams)[0];
        let v = streams.iter().find(|s| s.video).unwrap();
        seconds(a.packets[0].pts, a) - seconds(v.packets.iter().map(|p| p.pts).min().unwrap(), v)
    };
    let input = inspect(&fixture("offset.mp4"));
    let output = inspect(&ws.output());
    assert!(delta(&input) > 0.4);
    assert!((delta(&input) - delta(&output)).abs() <= 1.0 / 48000.0);
    same_audio(audio(&input)[0], audio(&output)[0]);
}

#[test]
fn unequal_stream_durations_drain_both_tracks_without_trimming_or_padding() {
    for name in ["audio-longer.mp4", "video-longer.mp4", "short-video.mp4"] {
        let (ws, result) = run(name, "copy");
        success(&result);
        let input = inspect(&fixture(name));
        let output = inspect(&ws.output());
        same_audio(audio(&input)[0], audio(&output)[0]);
        assert_eq!(
            input.iter().find(|s| s.video).unwrap().packets.len(),
            output.iter().find(|s| s.video).unwrap().packets.len()
        );
    }
}

#[test]
fn video_frame_limit_retains_the_complete_audio_track() {
    let ws = Workspace::new();
    let result = Process::start(
        command(&fixture("single.mp4"), &ws.output(), "copy").args(["--max-frames", "30"]),
    )
    .finish();
    success(&result);
    let output = inspect(&ws.output());
    assert_eq!(output.iter().find(|s| s.video).unwrap().packets.len(), 30);
    same_audio(
        audio(&inspect(&fixture("single.mp4")))[0],
        audio(&output)[0],
    );
    ws.assert_no_staging();
}

#[test]
fn non_audio_extra_stream_is_not_copied() {
    let (ws, result) = run("extra-stream.mp4", "copy");
    success(&result);
    let output = inspect(&ws.output());
    assert_eq!(output.len(), 2);
    same_audio(
        audio(&inspect(&fixture("extra-stream.mp4")))[0],
        audio(&output)[0],
    );
}

#[test]
fn audio_only_input_is_rejected_without_creating_output() {
    let (ws, result) = run("audio-only.mp4", "copy");
    assert!(!result.status.success());
    assert!(!ws.output().exists());
    assert!(String::from_utf8_lossy(&result.stderr).contains("video"));
}

#[test]
fn cfr_discontinuity_with_audio_is_a_terminal_safe_failure() {
    let (ws, result) = run("discontinuous.mp4", "copy");
    assert!(!result.status.success());
    assert!(!ws.output().exists());
    assert!(String::from_utf8_lossy(&result.stderr).contains("CFR"));
}

#[test]
fn audio_capabilities_and_explanations_report_selection_and_skips() {
    for (name, policy, keyword) in [
        ("single.mp4", "copy", "jpn"),
        ("multiple.mp4", "auto", "eng"),
        ("incompatible.mkv", "auto", "skipped"),
        ("single.mp4", "none", "no audio"),
    ] {
        let ws = Workspace::new();
        let mut cmd = command(&fixture(name), &ws.output(), policy);
        cmd.args(["--capabilities", "--explain-plan"]);
        let result = Process::start(&mut cmd).finish();
        success(&result);
        let text = String::from_utf8_lossy(&result.stdout);
        assert!(text.contains("Audio plan") && text.contains(keyword));
        assert!(!ws.output().exists());
        ws.assert_no_staging();
    }
}

#[cfg(target_os = "linux")]
#[test]
fn sigint_with_audio_preserves_existing_output_and_cleans_staging() {
    let ws = Workspace::new();
    cancel_preserving_output(&ws, command(&fixture("long.mp4"), &ws.output(), "copy"));
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "requires Intel VAAPI/Vulkan and a retained 3000-frame benchmark input"]
fn av1_full_interop_sigint_preserves_output_and_next_run_initializes() {
    let input = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/stage51a1-evidence/h264-testsrc2-3000-loop.mp4");
    let ws = Workspace::new();
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_asciiflow"));
    command
        .arg(&input)
        .arg(ws.output())
        .args([
            "--backend",
            "vulkan",
            "--decode",
            "vaapi",
            "--encode",
            "vaapi",
            "--output-codec",
            "av1",
            "--vaapi-vulkan-input-interop",
            "on",
            "--vaapi-vulkan-output-interop",
            "on",
            "--audio",
            "none",
            "--width",
            "80",
            "--no-progress",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    cancel_preserving_output(&ws, command);

    let followup = Workspace::new();
    let mut next = std::process::Command::new(env!("CARGO_BIN_EXE_asciiflow"));
    next.arg(&input)
        .arg(followup.output())
        .args([
            "--backend",
            "vulkan",
            "--decode",
            "vaapi",
            "--encode",
            "vaapi",
            "--output-codec",
            "av1",
            "--vaapi-vulkan-input-interop",
            "on",
            "--vaapi-vulkan-output-interop",
            "on",
            "--audio",
            "none",
            "--width",
            "80",
            "--max-frames",
            "3",
            "--no-progress",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    success(&Process::start(&mut next).finish());
    followup.assert_no_staging();
}

#[cfg(target_os = "linux")]
fn cancel_preserving_output(ws: &Workspace, mut command: std::process::Command) {
    std::fs::write(ws.output(), b"existing-output").unwrap();
    let process = Process::start(&mut command);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let writing = std::fs::read_dir(&ws.0)
            .unwrap()
            .filter_map(Result::ok)
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
            "pipeline did not enter packet writing"
        );
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
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
#[test]
fn buffered_output_failure_with_audio_preserves_existing_output() {
    use std::os::unix::process::CommandExt;
    let ws = Workspace::new();
    std::fs::write(ws.output(), b"existing-output").unwrap();
    let mut cmd = command(&fixture("single.mp4"), &ws.output(), "copy");
    unsafe {
        cmd.pre_exec(|| {
            libc::signal(libc::SIGXFSZ, libc::SIG_IGN);
            let limit = libc::rlimit {
                rlim_cur: 16384,
                rlim_max: 16384,
            };
            if libc::setrlimit(libc::RLIMIT_FSIZE, &limit) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let result = Process::start(&mut cmd).finish();
    assert!(!result.status.success());
    let message = String::from_utf8_lossy(&result.stderr);
    assert!(
        message.contains("File too large") && !message.contains("channel disconnected"),
        "{message}"
    );
    assert_eq!(std::fs::read(ws.output()).unwrap(), b"existing-output");
    ws.assert_no_staging();
}

#[test]
#[ignore = "60-second media stress; run explicitly to check completion without hardware"]
fn long_audio_stream_completes_without_deadlock_or_packet_loss() {
    let (ws, result) = run("long.mp4", "copy");
    success(&result);
    same_audio(
        audio(&inspect(&fixture("long.mp4")))[0],
        audio(&inspect(&ws.output()))[0],
    );
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "requires real Intel VAAPI/Vulkan interop and Khronos validation layer"]
fn intel_audio_parity_validation_and_cancellation() {
    fn hardware(input: &std::path::Path, output: &std::path::Path) -> std::process::Command {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_asciiflow"));
        command
            .arg(input)
            .arg(output)
            .args([
                "--backend",
                "vulkan",
                "--decode",
                "vaapi",
                "--encode",
                "vaapi",
                "--input-interop",
                "on",
                "--output-interop",
                "on",
                "--audio",
                "copy",
                "--width",
                "16",
                "--no-progress",
            ])
            .env("ASCIIFLOW_VULKAN_VALIDATION", "1")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        command
    }
    let (software, result) = run("single.mp4", "copy");
    success(&result);
    let gpu = Workspace::new();
    let result = Process::start(&mut hardware(&fixture("single.mp4"), &gpu.output())).finish();
    success(&result);
    let diagnostics = String::from_utf8_lossy(&result.stderr);
    assert!(
        !diagnostics.contains("VUID-") && !diagnostics.contains("Validation Error"),
        "{diagnostics}"
    );
    same_audio(
        audio(&inspect(&software.output()))[0],
        audio(&inspect(&gpu.output()))[0],
    );
    gpu.assert_no_staging();
    let cancelled = Workspace::new();
    cancel_preserving_output(
        &cancelled,
        hardware(&fixture("long.mp4"), &cancelled.output()),
    );
}
