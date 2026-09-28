#![cfg(feature = "hdr-pq-qualification")]

//! Synthetic host-P010 qualification only; this does not exercise decoder or encoder interop.

use asciiflow_core::hdr_pq::{self, PEAK_NITS};
use asciiflow_core::{
    AsciiBackend, AsciiConfig, BackendTimings, ChromaLocation, ColorMatrix, ColorPrimaries,
    ColorRange, ColorSpace, FrameDesc, HostFrame, PipelineStage, TransferCharacteristic,
    VideoFrame,
};
use asciiflow_cpu::hdr::HdrPqReference;
use asciiflow_font::GlyphAtlas;
use asciiflow_vulkan::{GpuPqCell, PqQualificationFault, VulkanPqQualification};
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

// Keep process-wide FD observations meaningful even with the default test runner.
static VULKAN_TEST_LOCK: Mutex<()> = Mutex::new(());

// Intel synthetic characterization (2026-09-28): maxima were 1.75822e-5
// normalized absolute RGB/luminance, 2.71030e-5 relative, and 8.03804e-6 PQ.
// These qualification gates allow roughly twice that measured error; they do
// not describe every Vulkan implementation or relax the final one-code gate.
const LINEAR_ABSOLUTE: f64 = 4e-5;
const LINEAR_RELATIVE: f64 = 6e-5;
const PERCEPTUAL_ABSOLUTE: f64 = 2e-5;

#[derive(Clone, Copy, Debug)]
enum Pattern {
    Neutral(u16),
    GraySteps,
    ColorBoundary,
    TenBitSteps,
}

fn frame(width: u32, height: u32, pattern: Pattern, pts: i64) -> VideoFrame {
    let desc = FrameDesc::host_p010_le(
        width,
        height,
        ColorSpace {
            matrix: ColorMatrix::Bt2020,
            range: ColorRange::Limited,
            primaries: ColorPrimaries::Bt2020,
            transfer: TransferCharacteristic::Pq,
            chroma_location: ChromaLocation::Left,
        },
    )
    .unwrap();
    let mut storage = HostFrame::new_zeroed(&desc);
    let (y_plane, uv_plane) = storage.planes_mut(&desc);
    // Values include black, near black, gray, reference white and the PQ ceiling.
    const GRAYS: [u16; 12] = [64, 65, 66, 80, 100, 180, 400, 500, 501, 700, 900, 940];
    for y in 0..height as usize {
        for x in 0..width as usize {
            let code = match pattern {
                Pattern::Neutral(code) => code,
                Pattern::GraySteps => GRAYS[x * GRAYS.len() / width as usize],
                Pattern::ColorBoundary => 300 + ((x * 7 + y * 11) % 400) as u16,
                Pattern::TenBitSteps => 500 + ((x + 3 * y) % 4) as u16,
            };
            store(y_plane, y * width as usize + x, code);
        }
    }
    for y in 0..height as usize / 2 {
        for x in 0..width as usize / 2 {
            let (cb, cr) = match pattern {
                // Deliberately cross both chroma-block and cell boundaries.
                Pattern::ColorBoundary => match (x + y) % 4 {
                    0 => (64, 960),
                    1 => (960, 64),
                    2 => (400, 600),
                    _ => (600, 400),
                },
                _ => (512, 512),
            };
            let index = y * width as usize + x * 2;
            store(uv_plane, index, cb);
            store(uv_plane, index + 1, cr);
        }
    }
    VideoFrame::new_host(desc, Some(pts), storage).unwrap()
}

fn store(plane: &mut [u8], index: usize, code: u16) {
    plane[index * 2..index * 2 + 2].copy_from_slice(&(code << 6).to_le_bytes());
}

fn codes(plane: &[u8]) -> Vec<u16> {
    plane
        .chunks_exact(2)
        .map(|sample| {
            let value = u16::from_le_bytes([sample[0], sample[1]]);
            assert_eq!(value & 63, 0, "P010 low six bits must be zero");
            value >> 6
        })
        .collect()
}

#[derive(Default)]
struct IntermediateError {
    rgb_absolute: [f64; 3],
    rgb_relative: [f64; 3],
    luminance_absolute: f64,
    luminance_relative: f64,
    perceptual_absolute: f64,
}

fn measure(actual: f64, expected: f64, absolute: &mut f64, relative: &mut f64) {
    assert!(actual.is_finite(), "nonfinite GPU value {actual}");
    let difference = (actual - expected).abs();
    assert!(
        difference <= LINEAR_ABSOLUTE,
        "normalized absolute error {difference:.10e}: GPU={actual:.10e}, CPU={expected:.10e}"
    );
    *absolute = absolute.max(difference);
    // Relative error is undefined at black; the absolute metric covers that case.
    if expected != 0.0 {
        let relative_error = difference / expected.abs();
        assert!(
            relative_error <= LINEAR_RELATIVE,
            "relative error {relative_error:.10e}: GPU={actual:.10e}, CPU={expected:.10e}"
        );
        *relative = relative.max(relative_error);
    }
}

fn compare_cells(
    label: &str,
    gpu: &[GpuPqCell],
    input: &VideoFrame,
    config: &AsciiConfig,
    reference: &HdrPqReference<'_>,
) {
    let (width, height) = config
        .resolved_grid(input.desc().width, input.desc().height)
        .unwrap();
    let cpu = reference.map(input, width, height).unwrap();
    assert_eq!(gpu.len(), cpu.cells.len(), "{label}: cell count");
    let mut clamped = 0u64;
    let mut error = IntermediateError::default();
    for (index, (actual, expected)) in gpu.iter().zip(&cpu.cells).enumerate() {
        assert_eq!(
            actual.counts[0],
            u32::from(expected.glyph),
            "{label}: glyph {index}"
        );
        clamped += u64::from(actual.counts[1]);
        assert_eq!(
            actual.counts[2], 0,
            "{label}: invalid components in cell {index}"
        );
        let channels = [
            expected.foreground_nits.r,
            expected.foreground_nits.g,
            expected.foreground_nits.b,
        ];
        for (channel, nits) in channels.into_iter().enumerate() {
            measure(
                f64::from(actual.linear_and_perceptual[channel]),
                nits / PEAK_NITS,
                &mut error.rgb_absolute[channel],
                &mut error.rgb_relative[channel],
            );
        }
        let luminance = hdr_pq::KR * f64::from(actual.linear_and_perceptual[0])
            + hdr_pq::KG * f64::from(actual.linear_and_perceptual[1])
            + hdr_pq::KB * f64::from(actual.linear_and_perceptual[2]);
        measure(
            luminance,
            expected.foreground_nits.luminance() / PEAK_NITS,
            &mut error.luminance_absolute,
            &mut error.luminance_relative,
        );
        let expected_pq = hdr_pq::pq_inverse_eotf(expected.foreground_nits.luminance()).unwrap();
        let actual_pq = f64::from(actual.linear_and_perceptual[3]);
        assert!(actual_pq.is_finite());
        assert!(
            (actual_pq - expected_pq).abs() <= PERCEPTUAL_ABSOLUTE,
            "{label}: cell {index} PQ GPU={actual_pq:.10e}, CPU={expected_pq:.10e}"
        );
        error.perceptual_absolute = error
            .perceptual_absolute
            .max((actual_pq - expected_pq).abs());
    }
    assert_eq!(
        clamped, cpu.clamped_input_components,
        "{label}: input clamp count"
    );
    // Keep reporting the measured error as well as enforcing the characterized bounds.
    eprintln!(
        "{label}: RGB normalized absolute={:?} relative={:?}; luminance normalized absolute={:.10e} relative={:.10e}; PQ absolute={:.10e}",
        error.rgb_absolute,
        error.rgb_relative,
        error.luminance_absolute,
        error.luminance_relative,
        error.perceptual_absolute
    );
}

fn compare_plane(label: &str, actual: &[u16], expected: &[u16], exact: bool) {
    assert_eq!(actual.len(), expected.len());
    let mut delta: Vec<_> = actual
        .iter()
        .zip(expected)
        .map(|(&a, &b)| a.abs_diff(b))
        .collect();
    delta.sort_unstable();
    let percentile = |per_mille: usize| delta[((delta.len() - 1) * per_mille).div_ceil(1000)];
    let max = *delta.last().unwrap();
    let zeros = delta.iter().filter(|&&d| d == 0).count();
    let ones = delta.iter().filter(|&&d| d == 1).count();
    let over_one = delta.len() - zeros - ones;
    eprintln!(
        "{label}: samples={} delta0={zeros} delta1={ones} delta_gt1={over_one} max={max} p50={} p95={} p99={} p99.9={}",
        delta.len(),
        percentile(500),
        percentile(950),
        percentile(990),
        percentile(999)
    );
    assert!(
        max <= if exact { 0 } else { 1 },
        "{label}: output code delta {max}"
    );
}

fn compare_output(label: &str, actual: &VideoFrame, expected: &VideoFrame, exact: bool) {
    assert_eq!(actual.desc(), expected.desc(), "{label}: descriptor");
    assert_eq!(actual.pts(), expected.pts(), "{label}: pts/order");
    let (actual_y, actual_uv) = actual.host().planes(actual.desc());
    let (expected_y, expected_uv) = expected.host().planes(expected.desc());
    assert!(
        codes(actual_y).iter().all(|code| (64..=940).contains(code)),
        "{label}: output Y range"
    );
    assert!(
        codes(actual_uv)
            .iter()
            .all(|code| (64..=960).contains(code)),
        "{label}: output UV range"
    );
    compare_plane(
        &format!("{label}/Y"),
        &codes(actual_y),
        &codes(expected_y),
        exact,
    );
    let actual_uv = codes(actual_uv);
    let expected_uv = codes(expected_uv);
    for (channel, name) in [(0, "U"), (1, "V")] {
        let actual: Vec<_> = actual_uv.iter().skip(channel).step_by(2).copied().collect();
        let expected: Vec<_> = expected_uv
            .iter()
            .skip(channel)
            .step_by(2)
            .copied()
            .collect();
        compare_plane(&format!("{label}/{name}"), &actual, &expected, exact);
    }
}

fn exercise_atlas(atlas: GlyphAtlas, config: AsciiConfig, label: &str) {
    let reference = HdrPqReference::new(&atlas, &config.charset).unwrap();
    let mut gpu = VulkanPqQualification::new()
        .unwrap()
        .with_atlas(atlas.clone(), &config)
        .unwrap();
    let cases = [
        ("black", 64, 48, Pattern::Neutral(64), 7, 5),
        ("near-black", 64, 48, Pattern::Neutral(65), 7, 5),
        ("near-white", 64, 48, Pattern::Neutral(939), 7, 5),
        (
            "100-nit-reference",
            64,
            48,
            Pattern::Neutral(neutral_nits_code(100.0)),
            7,
            5,
        ),
        (
            "1000-nit-highlight",
            64,
            48,
            Pattern::Neutral(neutral_nits_code(1000.0)),
            7,
            5,
        ),
        ("10000-nit-highlight", 64, 48, Pattern::Neutral(940), 7, 5),
        ("gray-levels", 96, 48, Pattern::GraySteps, 12, 3),
        ("chroma-boundaries", 66, 50, Pattern::ColorBoundary, 7, 3),
        ("ten-bit-steps", 64, 48, Pattern::TenBitSteps, 7, 5),
        ("large-cell", 256, 256, Pattern::ColorBoundary, 1, 1),
    ];
    for color in [true, false] {
        for (index, (name, width, height, pattern, gw, gh)) in cases.iter().enumerate() {
            let config = AsciiConfig {
                grid_width: *gw,
                grid_height: Some(*gh),
                color,
                ..config.clone()
            };
            let input = frame(*width, *height, *pattern, index as i64);
            let label = format!("{label}/{name}/color={color}");
            let mapped = gpu.map_cells(&input, &config).unwrap();
            compare_cells(&label, &mapped, &input, &config, &reference);
            // Independent physical reference levels use the Stage 5.3B-1
            // one-code quantization bounds, not a GPU-relative tolerance.
            let vector = match *name {
                "black" => Some((0.0, 0.01)),
                "100-nit-reference" => Some((100.0, 0.1)),
                "1000-nit-highlight" => Some((1000.0, 5.0)),
                "10000-nit-highlight" => Some((10000.0, 0.01)),
                _ => None,
            };
            if let Some((nits, quantization_bound)) = vector {
                for cell in &mapped {
                    let rgb = cell.linear_and_perceptual;
                    let luminance_nits = PEAK_NITS
                        * (hdr_pq::KR * f64::from(rgb[0])
                            + hdr_pq::KG * f64::from(rgb[1])
                            + hdr_pq::KB * f64::from(rgb[2]));
                    assert!(
                        (luminance_nits - nits).abs() <= quantization_bound,
                        "{label}: physical PQ vector {luminance_nits} vs {nits} nits"
                    );
                }
            }
            let (expected, diagnostics) = reference.process(&input, *gw, *gh, color).unwrap();
            assert_eq!(diagnostics.nonfinite_components, 0);
            let actual = gpu.process(&input, &config).unwrap();
            let cells = gpu.diagnostics().unwrap();
            let input_clips: u64 = cells.iter().map(|c| u64::from(c.counts[1])).sum();
            let output_clips: u64 = cells.iter().map(|c| u64::from(c.counts[3])).sum();
            assert_eq!(
                input_clips + output_clips,
                diagnostics.clamped_components,
                "{label}: CPU/GPU clamp diagnostics"
            );
            assert!(cells.iter().all(|c| c.counts[2] == 0));
            eprintln!(
                "{label}: input_RGB_clamps={input_clips} linear_clamps=0 output_YUV_clamps={output_clips} invalid_or_nonfinite=0 CPU_total_clamps={}",
                diagnostics.clamped_components
            );
            compare_output(
                &label,
                &actual.frame,
                &expected,
                matches!(pattern, Pattern::Neutral(64)),
            );
        }
    }
    assert_eq!(gpu.validation_error_count(), 0);
}

fn neutral_nits_code(nits: f64) -> u16 {
    let pq = hdr_pq::pq_inverse_eotf(nits).unwrap();
    hdr_pq::encode_limited(hdr_pq::pq_to_ycbcr(hdr_pq::PqRgb {
        r: pq,
        g: pq,
        b: pq,
    }))
    .unwrap()
    .0
    .y
}

#[test]
fn synthetic_p010_vectors_and_cpu_constants_are_portable() {
    // Independent published ST.2084 reference points, in absolute cd/m².
    for (pq, expected) in [
        (0.0, 0.0),
        (0.5, 92.24570899406527),
        (0.75, 983.3778555870275),
        (1.0, 10_000.0),
    ] {
        let actual = hdr_pq::pq_eotf_nits(pq).unwrap();
        assert!((actual - expected).abs() <= 1e-8, "PQ={pq}: {actual}");
    }
    let input = frame(24, 2, Pattern::GraySteps, 42);
    assert!(codes(input.host().planes(input.desc()).0).contains(&501));
    let atlas = GlyphAtlas::from_r8(1, 1, 1, vec![255]).unwrap();
    let cpu = HdrPqReference::new(&atlas, "#").unwrap();
    let cells = cpu.map(&input, 12, 1).unwrap();
    assert_eq!(cells.clamped_input_components, 0);
    assert_eq!(cells.cells[0].foreground_nits, hdr_pq::LinearRgb::BLACK);
    assert!(cells.cells[7].foreground_nits.r < cells.cells[8].foreground_nits.r);
    assert_eq!(cpu.process(&input, 12, 1, true).unwrap().0, input);
}

#[test]
#[ignore = "requires real Vulkan compute execution; CPU Vulkan is synthetic qualification only"]
fn pq_builtin_mapping_and_render_match_cpu() {
    let _guard = VULKAN_TEST_LOCK.lock().unwrap();
    let config = AsciiConfig::default();
    let atlas = GlyphAtlas::builtin(&config.font, &config.charset).unwrap();
    exercise_atlas(atlas, config, "builtin");
}

#[test]
#[ignore = "requires real Vulkan compute execution and the repository FreeType fixture"]
fn pq_freetype_mapping_and_render_match_cpu() {
    let _guard = VULKAN_TEST_LOCK.lock().unwrap();
    let config = AsciiConfig {
        font: "qualification-Inconsolata".into(),
        ..AsciiConfig::default()
    };
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/fonts/Inconsolata-Regular.ttf");
    let (atlas, _) = asciiflow_font::build_font_atlas(&path, 0, &config.charset, 16, 16).unwrap();
    assert!(
        atlas
            .as_r8_slice()
            .iter()
            .any(|&coverage| coverage > 0 && coverage < 255)
    );
    exercise_atlas(atlas, config, "FreeType");
}

#[test]
#[ignore = "requires real Vulkan compute execution; checks exact neutral vectors and slot reuse"]
fn pq_exact_neutral_and_alternating_slot_reuse() {
    let _guard = VULKAN_TEST_LOCK.lock().unwrap();
    let config = AsciiConfig {
        grid_width: 8,
        grid_height: Some(6),
        charset: "#".into(),
        ..AsciiConfig::default()
    };
    let atlas = GlyphAtlas::from_r8(1, 1, 1, vec![255]).unwrap();
    let reference = HdrPqReference::new(&atlas, "#").unwrap();
    let mut gpu = VulkanPqQualification::new()
        .unwrap()
        .with_atlas(atlas.clone(), &config)
        .unwrap();
    for (index, code) in [64, 65, 500, 501, 700, 940].into_iter().enumerate() {
        let input = frame(64, 48, Pattern::Neutral(code), index as i64);
        let expected = reference.process(&input, 8, 6, true).unwrap().0;
        compare_cells(
            "exact-gray",
            &gpu.map_cells(&input, &config).unwrap(),
            &input,
            &config,
            &reference,
        );
        compare_output(
            "exact-gray",
            &gpu.process(&input, &config).unwrap().frame,
            &expected,
            true,
        );
    }
    let inputs: Vec<_> = (0..64)
        .map(|pts| {
            frame(
                64,
                48,
                Pattern::Neutral(if pts % 2 == 0 { 65 } else { 900 }),
                pts,
            )
        })
        .collect();
    let expected: Vec<_> = inputs
        .iter()
        .map(|input| reference.process(input, 8, 6, true).unwrap().0)
        .collect();
    for (input, expected) in inputs.iter().zip(&expected) {
        compare_output(
            "one-slot alternating",
            &gpu.process(input, &config).unwrap().frame,
            expected,
            true,
        );
    }
    assert_eq!(gpu.validation_error_count(), 0);
    let mut slots = gpu
        .into_pipelined(inputs[0].desc().clone(), config.clone())
        .unwrap();
    let mut outputs = Vec::new();
    for input in inputs {
        if let Some(output) = slots.submit(input, &config).unwrap() {
            outputs.push(output.frame);
        }
    }
    while let Some(output) = slots.drain().unwrap() {
        outputs.push(output.frame);
    }
    assert_eq!(outputs.len(), expected.len());
    for (actual, expected) in outputs.iter().zip(&expected) {
        compare_output("two-slot alternating", actual, expected, true);
    }
    assert_eq!(slots.validation_error_count(), 0);
}

#[test]
#[ignore = "requires real Vulkan compute execution to reject malformed samples before output"]
fn pq_invalid_samples_and_metadata_fail_before_output() {
    let _guard = VULKAN_TEST_LOCK.lock().unwrap();
    let config = AsciiConfig {
        grid_width: 7,
        grid_height: Some(3),
        ..AsciiConfig::default()
    };
    let valid = frame(66, 50, Pattern::Neutral(500), 7);
    let mut gpu = VulkanPqQualification::new().unwrap();
    let y_bytes = valid.desc().y_plane_len();
    let invalid_samples = [
        ("Y padding", 0, (500 << 6) | 1),
        ("U padding", y_bytes, (512 << 6) | 32),
        ("V padding", y_bytes + 2, (512 << 6) | 63),
        ("Y below range", 0, 63 << 6),
        ("Y above range", 0, 941 << 6),
        ("U below range", y_bytes, 63 << 6),
        ("U above range", y_bytes, 961 << 6),
        ("V below range", y_bytes + 2, 63 << 6),
        ("V above range", y_bytes + 2, 961 << 6),
    ];
    for (label, offset, sample) in invalid_samples {
        let mut storage = valid.host().clone();
        storage.as_mut_slice()[offset..offset + 2].copy_from_slice(&(sample as u16).to_le_bytes());
        let input = VideoFrame::new_host(valid.desc().clone(), valid.pts(), storage).unwrap();
        let error = gpu.process(&input, &config).err().expect(label);
        eprintln!("rejected {label}: {error}");
        // Reusing the same slot after rejection must not preserve stale invalid counters.
        assert!(
            gpu.process(&valid, &config).is_ok(),
            "recovery after {label}"
        );
    }
    let valid_color = valid.desc().color_space;
    let mut invalid_metadata = Vec::new();
    for matrix in [
        ColorMatrix::Bt709,
        ColorMatrix::Bt2020Constant,
        ColorMatrix::Unspecified,
    ] {
        invalid_metadata.push(ColorSpace {
            matrix,
            ..valid_color
        });
    }
    for primaries in [ColorPrimaries::Bt709, ColorPrimaries::Unspecified] {
        invalid_metadata.push(ColorSpace {
            primaries,
            ..valid_color
        });
    }
    for transfer in [
        TransferCharacteristic::Bt709,
        TransferCharacteristic::Hlg,
        TransferCharacteristic::Unspecified,
    ] {
        invalid_metadata.push(ColorSpace {
            transfer,
            ..valid_color
        });
    }
    for range in [ColorRange::Full, ColorRange::Unspecified] {
        invalid_metadata.push(ColorSpace {
            range,
            ..valid_color
        });
    }
    for chroma_location in [ChromaLocation::Center, ChromaLocation::Unspecified] {
        invalid_metadata.push(ColorSpace {
            chroma_location,
            ..valid_color
        });
    }
    for color_space in invalid_metadata {
        let mut desc = valid.desc().clone();
        desc.color_space = color_space;
        let input = VideoFrame::new_host(desc, valid.pts(), valid.host().clone()).unwrap();
        let error = gpu
            .process(&input, &config)
            .err()
            .expect("invalid metadata produced output");
        eprintln!("rejected metadata {color_space:?}: {error}");
    }
    let nv12_desc = FrameDesc::host_nv12(66, 50, valid_color).unwrap();
    let nv12 =
        VideoFrame::new_host(nv12_desc.clone(), None, HostFrame::new_zeroed(&nv12_desc)).unwrap();
    assert!(gpu.process(&nv12, &config).is_err());
    assert_eq!(gpu.validation_error_count(), 0);
    let mut bridge = gpu.into_qualification_backend();
    let error = bridge
        .render_cells(valid.desc(), valid.pts(), &config, &[])
        .err()
        .expect("SDR cells entered PQ renderer");
    assert!(error.to_string().contains("SDR mapped cells"));
}

#[derive(Default)]
struct BenchmarkTimings {
    api_active: Duration,
    oracle_check: Duration,
    cpu_map: Duration,
    cpu_render: Duration,
    host_upload: Duration,
    gpu_map: Duration,
    gpu_render: Duration,
    gpu_upload: Duration,
    gpu_download: Duration,
    queue: Duration,
    wait: Duration,
    backend_wall: Duration,
}

impl BenchmarkTimings {
    fn add_gpu(&mut self, timings: BackendTimings) {
        self.host_upload += timings.host_upload;
        self.gpu_map += timings.gpu_mapping;
        self.gpu_render += timings.gpu_render;
        self.gpu_upload += timings.gpu_upload;
        self.gpu_download += timings.gpu_download;
        self.queue += timings.queue_submit;
        self.wait += timings.gpu_wait;
        self.backend_wall += timings.backend_wall;
    }

    fn report(&self, label: &str, frames: usize, wall: Duration) {
        let ms = |duration: Duration| duration.as_secs_f64() * 1000.0 / frames as f64;
        eprintln!(
            "PQ benchmark {label}: frames={frames} wall_including_oracle_s={:.6} wall_including_oracle_fps={:.3} API_active_excluding_oracle_s={:.6} oracle_check_s={:.6} CPU_map_ms={:.6} CPU_render_ms={:.6} host_upload_ms={:.6} GPU_map_ms={:.6} GPU_render_ms={:.6} GPU_upload_ms={:.6} GPU_download_ms={:.6} queue_ms={:.6} wait_ms={:.6} backend_wall_ms={:.6}",
            wall.as_secs_f64(),
            frames as f64 / wall.as_secs_f64(),
            self.api_active.as_secs_f64(),
            self.oracle_check.as_secs_f64(),
            ms(self.cpu_map),
            ms(self.cpu_render),
            ms(self.host_upload),
            ms(self.gpu_map),
            ms(self.gpu_render),
            ms(self.gpu_upload),
            ms(self.gpu_download),
            ms(self.queue),
            ms(self.wait),
            ms(self.backend_wall)
        );
    }
}

fn assert_benchmark_output(actual: &VideoFrame, expected: &VideoFrame) {
    assert_eq!(actual.desc(), expected.desc());
    assert_eq!(actual.pts(), expected.pts());
    // Check every sample against the cached f64 byte oracle. No sampling or hash surrogate.
    let y_samples = actual.desc().y_plane_len() / 2;
    for (index, (actual, expected)) in actual
        .host()
        .as_slice()
        .chunks_exact(2)
        .zip(expected.host().as_slice().chunks_exact(2))
        .enumerate()
    {
        let actual = u16::from_le_bytes([actual[0], actual[1]]);
        let expected = u16::from_le_bytes([expected[0], expected[1]]);
        assert_eq!(actual & 63, 0);
        let maximum = if index < y_samples { 940 } else { 960 };
        assert!((64..=maximum).contains(&(actual >> 6)));
        assert!((actual >> 6).abs_diff(expected >> 6) <= 1);
    }
}

fn report_1080p_input_identity(input: &VideoFrame) {
    // Linux-only qualification evidence; portable synthetic tests do not invoke this tool.
    let mut process = Command::new("sha256sum")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("1080p qualification requires the Linux sha256sum utility");
    process
        .stdin
        .take()
        .unwrap()
        .write_all(input.host().as_slice())
        .unwrap();
    let output = process.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "sha256sum failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = String::from_utf8(output.stdout).unwrap();
    let hash = output.split_whitespace().next().unwrap();
    assert_eq!(hash.len(), 64);
    assert!(hash.bytes().all(|value| value.is_ascii_hexdigit()));
    eprintln!(
        "PQ 1080p input identity: generator=pq_qualification::frame/v1 pattern=ColorBoundary width={} height={} raw_bytes={} raw_P010_SHA256={hash}; Y=300+((x*7+y*11)%400); UV=(64,960)/(960,64)/(400,600)/(600,400) by chroma(x+y)%4; samples=LE16(code<<6)",
        input.desc().width,
        input.desc().height,
        input.host().as_slice().len()
    );
}

#[test]
#[ignore = "1920x1080 300-frame CPU f64 and Vulkan one/two-slot benchmark; run in release"]
fn pq_1080p_300_frame_benchmark() {
    let _guard = VULKAN_TEST_LOCK.lock().unwrap();
    const FRAMES: usize = 300;
    let config = AsciiConfig {
        grid_width: 160,
        grid_height: Some(90),
        ..AsciiConfig::default()
    };
    let atlas = GlyphAtlas::builtin(&config.font, &config.charset).unwrap();
    let reference = HdrPqReference::new(&atlas, &config.charset).unwrap();
    let input = frame(1920, 1080, Pattern::ColorBoundary, 19);
    report_1080p_input_identity(&input);
    let expected = reference.process(&input, 160, 90, true).unwrap().0;
    eprintln!(
        "PQ benchmark: fixed BT2020 NCL/PQ/limited/left P010 1920x1080 color-boundary input; grid=160x90 builtin-8x8; f64 CPU algorithm unchanged; API active intervals exclude full-output parity checks; two-slot workers may overlap oracle checks, so API active time is not two-slot throughput"
    );
    let mut cpu = BenchmarkTimings::default();
    let started = Instant::now();
    for _ in 0..FRAMES {
        let api_started = Instant::now();
        let map_started = Instant::now();
        let grid = reference.map(&input, 160, 90).unwrap();
        cpu.cpu_map += map_started.elapsed();
        let render_started = Instant::now();
        let output = reference
            .render(&grid, input.desc().clone(), input.pts(), true)
            .unwrap()
            .0;
        cpu.cpu_render += render_started.elapsed();
        cpu.api_active += api_started.elapsed();
        let check_started = Instant::now();
        assert_eq!(output, expected);
        cpu.oracle_check += check_started.elapsed();
    }
    cpu.report("CPU f64", FRAMES, started.elapsed());
    let mut gpu = VulkanPqQualification::new()
        .unwrap()
        .with_atlas(atlas.clone(), &config)
        .unwrap();
    let info = gpu.device_info();
    eprintln!(
        "PQ benchmark device: name={} vendor_id={:#x} device_id={:#x} type={:?} API={} driver_version={:#x}",
        info.name,
        info.vendor_id,
        info.device_id,
        info.device_type,
        info.api_version_string(),
        info.driver_version
    );
    compare_cells(
        "1080p-benchmark",
        &gpu.map_cells(&input, &config).unwrap(),
        &input,
        &config,
        &reference,
    );
    for _ in 0..3 {
        assert_benchmark_output(&gpu.process(&input, &config).unwrap().frame, &expected);
    }
    let mut one_slot = BenchmarkTimings::default();
    let started = Instant::now();
    for _ in 0..FRAMES {
        let api_started = Instant::now();
        let output = gpu.process(&input, &config).unwrap();
        one_slot.api_active += api_started.elapsed();
        one_slot.add_gpu(output.timings);
        let check_started = Instant::now();
        assert_benchmark_output(&output.frame, &expected);
        one_slot.oracle_check += check_started.elapsed();
    }
    one_slot.report("Vulkan one-slot", FRAMES, started.elapsed());
    assert_eq!(gpu.validation_error_count(), 0);
    eprintln!(
        "PQ benchmark one-slot validation_errors={}",
        gpu.validation_error_count()
    );
    let mut slots = gpu
        .into_pipelined(input.desc().clone(), config.clone())
        .unwrap();
    for _ in 0..3 {
        if let Some(output) = slots.submit(input.clone(), &config).unwrap() {
            assert_benchmark_output(&output.frame, &expected);
        }
    }
    while let Some(output) = slots.drain().unwrap() {
        assert_benchmark_output(&output.frame, &expected);
    }
    let mut two_slot = BenchmarkTimings::default();
    let mut completed = 0;
    let started = Instant::now();
    for _ in 0..FRAMES {
        let api_started = Instant::now();
        let output = slots.submit(input.clone(), &config).unwrap();
        two_slot.api_active += api_started.elapsed();
        if let Some(output) = output {
            two_slot.add_gpu(output.timings);
            let check_started = Instant::now();
            assert_benchmark_output(&output.frame, &expected);
            two_slot.oracle_check += check_started.elapsed();
            completed += 1;
        }
    }
    loop {
        let api_started = Instant::now();
        let output = slots.drain().unwrap();
        two_slot.api_active += api_started.elapsed();
        let Some(output) = output else {
            break;
        };
        two_slot.add_gpu(output.timings);
        let check_started = Instant::now();
        assert_benchmark_output(&output.frame, &expected);
        two_slot.oracle_check += check_started.elapsed();
        completed += 1;
    }
    assert_eq!(completed, FRAMES);
    two_slot.report("Vulkan two-slot", FRAMES, started.elapsed());
    assert_eq!(slots.validation_error_count(), 0);
    eprintln!(
        "PQ benchmark two-slot validation_errors={}",
        slots.validation_error_count()
    );
}

#[test]
#[ignore = "1920x1080 intermediate error and complete output characterization on real Vulkan"]
fn pq_1080p_mapping_and_render_characterization() {
    let _guard = VULKAN_TEST_LOCK.lock().unwrap();
    let config = AsciiConfig {
        grid_width: 160,
        grid_height: Some(90),
        ..AsciiConfig::default()
    };
    let atlas = GlyphAtlas::builtin(&config.font, &config.charset).unwrap();
    let reference = HdrPqReference::new(&atlas, &config.charset).unwrap();
    let input = frame(1920, 1080, Pattern::ColorBoundary, 19);
    report_1080p_input_identity(&input);
    let mut gpu = VulkanPqQualification::new()
        .unwrap()
        .with_atlas(atlas.clone(), &config)
        .unwrap();
    compare_cells(
        "1080p-benchmark",
        &gpu.map_cells(&input, &config).unwrap(),
        &input,
        &config,
        &reference,
    );
    let expected = reference.process(&input, 160, 90, true).unwrap().0;
    compare_output(
        "1080p-benchmark",
        &gpu.process(&input, &config).unwrap().frame,
        &expected,
        false,
    );
    assert_eq!(gpu.validation_error_count(), 0);
}

#[test]
#[ignore = "requires real Vulkan compute and Linux FD observations for injected cleanup failures"]
fn pq_injected_faults_release_resources_and_allow_fresh_retry() {
    let _guard = VULKAN_TEST_LOCK.lock().unwrap();
    let config = AsciiConfig {
        grid_width: 8,
        grid_height: Some(6),
        ..AsciiConfig::default()
    };
    let input = frame(64, 48, Pattern::ColorBoundary, 11);
    let atlas = GlyphAtlas::builtin(&config.font, &config.charset).unwrap();
    let reference = HdrPqReference::new(&atlas, &config.charset).unwrap();
    let expected = reference.process(&input, 8, 6, true).unwrap().0;
    // Exclude loader/driver initialization caches from per-backend lifecycle observations.
    {
        let mut warmup = VulkanPqQualification::new().unwrap();
        assert_benchmark_output(&warmup.process(&input, &config).unwrap().frame, &expected);
        assert_eq!(warmup.validation_error_count(), 0);
    }
    let fd_count = || std::fs::read_dir("/proc/self/fd").unwrap().count();
    let baseline = fd_count();
    for (fault, stage) in [
        (
            PqQualificationFault::CellAllocation,
            PipelineStage::ProcessorInitialization,
        ),
        (
            PqQualificationFault::DescriptorCreation,
            PipelineStage::ProcessorInitialization,
        ),
        (
            PqQualificationFault::PipelineCreation,
            PipelineStage::ProcessorInitialization,
        ),
        (
            PqQualificationFault::BeforeSubmit,
            PipelineStage::ProcessingRuntime,
        ),
        (
            PqQualificationFault::AfterCompletion,
            PipelineStage::ProcessingRuntime,
        ),
    ] {
        {
            let mut failed = VulkanPqQualification::new().unwrap();
            // Initialization failures target fresh resources, before prepare/process.
            failed.inject_fault(fault);
            let error = failed
                .process(&input, &config)
                .err()
                .expect("injected fault produced output");
            assert_eq!(error.stage(), Some(stage), "{fault:?}: {error}");
            eprintln!("PQ injected fault {fault:?}: stage={stage:?} error={error}");
            assert_eq!(failed.validation_error_count(), 0);
        }
        assert_eq!(fd_count(), baseline, "FD leak after {fault:?}");
        {
            let mut healthy = VulkanPqQualification::new().unwrap();
            assert_benchmark_output(&healthy.process(&input, &config).unwrap().frame, &expected);
            assert_eq!(healthy.validation_error_count(), 0);
        }
        assert_eq!(
            fd_count(),
            baseline,
            "FD leak after fresh retry for {fault:?}"
        );
        eprintln!(
            "PQ injected fault {fault:?}: fd_before={baseline} fd_after={} fresh_retry=passed",
            fd_count()
        );
    }
}
