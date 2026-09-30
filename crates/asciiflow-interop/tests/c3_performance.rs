#![cfg(feature = "hdr-to-sdr-qualification")]

//! Full reference and qualification A/B/C performance on cached host P010.
//! Decode, download, DMA-BUF interop, encoding and production support are excluded.
use asciiflow_core::{
    AsciiConfig, ChromaSubsampling, ColorSpace, HostFrame, PixelFormat, VideoCodec, VideoFrame,
    sdr_target_volume::{SdrConversionOutput, convert_method_a_rgb},
    tone_map_bt2446::MethodA,
};
use asciiflow_cpu::hdr::HdrPqReference;
use asciiflow_font::GlyphAtlas;
use asciiflow_media::{DecodeMode, Decoder, VaapiDecodedFrame, VaapiOptions};
use asciiflow_vulkan::{C3Output, VulkanHdrToSdrQualification};
use ffmpeg_sys_next as ffi;
use std::{
    hint::black_box,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

const WIDTH: u32 = 1920;
const HEIGHT: u32 = 1080;
const FRAMES: usize = 300;
const WARMUP: usize = 5;
const INPUT: &str = "hevc-main10-pq-c3-legal-v1.mp4";
const N3_EPSILON: f64 = 1.0 / 1792.0;

fn downloaded(frame: &VaapiDecodedFrame) -> VideoFrame {
    struct Transfer(*mut ffi::AVFrame);
    impl Drop for Transfer {
        fn drop(&mut self) {
            unsafe { ffi::av_frame_free(&mut self.0) };
        }
    }
    let transfer = Transfer(unsafe { ffi::av_frame_alloc() });
    assert!(!transfer.0.is_null());
    unsafe { (*transfer.0).format = ffi::AVPixelFormat::AV_PIX_FMT_P010LE as i32 };
    let status = unsafe { ffi::av_hwframe_transfer_data(transfer.0, frame.as_raw_ptr(), 0) };
    assert!(status >= 0, "VAAPI P010 download failed: {status}");
    let native = unsafe { &*transfer.0 };
    assert_eq!(native.format, ffi::AVPixelFormat::AV_PIX_FMT_P010LE as i32);
    let desc = frame.desc();
    let mut storage = HostFrame::new_zeroed(desc);
    let (y, uv) = storage.planes_mut(desc);
    let stride = desc.y_stride();
    for (plane, target, rows) in [
        (0, y, desc.height as usize),
        (1, uv, desc.height as usize / 2),
    ] {
        assert!(native.linesize[plane] >= stride as i32);
        assert!(!native.data[plane].is_null());
        for row in 0..rows {
            unsafe {
                std::ptr::copy_nonoverlapping(
                    native.data[plane].add(row * native.linesize[plane] as usize),
                    target.as_mut_ptr().add(row * stride),
                    stride,
                );
            }
        }
    }
    VideoFrame::new_host(desc.clone(), frame.pts(), storage).unwrap()
}

fn input_sha256(path: &Path) -> String {
    let expected = include_str!("../../../tests/fixtures/codecs/SHA256SUMS")
        .lines()
        .find_map(|line| {
            let mut fields = line.split_whitespace();
            let hash = fields.next()?;
            (fields.next()? == format!("./{INPUT}")).then_some(hash)
        })
        .expect("canonical checksum");
    let result = Command::new("sha256sum").arg(path).output().unwrap();
    assert!(result.status.success(), "fixture checksum read failed");
    let actual = String::from_utf8(result.stdout)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .to_owned();
    assert_eq!(actual, expected, "canonical fixture bytes changed");
    actual
}

fn mask(rgb: [f64; 3]) -> u32 {
    rgb.into_iter().enumerate().fold(0, |mask, (c, v)| {
        mask | if v < 0.0 {
            1 << c
        } else if v > 1.0 {
            1 << (c + 3)
        } else {
            0
        }
    })
}

// N3 diagnostic projection of the existing display-power signal, 4:4:4 only.
fn codes([r, g, b]: [f64; 3]) -> [u32; 3] {
    let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    [
        64.0 + 876.0 * y,
        512.0 + 896.0 * (b - y) / 1.8556,
        512.0 + 896.0 * (r - y) / 1.5748,
    ]
    .map(|v| (v + 0.5).floor() as u32)
}

#[derive(Default)]
struct Precheck {
    glyph_mismatches: u64,
    coverage_mismatches: u64,
    mask_mismatches: u64,
    policy_mismatches: u64,
    invalid_components: u64,
    rgb_over_epsilon: u64,
    codes_over_one: u64,
    max_rgb_error: f64,
    max_code_distance: u32,
}

impl Precheck {
    fn passed(&self) -> bool {
        self.glyph_mismatches == 0
            && self.coverage_mismatches == 0
            && self.mask_mismatches == 0
            && self.policy_mismatches == 0
            && self.invalid_components == 0
            && self.rgb_over_epsilon == 0
            && self.codes_over_one == 0
    }
    fn json(&self) -> String {
        format!(
            "{{\"glyph_mismatches\":{},\"r8_coverage_mismatches\":{},\"exact_cpu_clip_mask_mismatches\":{},\"exact_limiter_policy_mismatches\":{},\"invalid_final_components\":{},\"components_over_n3_epsilon\":{},\"limited_10bit_codes_over_one\":{},\"max_rgb_error\":{},\"max_code_distance\":{},\"n3_epsilon\":{N3_EPSILON}}}",
            self.glyph_mismatches,
            self.coverage_mismatches,
            self.mask_mismatches,
            self.policy_mismatches,
            self.invalid_components,
            self.rgb_over_epsilon,
            self.codes_over_one,
            self.max_rgb_error,
            self.max_code_distance
        )
    }
}

fn precheck(
    gpu: &mut VulkanHdrToSdrQualification,
    host: &VideoFrame,
    config: &AsciiConfig,
    reference: &HdrPqReference<'_>,
) -> Precheck {
    let (w, h) = config.resolved_grid(WIDTH, HEIGHT).unwrap();
    let grid = reference.map(host, w, h).unwrap();
    let rendered = reference
        .render_linear(&grid, host.desc(), config.color)
        .unwrap();
    let method = MethodA::new();
    let expected: Vec<_> = rendered
        .pixels()
        .iter()
        .map(|p| convert_method_a_rgb(method.map(*p).unwrap().rgb).unwrap())
        .collect();
    let output = gpu.process_host(host, config, true).unwrap();
    assert_eq!((output.width, output.height), (WIDTH, HEIGHT));
    assert_eq!(output.pts, host.pts());
    assert_eq!(output.diagnostics[..5], [0; 5]);
    assert_eq!(output.cells.len(), grid.cells.len());
    let mut result = Precheck::default();
    for (actual, expected) in output.cells.iter().zip(&grid.cells) {
        result.glyph_mismatches += u64::from(actual.counts[0] != u32::from(expected.glyph));
    }
    let coverage = output.coverage.as_ref().expect("capture R8");
    assert_eq!(coverage.len(), rendered.coverage().len());
    result.coverage_mismatches = coverage
        .iter()
        .zip(rendered.coverage())
        .filter(|(a, b)| a != b)
        .count() as u64;
    let masks = output.clip_masks.as_ref().expect("capture masks");
    let pre = output.pre_limit_709.as_ref().expect("capture pre-limit");
    let bounded = output.bounded_709.as_ref().expect("capture bounded");
    for len in [
        output.nonlinear_709.len(),
        masks.len(),
        pre.len(),
        bounded.len(),
    ] {
        assert_eq!(len, expected.len());
    }
    for (i, expected) in expected.iter().enumerate() {
        result.mask_mismatches += u64::from(masks[i] != mask(expected.unbounded.components()));
        result.policy_mismatches += u64::from(masks[i] != mask(pre[i].map(f64::from)));
        let actual = output.nonlinear_709[i].map(f64::from);
        let expected = expected.nonlinear.components();
        for c in 0..3 {
            result.policy_mismatches += u64::from(
                !pre[i][c].is_finite()
                    || bounded[i][c].to_bits() != pre[i][c].clamp(0.0, 1.0).to_bits(),
            );
            if !actual[c].is_finite() || !(0.0..=1.0).contains(&actual[c]) {
                result.invalid_components += 1;
            } else {
                let error = (actual[c] - expected[c]).abs();
                result.max_rgb_error = result.max_rgb_error.max(error);
                result.rgb_over_epsilon += u64::from(error > N3_EPSILON);
            }
        }
        for (a, b) in codes(actual).into_iter().zip(codes(expected)) {
            let distance = a.abs_diff(b);
            result.max_code_distance = result.max_code_distance.max(distance);
            result.codes_over_one += u64::from(distance > 1);
        }
    }
    result
}

#[derive(Default)]
struct Samples {
    map: Vec<Duration>,
    a: Vec<Duration>,
    b: Vec<Duration>,
    c: Vec<Duration>,
    stage_wall: Vec<Duration>,
    readback: Vec<Duration>,
    backend_wall: Vec<Duration>,
    batch_wall: Vec<Duration>,
}

fn distribution(values: &[Duration]) -> String {
    if values.is_empty() {
        return "null".into();
    }
    let mut values: Vec<_> = values.iter().map(|v| v.as_secs_f64() * 1000.0).collect();
    values.sort_unstable_by(f64::total_cmp);
    format!(
        "{{\"median_ms\":{},\"p95_ms\":{}}}",
        (values[(values.len() - 1) / 2] + values[values.len() / 2]) / 2.0,
        values[((values.len() - 1) as f64 * 0.95).ceil() as usize]
    )
}

impl Samples {
    fn gpu_frame(&mut self, output: &C3Output, stage_wall: Duration) {
        let t = output.timings;
        self.map.push(t.hdr_map);
        self.a.push(t.linear_render);
        self.b.push(t.method_a);
        self.c.push(t.target_limit);
        self.readback.push(t.readback);
        self.backend_wall.push(t.backend_wall);
        self.stage_wall.push(stage_wall);
    }
    fn json(&self, frames_per_batch: usize) -> String {
        let frames = self.batch_wall.len() * frames_per_batch;
        let wall: f64 = self.batch_wall.iter().map(Duration::as_secs_f64).sum();
        let fps = if wall > 0.0 {
            (frames as f64 / wall).to_string()
        } else {
            "null".into()
        };
        format!(
            "{{\"measured_frames\":{frames},\"frames_per_batch\":{frames_per_batch},\"map\":{},\"pass_a\":{},\"pass_b\":{},\"pass_c\":{},\"host_stage_wall\":{},\"readback\":{},\"backend_frame_wall\":{},\"whole_batch_wall\":{},\"measured_total_wall_seconds\":{wall},\"throughput_fps\":{fps}}}",
            distribution(&self.map),
            distribution(&self.a),
            distribution(&self.b),
            distribution(&self.c),
            distribution(&self.stage_wall),
            distribution(&self.readback),
            distribution(&self.backend_wall),
            distribution(&self.batch_wall)
        )
    }
}

fn bench_cpu(host: &VideoFrame, config: &AsciiConfig, reference: &HdrPqReference<'_>) -> Samples {
    let (w, h) = config.resolved_grid(WIDTH, HEIGHT).unwrap();
    let method = MethodA::new();
    let mut samples = Samples::default();
    for frame in 0..WARMUP + FRAMES {
        let start = Instant::now();
        let grid = reference.map(black_box(host), w, h).unwrap();
        let map = start.elapsed();
        let stage = Instant::now();
        let rendered = reference
            .render_linear(&grid, host.desc(), config.color)
            .unwrap();
        let a = stage.elapsed();
        let stage = Instant::now();
        let mapped: Vec<_> = rendered
            .pixels()
            .iter()
            .map(|p| method.map(*p).unwrap().rgb)
            .collect();
        let b = stage.elapsed();
        let stage = Instant::now();
        let output: Vec<SdrConversionOutput> = black_box(&mapped)
            .iter()
            .map(|p| convert_method_a_rgb(*p).unwrap())
            .collect();
        let c = stage.elapsed();
        black_box(&output);
        drop((output, mapped, rendered, grid));
        let wall = start.elapsed();
        if frame >= WARMUP {
            samples.map.push(map);
            samples.a.push(a);
            samples.b.push(b);
            samples.c.push(c);
            samples.batch_wall.push(wall);
            if (frame - WARMUP + 1) % 50 == 0 {
                eprintln!(
                    "C3 performance CPU: {}/300 measured frames",
                    frame - WARMUP + 1
                );
            }
        }
    }
    samples
}

fn bench_gpu(
    host: &VideoFrame,
    config: &AsciiConfig,
    slots: &mut [&mut VulkanHdrToSdrQualification],
) -> Samples {
    assert!(slots.len() == 1 || slots.len() == 2);
    let measured_batches = FRAMES / slots.len();
    let mut samples = Samples::default();
    for batch in 0..WARMUP + measured_batches {
        let start = Instant::now();
        let mut stages = [Duration::ZERO; 2];
        for (i, slot) in slots.iter_mut().enumerate() {
            let stage = Instant::now();
            slot.stage_host(black_box(host), config, false).unwrap();
            stages[i] = stage.elapsed();
        }
        // Both map waits finish before either A/B/C submission in the dual mode.
        for slot in slots.iter_mut() {
            slot.submit_staged().unwrap();
        }
        for (i, slot) in slots.iter_mut().enumerate() {
            let output = slot.complete().unwrap();
            black_box(&output.nonlinear_709);
            if batch >= WARMUP {
                samples.gpu_frame(&output, stages[i]);
            }
        }
        let wall = start.elapsed();
        if batch >= WARMUP {
            samples.batch_wall.push(wall);
            let count = (batch - WARMUP + 1) * slots.len();
            if count % 50 == 0 {
                eprintln!(
                    "C3 performance Vulkan {} slots: {count}/300 measured frames",
                    slots.len()
                );
            }
        }
    }
    assert_eq!(samples.a.len(), FRAMES);
    samples
}

fn json_string(value: &str) -> String {
    let mut result = String::from("\"");
    for c in value.chars() {
        match c {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            c if c.is_control() => result.push_str(&format!("\\u{:04x}", c as u32)),
            c => result.push(c),
        }
    }
    result.push('"');
    result
}

#[test]
#[ignore = "Intel VAAPI first-frame download, 300 host-input frames per mode; validation must be off"]
fn c3_full_host_input_performance() {
    assert_eq!(
        std::env::var("ASCIIFLOW_VULKAN_VALIDATION").unwrap(),
        "0",
        "performance must explicitly disable validation"
    );
    let root = std::env::var_os("C3_LEGAL_FIXTURE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/codecs")
        });
    let path = root.join(INPUT);
    let hash = input_sha256(&path);
    let destination =
        std::env::var_os("C3_FULL_PERFORMANCE_REPORT").expect("C3_FULL_PERFORMANCE_REPORT");
    let mut report = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .unwrap();
    let host = {
        let mut decoder =
            Decoder::open_pq_qualification(&path, DecodeMode::Vaapi, VaapiOptions::default())
                .unwrap();
        assert_eq!(decoder.info().requirements.codec, VideoCodec::Hevc);
        assert_eq!(decoder.info().requirements.bit_depth, Some(10));
        assert_eq!(
            decoder.info().requirements.chroma_subsampling,
            ChromaSubsampling::Yuv420
        );
        let decoded = decoder
            .next_vaapi_frame()
            .unwrap()
            .expect("first legal HEVC frame");
        downloaded(&decoded)
    };
    assert_eq!((host.desc().width, host.desc().height), (WIDTH, HEIGHT));
    assert_eq!(host.desc().format, PixelFormat::P010Le);
    assert_eq!(host.desc().color_space, ColorSpace::pq_bt2020());
    assert_eq!(host.pts(), Some(0));
    let config = AsciiConfig::default();
    assert!(config.color);
    assert_eq!(config.resolved_grid(WIDTH, HEIGHT).unwrap(), (160, 90));
    let atlas = GlyphAtlas::builtin(&config.font, &config.charset).unwrap();
    let reference = HdrPqReference::new(&atlas, &config.charset).unwrap();
    let mut first = VulkanHdrToSdrQualification::new()
        .unwrap()
        .with_atlas(atlas.clone(), &config)
        .unwrap();
    assert_eq!(first.device_info().vendor_id, 0x8086, "Intel GPU required");
    let validation_observer = first.validation_observer();
    let mut second = first.try_fork().unwrap();
    let check_first = precheck(&mut first, &host, &config, &reference);
    let check_second = precheck(&mut second, &host, &config, &reference);
    let checks_passed = check_first.passed() && check_second.passed();
    let mut cpu = Samples::default();
    let mut single = Samples::default();
    let mut dual = Samples::default();
    if checks_passed && first.validation_error_count() == 0 {
        // End the observational capture epoch before any timing. Both private
        // slots use the same minimal capture-off payload, including the idle
        // second slot during the single-slot run.
        first.prepare(host.desc(), &config, false).unwrap();
        second.prepare(host.desc(), &config, false).unwrap();
        cpu = bench_cpu(&host, &config, &reference);
        single = bench_gpu(&host, &config, &mut [&mut first]);
        dual = bench_gpu(&host, &config, &mut [&mut first, &mut second]);
    }
    let device = format!("{:?}", first.device_info());
    let buffer_bytes = [
        first.total_buffer_bytes_per_slot(),
        second.total_buffer_bytes_per_slot(),
    ];
    drop((second, first));
    let validation_errors = validation_observer();
    let complete = cpu.batch_wall.len() == FRAMES
        && single.batch_wall.len() == FRAMES
        && dual.batch_wall.len() * 2 == FRAMES
        && [&cpu, &single, &dual].into_iter().all(|samples| {
            samples.map.len() == FRAMES
                && samples.a.len() == FRAMES
                && samples.b.len() == FRAMES
                && samples.c.len() == FRAMES
        });
    let passed = checks_passed && complete && validation_errors == 0;
    let body = format!(
        "{{\"status\":\"{}\",\"scope\":\"cached downloaded first-frame host P010; complete CPU reference map+A+B+C vs GPU host stage/map+A+B+C+final/cells/diagnostics readback; excludes decode/download/DMA-BUF/encode; not production qualification\",\"source\":{},\"source_sha256\":\"{hash}\",\"source_frame_index\":0,\"source_pts\":0,\"width\":{WIDTH},\"height\":{HEIGHT},\"grid\":[160,90],\"color\":true,\"atlas\":\"builtin\",\"validation_enabled\":false,\"vulkan_validation_errors\":{validation_errors},\"device\":{},\"warmup_batches_per_mode\":{WARMUP},\"requested_measured_frames_per_mode\":{FRAMES},\"correctness_prechecks\":[{},{}],\"gpu_buffer_bytes_per_slot\":[{},{}],\"cpu_f64_full\":{},\"vulkan_one_slot\":{},\"vulkan_two_slot\":{}}}\n",
        if passed { "PASS" } else { "FAIL" },
        json_string(&path.display().to_string()),
        json_string(&device),
        check_first.json(),
        check_second.json(),
        buffer_bytes[0],
        buffer_bytes[1],
        cpu.json(1),
        single.json(1),
        dual.json(2)
    );
    report.write_all(body.as_bytes()).unwrap();
    report.sync_all().unwrap();
    assert!(
        passed,
        "full host benchmark correctness precheck failed; see report"
    );
    assert_eq!(cpu.batch_wall.len(), FRAMES);
    assert_eq!(single.batch_wall.len(), FRAMES);
    assert_eq!(dual.batch_wall.len() * 2, FRAMES);
}
