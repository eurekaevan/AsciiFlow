#![cfg(feature = "hdr-to-sdr-qualification")]

//! Exploratory full-frame CPU/Vulkan diagnostics on the canonical C3 legal inputs.
//! This records measured differences; it does not select or pass a final contract.

use asciiflow_core::{
    AsciiConfig, ChromaSubsampling, ColorSpace, PixelFormat, VideoCodec,
    sdr_target_volume::convert_method_a_rgb, tone_map_bt2446::MethodA,
};
use asciiflow_cpu::hdr::HdrPqReference;
use asciiflow_font::GlyphAtlas;
use asciiflow_interop::DrmPrimeMapping;
use asciiflow_media::{DecodeMode, Decoder, VaapiDecodedFrame, VaapiOptions};
use asciiflow_vulkan::VulkanHdrToSdrQualification;
use ffmpeg_sys_next as ffi;
use std::{
    ffi::CString,
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

const WIDTH: u32 = 1920;
const HEIGHT: u32 = 1080;
const FRAME_COUNT: usize = 300;
const FIXTURES: [(&str, VideoCodec); 2] = [
    ("hevc-main10-pq-c3-legal-v1.mp4", VideoCodec::Hevc),
    ("av1-main10-pq-c3-legal-v1.mp4", VideoCodec::Av1),
];
const DEPTHS: [u32; 4] = [8, 10, 12, 16];

fn downloaded(frame: &VaapiDecodedFrame) -> asciiflow_core::VideoFrame {
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
    let mut storage = asciiflow_core::HostFrame::new_zeroed(desc);
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
    asciiflow_core::VideoFrame::new_host(desc.clone(), frame.pts(), storage).unwrap()
}

fn input_sha256(path: &Path, filename: &str) -> String {
    let manifest = include_str!("../../../tests/fixtures/codecs/SHA256SUMS");
    let expected = manifest
        .lines()
        .find_map(|line| {
            let mut fields = line.split_whitespace();
            let hash = fields.next()?;
            (fields.next()? == format!("./{filename}")).then_some(hash)
        })
        .expect("canonical fixture checksum missing");
    let result = Command::new("sha256sum")
        .arg(path)
        .output()
        .expect("sha256sum required for canonical input verification");
    assert!(result.status.success(), "fixture identity read failed");
    let actual = String::from_utf8(result.stdout)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .to_owned();
    assert_eq!(actual, expected, "canonical input bytes changed");
    actual
}

struct InputFormat(*mut ffi::AVFormatContext);

impl Drop for InputFormat {
    fn drop(&mut self) {
        unsafe { ffi::avformat_close_input(&mut self.0) };
    }
}

fn stream_time_base(path: &Path) -> (i32, i32) {
    let path = CString::new(path.to_string_lossy().as_bytes()).unwrap();
    let mut context = std::ptr::null_mut();
    assert_eq!(
        unsafe {
            ffi::avformat_open_input(
                &mut context,
                path.as_ptr(),
                std::ptr::null(),
                std::ptr::null_mut(),
            )
        },
        0
    );
    let context = InputFormat(context);
    assert!(unsafe { ffi::avformat_find_stream_info(context.0, std::ptr::null_mut()) } >= 0);
    let stream_index = unsafe {
        ffi::av_find_best_stream(
            context.0,
            ffi::AVMediaType::AVMEDIA_TYPE_VIDEO,
            -1,
            -1,
            std::ptr::null_mut(),
            0,
        )
    };
    assert!(stream_index >= 0);
    let stream = unsafe { *(*context.0).streams.add(stream_index as usize) };
    let time_base = unsafe { (*stream).time_base };
    assert!(time_base.num > 0 && time_base.den > 0);
    (time_base.num, time_base.den)
}

#[derive(Default)]
struct DeltaStats {
    exact: u64,
    delta_one: u64,
    over_one: u64,
    max: u32,
    histogram: Vec<u64>,
}

impl DeltaStats {
    fn add(&mut self, delta: u32) {
        if delta == 0 {
            self.exact += 1;
        }
        if delta == 1 {
            self.delta_one += 1;
        }
        if delta > 1 {
            self.over_one += 1;
        }
        self.max = self.max.max(delta);
        if self.histogram.len() <= delta as usize {
            self.histogram.resize(delta as usize + 1, 0);
        }
        self.histogram[delta as usize] += 1;
    }

    fn json(&self) -> String {
        let samples = self.exact + self.delta_one + self.over_one;
        let percentile = |fraction: f64| {
            let rank = (((samples.saturating_sub(1)) as f64) * fraction).ceil() as u64;
            let mut cumulative = 0;
            self.histogram
                .iter()
                .position(|count| {
                    cumulative += count;
                    cumulative > rank
                })
                .unwrap_or(0)
        };
        format!(
            "{{\"exact\":{},\"delta_1\":{},\"over_1\":{},\"max\":{},\"histogram_by_delta\":{},\"p50\":{},\"p95\":{},\"p99\":{},\"p999\":{}}}",
            self.exact,
            self.delta_one,
            self.over_one,
            self.max,
            json_u64(&self.histogram),
            percentile(0.5),
            percentile(0.95),
            percentile(0.99),
            percentile(0.999)
        )
    }
}

#[derive(Default)]
struct CodeHistogram {
    values: Vec<u64>,
    samples: u64,
}

impl CodeHistogram {
    fn add(&mut self, code: u32) {
        if self.values.len() <= code as usize {
            self.values.resize(code as usize + 1, 0);
        }
        self.values[code as usize] += 1;
        self.samples += 1;
    }

    fn percentile(&self, percentile: f64) -> u32 {
        if self.samples == 0 {
            return 0;
        }
        let rank = ((self.samples as f64 - 1.0) * percentile).ceil() as u64;
        let mut cumulative = 0;
        for (code, count) in self.values.iter().enumerate() {
            cumulative += count;
            if cumulative > rank {
                return code as u32;
            }
        }
        self.values.len().saturating_sub(1) as u32
    }

    fn json(&self) -> String {
        format!(
            "{{\"samples\":{},\"p50\":{},\"p95\":{},\"p99\":{},\"p999\":{},\"histogram_by_code\":{}}}",
            self.samples,
            self.percentile(0.50),
            self.percentile(0.95),
            self.percentile(0.99),
            self.percentile(0.999),
            json_u64(&self.values)
        )
    }
}

#[derive(Default)]
struct YuvStats {
    cpu: [CodeHistogram; 3],
    gpu: [CodeHistogram; 3],
    delta: [DeltaStats; 3],
}

impl YuvStats {
    fn json(&self) -> String {
        format!(
            "{{\"cpu\":[{}],\"gpu\":[{}],\"code_delta\":[{}]}}",
            self.cpu
                .iter()
                .map(CodeHistogram::json)
                .collect::<Vec<_>>()
                .join(","),
            self.gpu
                .iter()
                .map(CodeHistogram::json)
                .collect::<Vec<_>>()
                .join(","),
            self.delta
                .iter()
                .map(DeltaStats::json)
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}

#[derive(Default)]
struct FixtureStats {
    filename: String,
    input_sha256: String,
    frames: usize,
    time_base: (i32, i32),
    first_pts: Option<i64>,
    last_pts: Option<i64>,
    max_nonlinear_abs: [f64; 3],
    mask_pixels_mismatched: u64,
    mask_channels_mismatched: u64,
    glyph_mismatches: u64,
    coverage_mismatches: u64,
    pixels: u64,
    quantized: [DeltaStats; 4],
    yuv444_8: YuvStats,
    yuv444_10: YuvStats,
    first_over_two: Vec<String>,
    validation_errors: usize,
}

impl FixtureStats {
    fn json(&self) -> String {
        format!(
            "{{\"source\":\"{}\",\"input_sha256\":\"{}\",\"frames\":{},\"time_base\":[{},{}],\"first_pts\":{},\"last_pts\":{},\"pixels\":{},\"max_nonlinear_abs_by_rgb\":[{},{},{}],\"mask_mismatched_pixels\":{},\"mask_mismatched_channels\":{},\"glyph_mismatches\":{},\"coverage_mismatches\":{},\"unorm\":{{\"8\":{},\"10\":{},\"12\":{},\"16\":{}}},\"limited_bt709_444_8bit\":{},\"limited_bt709_444_10bit\":{},\"first_over_2_unorm16\":[{}],\"vulkan_validation_errors\":{}}}",
            json_escape(&self.filename),
            self.input_sha256,
            self.frames,
            self.time_base.0,
            self.time_base.1,
            option_i64(self.first_pts),
            option_i64(self.last_pts),
            self.pixels,
            self.max_nonlinear_abs[0],
            self.max_nonlinear_abs[1],
            self.max_nonlinear_abs[2],
            self.mask_pixels_mismatched,
            self.mask_channels_mismatched,
            self.glyph_mismatches,
            self.coverage_mismatches,
            self.quantized[0].json(),
            self.quantized[1].json(),
            self.quantized[2].json(),
            self.quantized[3].json(),
            self.yuv444_8.json(),
            self.yuv444_10.json(),
            self.first_over_two.join(","),
            self.validation_errors,
        )
    }
}

fn quantize_unorm(value: f64, depth: u32) -> u32 {
    let max_code = (1u32 << depth) - 1;
    (value.clamp(0.0, 1.0) * f64::from(max_code) + 0.5).floor() as u32
}

fn clip_mask(rgb: [f64; 3]) -> u32 {
    rgb.into_iter()
        .enumerate()
        .fold(0, |mask, (channel, value)| {
            mask | if value < 0.0 {
                1 << channel
            } else if value > 1.0 {
                1 << (channel + 3)
            } else {
                0
            }
        })
}

/// Test-only BT.709 NCL limited-range 4:4:4 code diagnostic. C3's output transfer
/// remains its existing display-power encoding; this does not assert production
/// BT.709 OETF conformance or apply 4:2:0 chroma averaging.
fn limited_bt709_444(rgb: [f64; 3], depth: u32) -> [u32; 3] {
    const KR: f64 = 0.2126;
    const KG: f64 = 0.7152;
    const KB: f64 = 0.0722;
    let [r, g, b] = rgb;
    let y = KR * r + KG * g + KB * b;
    let cb = (b - y) / (2.0 * (1.0 - KB));
    let cr = (r - y) / (2.0 * (1.0 - KR));
    let shift = depth - 8;
    let y_code = ((16u32 << shift) as f64 + f64::from(219u32 << shift) * y + 0.5).floor();
    let c_center = (128u32 << shift) as f64;
    let c_scale = f64::from(224u32 << shift);
    let max_code = ((1u32 << depth) - 1) as f64;
    [
        y_code.clamp((16u32 << shift) as f64, (235u32 << shift) as f64) as u32,
        (c_center + c_scale * cb + 0.5).floor().clamp(
            (16u32 << shift) as f64,
            max_code.min((240u32 << shift) as f64),
        ) as u32,
        (c_center + c_scale * cr + 0.5).floor().clamp(
            (16u32 << shift) as f64,
            max_code.min((240u32 << shift) as f64),
        ) as u32,
    ]
}

fn update_yuv(stats: &mut YuvStats, cpu: [f64; 3], gpu: [f32; 3], depth: u32) {
    let cpu_codes = limited_bt709_444(cpu, depth);
    let gpu_codes = limited_bt709_444(gpu.map(f64::from), depth);
    for channel in 0..3 {
        stats.cpu[channel].add(cpu_codes[channel]);
        stats.gpu[channel].add(gpu_codes[channel]);
        stats.delta[channel].add(cpu_codes[channel].abs_diff(gpu_codes[channel]));
    }
}

fn audit_fixture(
    path: &Path,
    filename: &str,
    codec: VideoCodec,
    config: &AsciiConfig,
    reference: &HdrPqReference<'_>,
    atlas: &GlyphAtlas,
) -> FixtureStats {
    let hash = input_sha256(path, filename);
    let source_time_base = stream_time_base(path);
    let mut decoder =
        Decoder::open_pq_qualification(path, DecodeMode::Vaapi, VaapiOptions::default()).unwrap();
    let requirements = &decoder.info().requirements;
    let desc = &decoder.info().frame_desc;
    assert_eq!(requirements.codec, codec, "{filename} codec");
    assert_eq!(requirements.bit_depth, Some(10), "{filename} bit depth");
    assert_eq!(requirements.chroma_subsampling, ChromaSubsampling::Yuv420);
    assert_eq!(desc.format, PixelFormat::P010Le);
    assert_eq!((desc.width, desc.height), (WIDTH, HEIGHT));
    assert_eq!(desc.color_space, ColorSpace::pq_bt2020());
    assert_eq!(decoder.info().frame_count, Some(FRAME_COUNT as u64));
    assert_eq!(decoder.info().frame_rate.numerator, 50);
    assert_eq!(decoder.info().frame_rate.denominator, 1);

    let mut gpu = VulkanHdrToSdrQualification::new()
        .unwrap()
        .with_atlas(atlas.clone(), config)
        .unwrap();
    let (grid_width, grid_height) = config.resolved_grid(WIDTH, HEIGHT).unwrap();
    let method_a = MethodA::new();
    let mut stats = FixtureStats {
        filename: filename.into(),
        input_sha256: hash,
        time_base: source_time_base,
        ..FixtureStats::default()
    };
    let mut expected_pts = None;

    while let Some(decoded) = decoder.next_vaapi_frame().unwrap() {
        let frame_index = stats.frames;
        let host = downloaded(&decoded);
        let pts = host.pts().expect("canonical C3 frame has PTS");
        assert_eq!(pts, (frame_index as i64) * 1000, "{filename} frame PTS");
        if let Some(previous) = expected_pts {
            assert_eq!(pts - previous, 1000, "{filename} PTS step");
        }
        expected_pts = Some(pts);
        stats.first_pts.get_or_insert(pts);
        stats.last_pts = Some(pts);

        let grid = reference.map(&host, grid_width, grid_height).unwrap();
        let rendered = reference.render_linear(&grid, host.desc(), true).unwrap();
        let mapping = DrmPrimeMapping::map_direct_read(decoded).unwrap();
        gpu.submit_external(
            host.desc(),
            mapping.pts(),
            config,
            mapping.duplicate_external_p010_planes().unwrap(),
            true,
        )
        .unwrap();
        let output = gpu.complete().unwrap();
        // Ordinary qualification dispatch uses the C3B-selected f32 arithmetic.
        // Capture is observational, not a per-frame experiment selector.
        assert_eq!(output.diagnostics[..5], [0; 5]);
        assert_eq!((output.width, output.height), (WIDTH, HEIGHT));
        assert_eq!(output.pts, Some(pts));
        assert_eq!(output.cells.len(), grid.cells.len());
        let coverage = output.coverage.as_ref().expect("capture enabled");
        assert_eq!(coverage.len(), rendered.coverage().len());
        for (gpu_cell, cpu_cell) in output.cells.iter().zip(&grid.cells) {
            stats.glyph_mismatches += u64::from(gpu_cell.counts[0] != u32::from(cpu_cell.glyph));
        }
        stats.coverage_mismatches += coverage
            .iter()
            .zip(rendered.coverage())
            .filter(|(actual, expected)| actual != expected)
            .count() as u64;

        let gpu_masks = output.clip_masks.as_ref().expect("capture enabled");
        let gpu_pre = output.pre_limit_709.as_ref().expect("capture enabled");
        assert_eq!(gpu_masks.len(), rendered.pixels().len());
        assert_eq!(output.nonlinear_709.len(), rendered.pixels().len());
        for (index, pixel) in rendered.pixels().iter().enumerate() {
            let cpu = convert_method_a_rgb(method_a.map(*pixel).unwrap().rgb).unwrap();
            let cpu_rgb = cpu.nonlinear.components();
            let expected_mask = clip_mask(cpu.unbounded.components());
            let actual_mask = gpu_masks[index];
            if actual_mask != expected_mask {
                stats.mask_pixels_mismatched += 1;
                stats.mask_channels_mismatched +=
                    u64::from((actual_mask ^ expected_mask).count_ones());
            }
            let gpu_rgb = output.nonlinear_709[index];
            assert!(
                gpu_rgb
                    .iter()
                    .all(|sample| sample.is_finite() && (0.0..=1.0).contains(sample)),
                "invalid final RGB frame {frame_index} pixel {index}"
            );
            let mut u16_delta = 0;
            for channel in 0..3 {
                stats.max_nonlinear_abs[channel] = stats.max_nonlinear_abs[channel]
                    .max((f64::from(gpu_rgb[channel]) - cpu_rgb[channel]).abs());
                for (depth_index, depth) in DEPTHS.into_iter().enumerate() {
                    let delta = quantize_unorm(cpu_rgb[channel], depth)
                        .abs_diff(quantize_unorm(f64::from(gpu_rgb[channel]), depth));
                    stats.quantized[depth_index].add(delta);
                    if depth == 16 {
                        u16_delta = u16_delta.max(delta);
                    }
                }
            }
            update_yuv(&mut stats.yuv444_8, cpu_rgb, gpu_rgb, 8);
            update_yuv(&mut stats.yuv444_10, cpu_rgb, gpu_rgb, 10);
            if u16_delta > 2 && stats.first_over_two.len() < 16 {
                let x = index as u32 % WIDTH;
                let y = index as u32 / WIDTH;
                stats.first_over_two.push(format!(
                    "{{\"frame\":{frame_index},\"pts\":{pts},\"xy\":[{x},{y}],\"input_linear_rgb_nits\":[{:.17e},{:.17e},{:.17e}],\"cpu_pre_limit_rgb\":[{:.17e},{:.17e},{:.17e}],\"gpu_pre_limit_rgb\":[{:.9e},{:.9e},{:.9e}],\"cpu_final_rgb\":[{:.17e},{:.17e},{:.17e}],\"gpu_final_rgb\":[{:.9e},{:.9e},{:.9e}],\"cpu_clip_mask\":{},\"gpu_clip_mask\":{},\"max_unorm16_delta\":{u16_delta}}}",
                    pixel.r, pixel.g, pixel.b,
                    cpu.unbounded.r, cpu.unbounded.g, cpu.unbounded.b,
                    gpu_pre[index][0], gpu_pre[index][1], gpu_pre[index][2],
                    cpu_rgb[0], cpu_rgb[1], cpu_rgb[2],
                    gpu_rgb[0], gpu_rgb[1], gpu_rgb[2],
                    expected_mask, actual_mask
                ));
            }
        }
        stats.pixels += rendered.pixels().len() as u64;
        stats.frames += 1;
        stats.validation_errors = gpu.validation_error_count();
        eprintln!(
            "C3 precision exploratory: {filename} frame {}/{}",
            stats.frames, FRAME_COUNT
        );
    }
    assert_eq!(stats.frames, FRAME_COUNT, "{filename} decoded frame count");
    assert_eq!(stats.time_base, (1, 50000), "{filename} stream time base");
    assert_eq!(stats.first_pts, Some(0));
    assert_eq!(stats.last_pts, Some(299000));
    assert_eq!(stats.validation_errors, 0, "{filename} Vulkan validation");
    assert_eq!(stats.glyph_mismatches, 0);
    assert_eq!(stats.coverage_mismatches, 0);
    assert_eq!(stats.mask_pixels_mismatched, 0);
    assert!(stats.max_nonlinear_abs.iter().all(|v| *v <= 1.0 / 1792.0));
    for channel in &stats.yuv444_10.delta {
        assert!(channel.max <= 1);
        assert_eq!(channel.over_one, 0);
    }
    stats
}

#[test]
#[ignore = "full 300-frame Intel VAAPI/DMA-BUF to Vulkan C3 precision diagnostics; exploratory only"]
fn c3_precision_real_full_frame_diagnostics() {
    let root = std::env::var_os("C3_LEGAL_FIXTURE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/codecs")
        });
    let config = AsciiConfig::default();
    let atlas = GlyphAtlas::builtin(&config.font, &config.charset).unwrap();
    let reference = HdrPqReference::new(&atlas, &config.charset).unwrap();
    let reports = FIXTURES
        .iter()
        .map(|(filename, codec)| {
            audit_fixture(
                &root.join(filename),
                filename,
                codec.clone(),
                &config,
                &reference,
                &atlas,
            )
        })
        .collect::<Vec<_>>();
    let destination = std::env::var_os("C3_PRECISION_REAL_REPORT")
        .expect("set C3_PRECISION_REAL_REPORT to a new report path");
    let body = format!(
        "{{\"schema_version\":1,\"scope\":\"C3B selected f32 mode262 full-frame N3 qualification\",\"diagnostic_transfer_note\":\"limited BT.709 NCL matrix applied to existing C3 nonlinear output; output transfer remains display-power; 4:4:4 with no subsampling\",\"reports\":[{}]}}\n",
        reports
            .iter()
            .map(FixtureStats::json)
            .collect::<Vec<_>>()
            .join(",")
    );
    let mut report = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .expect("create C3_PRECISION_REAL_REPORT exclusively");
    report
        .write_all(body.as_bytes())
        .expect("write C3_PRECISION_REAL_REPORT");
}

fn json_u64(values: &[u64]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn option_i64(value: Option<i64>) -> String {
    value.map_or_else(|| "null".into(), |value| value.to_string())
}

fn json_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn fd_count() -> usize {
    std::fs::read_dir("/proc/self/fd").unwrap().count()
}

fn output_hash(output: &asciiflow_vulkan::C3Output) -> String {
    use std::process::Stdio;
    let mut child = Command::new("sha256sum")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    // Canonical little-endian IEEE f32 bits, not text formatting or quantization.
    let mut bytes = Vec::with_capacity(output.nonlinear_709.len() * 12);
    for pixel in &output.nonlinear_709 {
        for sample in pixel {
            assert!(sample.is_finite() && (0.0..=1.0).contains(sample));
            bytes.extend_from_slice(&sample.to_bits().to_le_bytes());
        }
    }
    child.stdin.take().unwrap().write_all(&bytes).unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(result.status.success());
    String::from_utf8(result.stdout)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .to_owned()
}

#[test]
#[ignore = "real Intel VAAPI/DMA-BUF, 3000 frames per codec, two private C3 slots"]
fn c3_selected_dual_slot_3000_frame_stress() {
    assert_eq!(std::env::var("ASCIIFLOW_VULKAN_VALIDATION").unwrap(), "1");
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/codecs");
    let config = AsciiConfig::default();
    let atlas = GlyphAtlas::builtin(&config.font, &config.charset).unwrap();
    let mut reports = Vec::new();
    for (name, _) in FIXTURES {
        let path = root.join(name);
        let input_hash = input_sha256(&path, name);
        let before = fd_count();
        let mut peak = before;
        let mut hashes = Vec::with_capacity(FRAME_COUNT);
        let mut total = 0;
        let mut steady_fds = None;
        let mut rss_kib = Vec::new();
        let payload;
        {
            let first = VulkanHdrToSdrQualification::new()
                .unwrap()
                .with_atlas(atlas.clone(), &config)
                .unwrap();
            assert_eq!(first.device_info().vendor_id, 0x8086);
            assert_eq!(first.device_info().device_id, 0x7d55);
            let second = first.try_fork().unwrap();
            let observe_validation = first.validation_observer();
            let mut slots = [first, second];
            let mut expected_payload = None;
            for cycle in 0..10 {
                // Alternate private slots for the same source frame on each
                // cycle: the first-cycle hashes also detect slot contamination.
                if cycle > 0 {
                    slots.swap(0, 1);
                }
                let mut decoder = Decoder::open_pq_qualification(
                    &path,
                    DecodeMode::Vaapi,
                    VaapiOptions::default(),
                )
                .unwrap();
                for pair in 0..FRAME_COUNT / 2 {
                    // Keep both source mappings alive until both fences finish.
                    // Both copy/map stages finish before either A/B/C submit;
                    // no intervening map fence drains the first C3 submission.
                    // No claim of parallel hardware execution on one queue.
                    let mut mappings = Vec::with_capacity(2);
                    for slot in &mut slots {
                        let decoded = decoder.next_vaapi_frame().unwrap().unwrap();
                        let desc = decoded.desc().clone();
                        let mapping = DrmPrimeMapping::map_direct_read(decoded).unwrap();
                        slot.stage_external(
                            &desc,
                            mapping.pts(),
                            &config,
                            mapping.duplicate_external_p010_planes().unwrap(),
                            false,
                        )
                        .unwrap();
                        mappings.push(mapping);
                    }
                    for slot in &mut slots {
                        slot.submit_staged().unwrap();
                    }
                    peak = peak.max(fd_count());
                    for (index, slot) in slots.iter_mut().enumerate() {
                        let frame = pair * 2 + index;
                        let output = slot.complete().unwrap();
                        assert_eq!(output.pts, Some(frame as i64 * 1000));
                        assert_eq!((output.width, output.height), (WIDTH, HEIGHT));
                        assert_eq!(&output.diagnostics[..5], &[0; 5]);
                        assert_eq!(slot.validation_error_count(), 0);
                        let hash = output_hash(&output);
                        if cycle == 0 {
                            hashes.push(hash);
                        } else {
                            assert_eq!(hash, hashes[frame], "{name} cycle {cycle} frame {frame}");
                        }
                        let bytes = slot.total_buffer_bytes_per_slot();
                        if let Some(expected) = expected_payload {
                            assert_eq!(bytes, expected, "C3 payload grew");
                        } else {
                            expected_payload = Some(bytes);
                        }
                        total += 1;
                    }
                    drop(mappings);
                    // Use the complete first cycle as decoder/import warmup;
                    // decoder reference-surface growth is not a C3 FD leak.
                    if cycle == 0 && pair == FRAME_COUNT / 2 - 1 {
                        steady_fds = Some(fd_count());
                    } else if let Some(steady) = steady_fds {
                        assert!(fd_count() <= steady, "{name} steady-state FD growth");
                    }
                }
                assert!(decoder.next_vaapi_frame().unwrap().is_none());
                let status = std::fs::read_to_string("/proc/self/status").unwrap();
                let rss = status
                    .lines()
                    .find(|line| line.starts_with("VmRSS:"))
                    .unwrap()
                    .split_whitespace()
                    .nth(1)
                    .unwrap()
                    .parse::<u64>()
                    .unwrap();
                rss_kib.push(rss);
                eprintln!("C3 stress {name}: {total}/3000 frames, FD peak={peak}");
            }
            payload = expected_payload.unwrap();
            drop(slots);
            assert_eq!(observe_validation(), 0, "slot/device teardown validation");
            drop(observe_validation);
        }
        let after = fd_count();
        assert_eq!(total, 3000);
        assert_eq!(
            after, before,
            "{name} leaked FDs after complete resource drop"
        );
        reports.push(format!(
            "{{\"source\":\"{name}\",\"input_sha256\":\"{input_hash}\",\"frames\":{total},\"slots\":2,\"fd_before\":{before},\"fd_sampled_peak\":{peak},\"fd_after\":{after},\"bytes_per_slot\":{payload},\"rss_kib_after_cycles\":{rss_kib:?},\"validation_errors_through_slot_teardown\":0,\"frame_hashes\":[{}]}}",
            hashes.iter().map(|hash| format!("\"{hash}\"")).collect::<Vec<_>>().join(",")
        ));
    }
    let destination = std::env::var_os("C3_STRESS_REPORT").expect("set new C3_STRESS_REPORT path");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .unwrap();
    writeln!(file, "{{\"scope\":\"C3B selected f32 mode262; repeat each 300-frame source ten times; source PTS resets each cycle; no encoded output\",\"reports\":[{}]}}", reports.join(",")).unwrap();
}
