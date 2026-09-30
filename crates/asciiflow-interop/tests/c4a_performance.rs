#![cfg(feature = "hdr-to-sdr-qualification")]

//! One-slot C-4A qualification timing, including C-3 and packed host readback.
//! This is not production throughput: no encoder is opened or fed.
use asciiflow_core::{AsciiConfig, ColorSpace, PixelFormat, VideoCodec};
use asciiflow_cpu::sdr_output::{NonlinearBt709Rgb, pack};
use asciiflow_font::GlyphAtlas;
use asciiflow_interop::{DrmPrimeMapping, pack_completed_sdr_surface};
use asciiflow_media::{DecodeMode, Decoder, VaapiOptions, VaapiSdrQualificationPool};
use asciiflow_vulkan::{C3Output, SdrPackOutput, VulkanHdrToSdrQualification};
use std::{
    hint::black_box,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

const WIDTH: u32 = 1920;
const HEIGHT: u32 = 1080;
const WARMUP: usize = 5;
const FRAMES: usize = 300;
const INPUT: &str = "hevc-main10-pq-c3-legal-v1.mp4";

fn checksum(path: &Path) -> String {
    let expected = include_str!("../../../tests/fixtures/codecs/SHA256SUMS")
        .lines()
        .find_map(|line| {
            let mut fields = line.split_whitespace();
            let hash = fields.next()?;
            (fields.next()? == format!("./{INPUT}")).then_some(hash)
        })
        .expect("canonical fixture checksum");
    let result = Command::new("sha256sum").arg(path).output().unwrap();
    assert!(result.status.success());
    let actual = String::from_utf8(result.stdout)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .to_owned();
    assert_eq!(actual, expected, "canonical fixture bytes changed");
    actual
}

fn json_string(value: &str) -> String {
    let mut escaped = String::from("\"");
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            ch if ch.is_control() => escaped.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => escaped.push(ch),
        }
    }
    escaped.push('"');
    escaped
}

#[derive(Default)]
struct Samples {
    hdr_map_wall: Vec<Duration>,
    linear_gpu: Vec<Duration>,
    method_a_gpu: Vec<Duration>,
    limiter_gpu: Vec<Duration>,
    c3_readback_wall: Vec<Duration>,
    c3_backend_wall: Vec<Duration>,
    pack_gpu: Vec<Duration>,
    pack_wall: Vec<Duration>,
    copy_gpu: Vec<Duration>,
    copy_wall: Vec<Duration>,
    total_wall: Vec<Duration>,
}

fn milliseconds(values: &[Duration]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|v| (v.as_secs_f64() * 1000.0).to_string())
            .collect::<Vec<_>>()
            .join(",")
    )
}

impl Samples {
    fn add(
        &mut self,
        c3: &C3Output,
        map_wall: Duration,
        packed: Option<&SdrPackOutput>,
        total: Duration,
    ) {
        self.hdr_map_wall.push(map_wall);
        self.linear_gpu.push(c3.timings.linear_render);
        self.method_a_gpu.push(c3.timings.method_a);
        self.limiter_gpu.push(c3.timings.target_limit);
        self.c3_readback_wall.push(c3.timings.readback);
        self.c3_backend_wall.push(c3.timings.backend_wall);
        if let Some(packed) = packed {
            self.pack_gpu.push(packed.gpu_pack);
            self.pack_wall.push(packed.pack_duration);
            self.copy_gpu.push(packed.gpu_copy);
            self.copy_wall.push(packed.copy_duration);
        }
        self.total_wall.push(total);
    }

    fn json(&self) -> String {
        assert_eq!(self.total_wall.len(), FRAMES);
        let fields = [
            ("hdr_map_staging_wall_ms", &self.hdr_map_wall),
            ("linear_render_gpu_ms", &self.linear_gpu),
            ("method_a_gpu_ms", &self.method_a_gpu),
            ("limiter_gpu_ms", &self.limiter_gpu),
            ("c3_readback_wall_ms", &self.c3_readback_wall),
            ("c3_backend_wall_ms", &self.c3_backend_wall),
            ("pack_gpu_ms", &self.pack_gpu),
            ("pack_wall_ms", &self.pack_wall),
            ("surface_copy_gpu_ms", &self.copy_gpu),
            ("surface_copy_wall_ms", &self.copy_wall),
            ("total_qualification_frame_wall_ms", &self.total_wall),
        ];
        for (_, values) in fields {
            assert!(values.len() == FRAMES || values.is_empty());
        }
        let wall: f64 = self.total_wall.iter().map(Duration::as_secs_f64).sum();
        format!(
            "{{\"measured_frames\":{FRAMES},\"measured_frame_wall_seconds\":{wall},\"qualification_wall_fps\":{},\"samples\":{{{}}}}}",
            FRAMES as f64 / wall,
            fields
                .iter()
                .map(|(name, values)| format!("{}:{}", json_string(name), milliseconds(values)))
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}

fn c3_frame(
    slot: &mut VulkanHdrToSdrQualification,
    mapping: &DrmPrimeMapping<asciiflow_media::VaapiDecodedFrame>,
    desc: &asciiflow_core::FrameDesc,
    pts: Option<i64>,
    config: &AsciiConfig,
) -> (C3Output, Duration) {
    let planes = mapping.duplicate_external_p010_planes().unwrap();
    let start = Instant::now();
    slot.stage_external(desc, pts, config, planes, false)
        .unwrap();
    let map_wall = start.elapsed();
    slot.submit_staged().unwrap();
    let output = slot.complete().unwrap();
    assert_eq!(output.diagnostics[..5], [0; 5]);
    (output, map_wall)
}

fn exact_precheck(
    slot: &mut VulkanHdrToSdrQualification,
    output: &C3Output,
    pool: &VaapiSdrQualificationPool,
    format: PixelFormat,
) {
    let rgb: Vec<_> = output
        .nonlinear_709
        .iter()
        .map(|pixel| NonlinearBt709Rgb(pixel.map(f64::from)))
        .collect();
    let expected = pack(WIDTH, HEIGHT, &rgb, format).unwrap();
    let (surface, packed) =
        pack_completed_sdr_surface(slot, pool.acquire().unwrap(), None).unwrap();
    assert_eq!(packed.diagnostics, [0; 3]);
    assert_eq!(packed.validation_errors, 0);
    let actual = match format {
        PixelFormat::Nv12 => surface.download_nv12(),
        PixelFormat::P010Le => surface.download_p010(),
    }
    .unwrap();
    assert_eq!(
        expected, packed.bytes,
        "exact CPU f64 packing vs GPU packed buffer"
    );
    assert_eq!(
        actual.host().as_slice(),
        packed.bytes,
        "exact actual surface vs GPU packed buffer"
    );
    assert_eq!(packed.buffer_bytes, output_buffer_bytes(format));
}

fn output_buffer_bytes(format: PixelFormat) -> u64 {
    // Tight 4:2:0 packed buffer plus three u32 diagnostic counters.
    u64::from(WIDTH) * u64::from(HEIGHT) * 3 * format.bytes_per_sample() as u64 / 2 + 12
}

#[test]
#[ignore = "real Intel VAAPI/Vulkan C-4A performance; run only after correctness and with GPU idle"]
fn c4a_one_slot_performance() {
    assert!(
        std::env::var_os("ASCIIFLOW_VULKAN_VALIDATION").is_none_or(|value| value == "0"),
        "validation must be OFF for timing"
    );
    let destination = std::env::var_os("C4A_PERFORMANCE_REPORT").expect("C4A_PERFORMANCE_REPORT");
    let mut report = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .unwrap();
    let fixture_root = std::env::var_os("C3_LEGAL_FIXTURE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/codecs")
        });
    let path = fixture_root.join(INPUT);
    let hash = checksum(&path);
    let mut decoder =
        Decoder::open_pq_qualification(&path, DecodeMode::Vaapi, VaapiOptions::default()).unwrap();
    assert_eq!(decoder.info().requirements.codec, VideoCodec::Hevc);
    assert_eq!(decoder.info().requirements.bit_depth, Some(10));
    let frame = decoder
        .next_vaapi_frame()
        .unwrap()
        .expect("first legal HEVC frame");
    let desc = frame.desc().clone();
    let pts = frame.pts();
    assert_eq!((desc.width, desc.height), (WIDTH, HEIGHT));
    assert_eq!(desc.format, PixelFormat::P010Le);
    assert_eq!(desc.color_space, ColorSpace::pq_bt2020());
    assert_eq!(pts, Some(0));
    // Map once and retain the decoded frame for every precheck and measurement.
    let mapping = DrmPrimeMapping::map_direct_read(frame).unwrap();
    drop(decoder);
    let config = AsciiConfig::default();
    assert!(config.color);
    assert_eq!(config.resolved_grid(WIDTH, HEIGHT).unwrap(), (160, 90));
    let atlas = GlyphAtlas::builtin(&config.font, &config.charset).unwrap();
    let mut slot = VulkanHdrToSdrQualification::new()
        .unwrap()
        .with_atlas(atlas, &config)
        .unwrap();
    eprintln!("C-4A qualification device: {:?}", slot.device_info());
    let device = format!("{:?}", slot.device_info());
    let observer = slot.validation_observer();
    let nv12 = VaapiSdrQualificationPool::new(
        Path::new("/dev/dri/renderD128"),
        WIDTH,
        HEIGHT,
        PixelFormat::Nv12,
    )
    .unwrap();
    let p010 = VaapiSdrQualificationPool::new(
        Path::new("/dev/dri/renderD128"),
        WIDTH,
        HEIGHT,
        PixelFormat::P010Le,
    )
    .unwrap();
    let (precheck, _) = c3_frame(&mut slot, &mapping, &desc, pts, &config);
    let c3_buffers = slot.total_buffer_bytes_per_slot();
    exact_precheck(&mut slot, &precheck, &nv12, PixelFormat::Nv12);
    exact_precheck(&mut slot, &precheck, &p010, PixelFormat::P010Le);
    drop(precheck);
    let mut results = Vec::new();
    for (name, pool) in [
        ("c3_only", None),
        ("c3_nv12_surface", Some(&nv12)),
        ("c3_p010_surface", Some(&p010)),
    ] {
        let mut samples = Samples::default();
        for iteration in 0..WARMUP + FRAMES {
            let start = Instant::now();
            let (output, map_wall) = c3_frame(&mut slot, &mapping, &desc, pts, &config);
            let packed = pool.map(|pool| {
                let (surface, packed) =
                    pack_completed_sdr_surface(&mut slot, pool.acquire().unwrap(), None).unwrap();
                assert_eq!(packed.diagnostics, [0; 3]);
                assert_eq!(packed.validation_errors, 0);
                black_box(&surface);
                drop(surface);
                packed
            });
            black_box(&output);
            black_box(&packed);
            let total = start.elapsed();
            if iteration >= WARMUP {
                samples.add(&output, map_wall, packed.as_ref(), total);
            }
        }
        results.push(format!("{}:{}", json_string(name), samples.json()));
    }
    drop((slot, mapping, nv12, p010));
    let validation_errors = observer();
    let nv12_bytes = output_buffer_bytes(PixelFormat::Nv12);
    let p010_bytes = output_buffer_bytes(PixelFormat::P010Le);
    // The C-3 accessor excludes the cached C-4A packer allocations.
    let cached_slot_buffers = c3_buffers + nv12_bytes + p010_bytes;
    writeln!(report,
        "{{\"status\":\"{}\",\"scope\":\"one-slot qualification whole-path wall including C3 float/cell/diagnostic host readback and packed host readback; cached first decoded frame and retained DRM mapping; timed surface allocation/map/import/copy; excludes decode and actual-surface host download; no encoder; not production throughput\",\"hdr_map_timing\":\"staging wall, including prepare/import/input copy/map; not necessarily GPU time\",\"source\":{},\"source_sha256\":{},\"source_frame_index\":0,\"source_pts\":0,\"width\":{WIDTH},\"height\":{HEIGHT},\"grid\":[160,90],\"color\":true,\"atlas\":\"builtin\",\"slots\":1,\"warmup_per_mode\":{WARMUP},\"measured_frames_per_mode\":{FRAMES},\"validation_enabled\":false,\"validation_errors_through_drop\":{validation_errors},\"exact_cpu_pack_and_actual_surface_prechecks\":true,\"device\":{},\"explicit_buffer_bytes\":{{\"c3_per_slot\":{c3_buffers},\"cached_slot_with_both_packers\":{cached_slot_buffers},\"nv12_output\":{nv12_bytes},\"p010_output\":{p010_bytes},\"dual_nv12_additional\":{},\"dual_p010_additional\":{},\"excludes\":\"driver objects, allocator padding, external surfaces\"}},\"modes\":{{{}}}}}",
        if validation_errors == 0 { "passed" } else { "validation_failed" },
        json_string(&path.to_string_lossy()), json_string(&hash), json_string(&device),
        nv12_bytes * 2, p010_bytes * 2, results.join(",")
    ).unwrap();
    report.sync_all().unwrap();
    assert_eq!(validation_errors, 0, "validation errors through teardown");
}
