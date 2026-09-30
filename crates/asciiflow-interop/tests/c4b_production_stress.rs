#![cfg(feature = "hdr-to-sdr-production")]

//! Opt-in full production stress, distinct from C4A pre-encode surface parity.
//! Run alone with --ignored --test-threads=1 and validation enabled. Resource
//! samples describe this process; they do not establish a universal leak bound.
use asciiflow_core::{
    AsciiConfig, CancellationToken, ColorProcessing, ColorSpace, FrameDesc, FrameSource,
    PixelFormat, Rational, VideoCodec,
};
use asciiflow_font::GlyphAtlas;
use asciiflow_interop::{VaapiVulkanFullInteropProcessor, run_full_interop_pipeline};
use asciiflow_media::{DecodeMode, Decoder, Encoder, VaapiOptions};
use asciiflow_vulkan::VulkanAsciiBackend;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const FRAMES: u64 = 3000;
const FFMPEG: &str = "/usr/bin/ffmpeg";

fn quoted(value: &str) -> String {
    let mut result = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            c if c.is_control() => result.push_str(&format!("\\u{:04x}", c as u32)),
            c => result.push(c),
        }
    }
    result.push('"');
    result
}

fn successful(command: &mut Command) -> String {
    let output = command
        .output()
        .expect("required evidence tool unavailable");
    assert!(
        output.status.success(),
        "{command:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn sha256(path: &Path) -> String {
    successful(Command::new("sha256sum").arg(path))
        .split_whitespace()
        .next()
        .unwrap()
        .to_owned()
}

fn write_new(path: &Path, document: &str) {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .unwrap()
        .write_all(document.as_bytes())
        .unwrap();
}

fn fixture(name: &str) -> (PathBuf, String) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/codecs")
        .join(name)
        .canonicalize()
        .unwrap();
    let expected = include_str!("../../../tests/fixtures/codecs/SHA256SUMS")
        .lines()
        .find_map(|line| {
            let mut fields = line.split_whitespace();
            let digest = fields.next()?;
            (fields.next()? == format!("./{name}")).then_some(digest)
        })
        .expect("legal fixture identity missing");
    let hash = sha256(&path);
    assert_eq!(hash, expected, "legal fixture changed");
    (path, hash)
}

#[derive(Clone, Copy)]
struct Sample {
    elapsed_ms: u128,
    fd: usize,
    rss_kib: u64,
}

fn sample(started: Instant) -> Sample {
    let fd = fs::read_dir("/proc/self/fd").unwrap().count();
    let status = fs::read_to_string("/proc/self/status").unwrap();
    let rss_kib = status
        .lines()
        .find(|line| line.starts_with("VmRSS:"))
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    Sample {
        elapsed_ms: started.elapsed().as_millis(),
        fd,
        rss_kib,
    }
}

struct Sampler {
    stop: Arc<AtomicBool>,
    samples: Arc<Mutex<Vec<Sample>>>,
    worker: Option<JoinHandle<()>>,
}

impl Sampler {
    fn new(started: Instant) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let samples = Arc::new(Mutex::new(Vec::new()));
        let thread_stop = stop.clone();
        let thread_samples = samples.clone();
        let worker = thread::spawn(move || {
            while !thread_stop.load(Ordering::Acquire) {
                thread_samples.lock().unwrap().push(sample(started));
                thread::sleep(Duration::from_millis(25));
            }
        });
        Self {
            stop,
            samples,
            worker: Some(worker),
        }
    }

    fn finish(mut self) -> Vec<Sample> {
        self.stop.store(true, Ordering::Release);
        self.worker.take().unwrap().join().unwrap();
        self.samples.lock().unwrap().clone()
    }
}

impl Drop for Sampler {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            worker.join().unwrap();
        }
    }
}

fn assert_decoded_output(output: &Path, codec: VideoCodec, format: PixelFormat) -> u64 {
    let mut decoder =
        Decoder::open_with(output, DecodeMode::Software, VaapiOptions::default()).unwrap();
    assert_eq!(decoder.info().requirements.codec, codec);
    assert_eq!(
        decoder.info().requirements.bit_depth,
        Some(if format == PixelFormat::Nv12 { 8 } else { 10 })
    );
    assert_eq!(decoder.info().frame_rate, Rational::new(50, 1).unwrap());
    let mut frames = 0;
    let mut previous_pts = None;
    while let Some(frame) = decoder.next_frame().unwrap() {
        assert_eq!((frame.desc().width, frame.desc().height), (1920, 1080));
        assert_eq!(frame.desc().format, format);
        assert_eq!(frame.desc().color_space, ColorSpace::default());
        let pts = frame.pts().expect("output frame has no timestamp");
        if let Some(previous) = previous_pts {
            assert!(pts > previous);
        }
        previous_pts = Some(pts);
        frames += 1;
    }
    assert_eq!(frames, FRAMES, "full encoded output must decode");
    frames
}

#[test]
#[ignore = "real Intel full VAAPI/Vulkan/VAAPI production; two 3000-frame paths"]
fn c4b_full_production_3000_frame_stress() {
    assert_eq!(std::env::var("ASCIIFLOW_C4B_STRESS").unwrap(), "1");
    assert_eq!(std::env::var("ASCIIFLOW_VULKAN_VALIDATION").unwrap(), "1");
    let destination = PathBuf::from(
        std::env::var_os("ASCIIFLOW_C4B_STRESS_DIR")
            .expect("set a new ASCIIFLOW_C4B_STRESS_DIR evidence directory"),
    );
    fs::create_dir(&destination).expect("evidence directory must be new");
    let version = successful(Command::new(FFMPEG).arg("-version"));
    assert!(
        version.starts_with("ffmpeg version 8.1.3 "),
        "fixed fixture toolchain required: {version}"
    );
    write_new(&destination.join("ffmpeg-version.log"), &version);
    let ffmpeg_hash = sha256(Path::new(FFMPEG));
    let binary = std::env::current_exe().unwrap();
    let binary_hash = sha256(&binary);

    for (name, label, codec, format) in [
        (
            "hevc-main10-pq-c3-legal-v1.mp4",
            "hevc-pq-to-h264-nv12",
            VideoCodec::H264,
            PixelFormat::Nv12,
        ),
        (
            "av1-main10-pq-c3-legal-v1.mp4",
            "av1-pq-to-hevc10-p010",
            VideoCodec::Hevc,
            PixelFormat::P010Le,
        ),
    ] {
        let (input, input_hash) = fixture(name);
        let looped = destination.join(format!("{label}-input.mp4"));
        let output = destination.join(format!("{label}-output.mp4"));
        let argv = vec![
            "-v".into(),
            "error".into(),
            "-nostdin".into(),
            "-n".into(),
            "-stream_loop".into(),
            "9".into(),
            "-i".into(),
            input.to_string_lossy().into_owned(),
            "-map".into(),
            "0:v:0".into(),
            "-an".into(),
            "-c:v".into(),
            "copy".into(),
            "-frames:v".into(),
            FRAMES.to_string(),
            looped.to_string_lossy().into_owned(),
        ];
        write_new(
            &destination.join(format!("{label}-fixture-command.json")),
            &format!(
                "{{\"input\":{},\"input_sha256\":{},\"ffmpeg_sha256\":{},\"cwd\":{},\"argv\":[{},{}]}}\n",
                quoted(&input.display().to_string()),
                quoted(&input_hash),
                quoted(&ffmpeg_hash),
                quoted(&std::env::current_dir().unwrap().display().to_string()),
                quoted(FFMPEG),
                argv.iter().map(|a| quoted(a)).collect::<Vec<_>>().join(",")
            ),
        );
        let loop_result = Command::new(FFMPEG).args(&argv).output().unwrap();
        write_new(
            &destination.join(format!("{label}-fixture.log")),
            &String::from_utf8_lossy(&loop_result.stderr),
        );
        assert!(
            loop_result.status.success(),
            "3000-frame fixture copy failed"
        );
        let loop_hash = sha256(&looped);

        let started = Instant::now();
        let before = sample(started);
        let sampler = Sampler::new(started);
        let vaapi = VaapiOptions::new(Some(PathBuf::from("/dev/dri/renderD128")));
        let decoder =
            Decoder::open_with_pq_preserve(&looped, DecodeMode::Vaapi, vaapi.clone()).unwrap();
        let info = decoder.info().clone();
        assert_eq!(
            (info.frame_desc.width, info.frame_desc.height),
            (1920, 1080)
        );
        assert_eq!(info.frame_desc.format, PixelFormat::P010Le);
        assert_eq!(info.frame_desc.color_space, ColorSpace::pq_bt2020());
        assert_eq!(info.frame_rate, Rational::new(50, 1).unwrap());
        let config = AsciiConfig {
            grid_width: 80,
            color: true,
            ..Default::default()
        };
        let atlas = GlyphAtlas::builtin(&config.font, &config.charset).unwrap();
        let mut backend =
            VulkanAsciiBackend::new_for_color_processing(ColorProcessing::HdrPqToSdrBt709)
                .unwrap()
                .with_atlas(atlas, &config)
                .unwrap();
        backend.prepare(&info.frame_desc, &config).unwrap();
        assert_eq!(backend.device_info().vendor_id, 0x8086);
        let device = format!("{:?}", backend.device_info());
        let observe = backend.validation_observer();
        let desc = match format {
            PixelFormat::Nv12 => FrameDesc::host_nv12(1920, 1080, ColorSpace::default()),
            PixelFormat::P010Le => FrameDesc::host_p010_le(1920, 1080, ColorSpace::default()),
        }
        .unwrap();
        let encoder = Encoder::create_with_hardware_frames_codec_and_audio(
            &output,
            desc,
            info.frame_rate,
            codec.clone(),
            vaapi,
            Vec::new(),
            CancellationToken::new(),
        )
        .unwrap();
        let processor = VaapiVulkanFullInteropProcessor::new_hdr_to_sdr(
            backend,
            encoder.encoder_frames().unwrap(),
            info.frame_desc,
            config,
            format,
        )
        .unwrap();
        let result = run_full_interop_pipeline(decoder, processor, encoder, Some(FRAMES), 2);
        // The pipeline owns and tears down decoder, two resident slots and encoder
        // before returning. The observer retains only the validation counter.
        let after = sample(started);
        let mut samples = sampler.finish();
        samples.push(before);
        samples.push(after);
        samples.sort_by_key(|s| s.elapsed_ms);
        let validation_errors = observe();
        let peak_fd = samples.iter().map(|s| s.fd).max().unwrap();
        let peak_rss = samples.iter().map(|s| s.rss_kib).max().unwrap();
        let sample_json = samples
            .iter()
            .map(|s| {
                format!(
                    "{{\"elapsed_ms\":{},\"fd\":{},\"rss_kib\":{}}}",
                    s.elapsed_ms, s.fd, s.rss_kib
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let pipeline_log =
            format!("{result:#?}\nvalidation_errors_after_teardown={validation_errors}\n");
        write_new(
            &destination.join(format!("{label}-pipeline.log")),
            &pipeline_log,
        );
        let metrics_frames = result.as_ref().map_or(0, |m| m.frames);
        let error = result.as_ref().err().map(|e| e.to_string());
        let evidence = format!(
            concat!(
                "{{\"scope\":\"full production pipeline resource stress; encoded output correctness checked separately\",",
                "\"path\":{},\"input\":{},\"input_sha256\":{},\"fixture_command\":[\"/usr/bin/ffmpeg\",{}],",
                "\"looped_input\":{},\"looped_input_sha256\":{},\"test_binary\":{},\"test_binary_sha256\":{},",
                "\"device\":{},\"metrics_frames\":{},\"pipeline_error\":{},\"validation_errors_after_teardown\":{},",
                "\"fd_before\":{},\"fd_peak_sampled\":{},\"fd_after_teardown\":{},",
                "\"rss_before_kib\":{},\"rss_peak_sampled_kib\":{},\"rss_after_teardown_kib\":{},",
                "\"sampling_interval_ms\":25,\"resource_gate\":\"observed samples only; no invented universal FD/RSS limit\",\"samples\":[{}]}}\n"
            ),
            quoted(label),
            quoted(&input.display().to_string()),
            quoted(&input_hash),
            argv.iter().map(|a| quoted(a)).collect::<Vec<_>>().join(","),
            quoted(&looped.display().to_string()),
            quoted(&loop_hash),
            quoted(&binary.display().to_string()),
            quoted(&binary_hash),
            quoted(&device),
            metrics_frames,
            error.map_or("null".into(), |e| quoted(&e)),
            validation_errors,
            before.fd,
            peak_fd,
            after.fd,
            before.rss_kib,
            peak_rss,
            after.rss_kib,
            sample_json
        );
        write_new(
            &destination.join(format!("{label}-resources.json")),
            &evidence,
        );
        let metrics = result.unwrap();
        assert_eq!(metrics.frames, FRAMES);
        assert!(
            metrics.hardware_download.is_zero(),
            "production path downloaded source pixels"
        );
        assert!(
            metrics.hardware_upload.is_zero(),
            "production path uploaded host output pixels"
        );
        assert_eq!(validation_errors, 0, "validation failed including teardown");
        assert_eq!(after.fd, before.fd, "full production pipeline leaked FDs");
        let decoded = assert_decoded_output(&output, codec, format);
        let output_hash = sha256(&output);
        let probe = successful(
            Command::new("ffprobe")
                .args([
                    "-v",
                    "error",
                    "-count_frames",
                    "-show_streams",
                    "-of",
                    "json",
                ])
                .arg(&output),
        );
        write_new(
            &destination.join(format!("{label}-output-probe.json")),
            &probe,
        );
        write_new(
            &destination.join(format!("{label}-completion.json")),
            &format!(
                "{{\"decoded_output_frames\":{decoded},\"output\":{},\"output_sha256\":{},\"validation_errors_after_teardown\":{validation_errors}}}\n",
                quoted(&output.display().to_string()),
                quoted(&output_hash)
            ),
        );
        eprintln!(
            "{label}: frames={decoded} FD before/peak/after={}/{}/{} RSS KiB={}/{}/{} validation={validation_errors}",
            before.fd, peak_fd, after.fd, before.rss_kib, peak_rss, after.rss_kib
        );
    }
}
