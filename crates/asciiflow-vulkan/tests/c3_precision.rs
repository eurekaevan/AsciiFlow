#![cfg(feature = "hdr-to-sdr-fp64-experiment")]

//! Opt-in real-device precision captures against the immutable CPU oracles.
//! Numerical gates are reported, never asserted, so every variant is captured.
use asciiflow_core::hdr_pq::LinearRgb;
use asciiflow_core::sdr_target_volume as target;
use asciiflow_core::tone_map_bt2446::{MethodA, SdrBt2020NonlinearRgb};
use asciiflow_vulkan::{C3MatrixExperiment, C3Output, VulkanHdrToSdrQualification};
use std::io::{BufWriter, Write};
use std::path::Path;

struct Reference {
    source: [f64; 3],
    pre: [f64; 3],
    final_rgb: [f64; 3],
}

fn scientific(signal: [f64; 3]) -> Reference {
    let converted = target::convert_method_a_rgb(SdrBt2020NonlinearRgb {
        r: signal[0],
        g: signal[1],
        b: signal[2],
    })
    .unwrap();
    Reference {
        source: converted.source_linear.components(),
        pre: converted.unbounded.components(),
        final_rgb: converted.nonlinear.components(),
    }
}

fn method_f64(method: &MethodA, input: [f64; 3]) -> [f64; 3] {
    let out = method
        .map(LinearRgb {
            r: input[0],
            g: input[1],
            b: input[2],
        })
        .unwrap()
        .rgb;
    [out.r, out.g, out.b]
}

fn read_fixture(path: &Path, magic: &[u8], stride: usize) -> (u32, u32, Vec<[f64; 3]>) {
    let bytes = std::fs::read(path).unwrap();
    assert!(bytes.starts_with(magic), "fixture magic mismatch");
    let offset = magic.len();
    let width = u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
    let height = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap());
    assert_eq!(
        bytes.len(),
        offset + 8 + width as usize * height as usize * stride * 8
    );
    let pixels = bytes[offset + 8..]
        .chunks_exact(stride * 8)
        .map(|pixel| {
            std::array::from_fn(|c| f64::from_le_bytes(pixel[c * 8..c * 8 + 8].try_into().unwrap()))
        })
        .collect();
    (width, height, pixels)
}

fn new_file(path: &Path) -> BufWriter<std::fs::File> {
    BufWriter::new(
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .unwrap(),
    )
}

fn quoted(value: &str) -> String {
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

fn clip_mask(rgb: [f64; 3]) -> u32 {
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

fn rgb_codes(rgb: [f64; 3], bits: u32) -> [u32; 3] {
    let scale = ((1_u32 << bits) - 1) as f64;
    rgb.map(|v| (v.clamp(0.0, 1.0) * scale + 0.5).floor() as u32)
}

fn limited_codes(rgb: [f64; 3], bits: u32) -> [u32; 3] {
    let [r, g, b] = rgb;
    let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    let cb = (b - y) / 1.8556;
    let cr = (r - y) / 1.5748;
    let scale = if bits == 8 { 1.0 } else { 4.0 };
    [16.0 + 219.0 * y, 128.0 + 224.0 * cb, 128.0 + 224.0 * cr]
        .map(|v| (v * scale + 0.5).floor() as u32)
}

struct Histogram {
    bins: Vec<usize>,
    count: usize,
}
impl Histogram {
    fn new() -> Self {
        Self {
            bins: Vec::new(),
            count: 0,
        }
    }
    fn add(&mut self, delta: u32) {
        let index = delta as usize;
        if self.bins.len() <= index {
            self.bins.resize(index + 1, 0);
        }
        self.bins[index] += 1;
        self.count += 1;
    }
    fn percentile(&self, fraction: f64) -> usize {
        let rank = (((self.count - 1) as f64 * fraction).ceil() as usize) + 1;
        let mut sum = 0;
        for (delta, count) in self.bins.iter().enumerate() {
            sum += count;
            if sum >= rank {
                return delta;
            }
        }
        unreachable!()
    }
    fn json(&self) -> String {
        let exact = self.bins.first().copied().unwrap_or(0);
        let delta1 = self.bins.get(1).copied().unwrap_or(0);
        format!(
            "{{\"histogram\":{:?},\"exact\":{exact},\"delta1\":{delta1},\"over1\":{},\"max\":{},\"p50\":{},\"p95\":{},\"p99\":{},\"p999\":{}}}",
            self.bins,
            self.count - exact - delta1,
            self.bins.len() - 1,
            self.percentile(0.5),
            self.percentile(0.95),
            self.percentile(0.99),
            self.percentile(0.999)
        )
    }
}

fn capture_case(
    directory: &Path,
    path: &str,
    variant: u32,
    output: &C3Output,
    reference: &[Reference],
    gpu: &VulkanHdrToSdrQualification,
) -> String {
    let count = output.width as usize * output.height as usize;
    assert!(count > 0);
    assert_eq!(output.nonlinear_709.len(), count);
    assert_eq!(reference.len(), count);
    let source = output.display_linear_2020.as_ref().unwrap();
    let pre = output.pre_limit_709.as_ref().unwrap();
    let bounded = output.bounded_709.as_ref().unwrap();
    let masks = output.clip_masks.as_ref().unwrap();
    for len in [source.len(), pre.len(), bounded.len(), masks.len()] {
        assert_eq!(len, count);
    }
    if let Some(nonlinear_2020) = &output.nonlinear_2020 {
        assert_eq!(nonlinear_2020.len(), count);
        assert!(nonlinear_2020.iter().flatten().all(|v| v.is_finite()));
    }
    let file_name = format!("{path}-M{variant}-actual-f32le.rgb");
    let mut raw = new_file(&directory.join(&file_name));
    let mut max_abs = [0.0_f64; 3];
    let mut source_max_abs = [0.0_f64; 3];
    let mut unorm16_max = 0;
    let mut over_two = Vec::new();
    let mut mismatched_pixels = 0;
    let mut mismatched_channels = 0;
    let mut histograms: [[Histogram; 3]; 6] =
        std::array::from_fn(|_| std::array::from_fn(|_| Histogram::new()));
    for (i, (&actual, expected)) in output.nonlinear_709.iter().zip(reference).enumerate() {
        let promoted = actual.map(f64::from);
        let actual16 = rgb_codes(promoted, 16);
        let expected16 = rgb_codes(expected.final_rgb, 16);
        let mask_difference = masks[i] ^ clip_mask(expected.pre);
        if mask_difference != 0 {
            mismatched_pixels += 1;
        }
        for c in 0..3 {
            assert!(actual[c].is_finite() && (0.0..=1.0).contains(&actual[c]));
            assert!(source[i][c].is_finite() && pre[i][c].is_finite());
            assert!(bounded[i][c].is_finite() && (0.0..=1.0).contains(&bounded[i][c]));
            assert!(expected.final_rgb[c].is_finite() && expected.source[c].is_finite());
            assert!((0.0..=1.0).contains(&expected.final_rgb[c]));
            assert!(expected.pre[c].is_finite());
            raw.write_all(&actual[c].to_le_bytes()).unwrap();
            max_abs[c] = max_abs[c].max((promoted[c] - expected.final_rgb[c]).abs());
            source_max_abs[c] =
                source_max_abs[c].max((f64::from(source[i][c]) - expected.source[c]).abs());
            if mask_difference & ((1 << c) | (1 << (c + 3))) != 0 {
                mismatched_channels += 1;
            }
            let delta = actual16[c].abs_diff(expected16[c]);
            unorm16_max = unorm16_max.max(delta);
            if delta > 2 {
                over_two.push(format!("{{\"xy\":[{},{}],\"channel\":{c},\"delta\":{delta},\"cpu_rgb\":{:?},\"gpu_rgb\":{:?},\"cpu_codes\":{:?},\"gpu_codes\":{:?},\"cpu_source_rgb\":{:?},\"gpu_source_rgb\":{:?}}}",
                    i % output.width as usize, i / output.width as usize, expected.final_rgb, actual,
                    expected16, actual16, expected.source, source[i]));
            }
        }
        for (format_index, bits) in [8, 10, 12, 16, 8, 10].into_iter().enumerate() {
            let codes = if format_index < 4 {
                rgb_codes
            } else {
                limited_codes
            };
            let a = codes(promoted, bits);
            let e = codes(expected.final_rgb, bits);
            for c in 0..3 {
                histograms[format_index][c].add(a[c].abs_diff(e[c]));
            }
        }
    }
    raw.flush().unwrap();
    let validation_errors = gpu.validation_error_count();
    assert_eq!(validation_errors, 0);
    assert!(output.diagnostics[..5].iter().all(|v| *v == 0));
    let info = gpu.device_info();
    let formats: Vec<_> = [
        "RGB8",
        "RGB10",
        "RGB12",
        "RGB16",
        "limited444_BT709_8",
        "limited444_BT709_10",
    ]
    .into_iter()
    .zip(histograms)
    .map(|(name, channels)| {
        format!(
            "{}:[{}]",
            quoted(name),
            channels
                .iter()
                .map(Histogram::json)
                .collect::<Vec<_>>()
                .join(",")
        )
    })
    .collect();
    format!(
        "{{\"path\":{},\"variant\":{variant},\"width\":{},\"height\":{},\"actual_file\":{},\"max_abs_nonlinear_by_channel\":{:?},\"max_abs_nonlinear\":{},\"max_abs_source_power_by_channel\":{:?},\"unorm16_max\":{unorm16_max},\"unorm16_count_over_two\":{},\"mask_mismatch_pixels\":{mismatched_pixels},\"mask_mismatch_channels\":{mismatched_channels},\"GPU_target_limit_ms\":{},\"backend_wall_ms\":{},\"validation_errors\":{validation_errors},\"diagnostics\":{:?},\"device\":{{\"name\":{},\"vendor_id\":{},\"device_id\":{},\"api_version\":{},\"driver_version\":{},\"type\":{}}},\"quantization\":{{{}}},\"all_over_two\":[{}]}}",
        quoted(path),
        output.width,
        output.height,
        quoted(&file_name),
        max_abs,
        max_abs.into_iter().fold(0.0_f64, f64::max),
        source_max_abs,
        over_two.len(),
        output.timings.target_limit.as_secs_f64() * 1000.0,
        output.timings.backend_wall.as_secs_f64() * 1000.0,
        output.diagnostics,
        quoted(&info.name),
        info.vendor_id,
        info.device_id,
        quoted(&info.api_version_string()),
        info.driver_version,
        quoted(&format!("{:?}", info.device_type)),
        formats.join(","),
        over_two.join(",")
    )
}

fn write_expected(directory: &Path, path: &str, reference: &[Reference]) {
    let mut file = new_file(&directory.join(format!("{path}-expected-f64le.rgb")));
    for rgb in reference {
        for v in rgb.final_rgb {
            file.write_all(&v.to_le_bytes()).unwrap();
        }
    }
    file.flush().unwrap();
}

#[test]
#[ignore = "real GPU precision capture; C1_INPUT/C1_OUTPUT/C3_PRECISION_DIR required"]
fn canonical_precision_sweep() {
    let input_path = std::env::var("C1_INPUT").expect("C1_INPUT");
    let c1_path = std::env::var("C1_OUTPUT").expect("C1_OUTPUT");
    let directory = std::env::var("C3_PRECISION_DIR").expect("C3_PRECISION_DIR");
    let directory = Path::new(&directory);
    std::fs::create_dir_all(directory).unwrap();
    let mut report = new_file(&directory.join("precision.json"));
    let (width, height, original) = read_fixture(Path::new(&input_path), b"AF-C1-IN-v1\0", 3);
    let (cw, ch, c1) = read_fixture(Path::new(&c1_path), b"AF-C1-OUT-v1\0", 7);
    assert_eq!((width, height), (1920, 1080));
    assert_eq!((cw, ch), (width, height));
    let method = MethodA::new();
    let mut normal = VulkanHdrToSdrQualification::new().unwrap();
    let mut fp64 = VulkanHdrToSdrQualification::new_fp64_experiment().unwrap();
    let mut cases = Vec::new();
    for (path, includes_method_a, values) in [("BC", true, &original), ("C", false, &c1)] {
        let reference: Vec<_> = values
            .iter()
            .map(|&p| {
                scientific(if includes_method_a {
                    method_f64(&method, p)
                } else {
                    p
                })
            })
            .collect();
        write_expected(directory, path, &reference);
        let input: Vec<_> = values.iter().map(|p| p.map(|v| v as f32)).collect();
        for variant in 0..=3 {
            eprintln!("C3 precision {path}/M{variant}");
            let (output, gpu) = if variant == 0 {
                let output = normal
                    .process_arithmetic_experiment(
                        width,
                        height,
                        &input,
                        includes_method_a,
                        C3MatrixExperiment::NeutralTwoProduct,
                        includes_method_a,
                    )
                    .unwrap();
                (output, &normal)
            } else {
                let output = fp64
                    .process_precision_experiment(width, height, &input, includes_method_a, variant)
                    .unwrap();
                (output, &fp64)
            };
            assert_eq!((output.width, output.height), (width, height));
            cases.push(capture_case(
                directory, path, variant, &output, &reference, gpu,
            ));
        }
    }
    write!(report, "{{\"schema\":\"AF-C3-precision-v1\",\"oracle\":\"sealed CPU f64, original input for BC and C1 output for C\",\"raw_layout\":\"row-major RGB triples, little-endian, no header\",\"cases\":[{}]}}", cases.join(",")).unwrap();
    report.flush().unwrap();
}

#[test]
#[ignore = "real GPU M3 source/target power oracle probe; C3_PRECISION_DIR required"]
fn independent_power_edges() {
    let directory = std::env::var("C3_PRECISION_DIR").expect("C3_PRECISION_DIR");
    let directory = Path::new(&directory);
    std::fs::create_dir_all(directory).unwrap();
    let mut report = new_file(&directory.join("power-edges.json"));
    // Neutral values isolate both power functions from gamut mixing, including
    // sign retention, subnormal/near-zero source input and either target edge.
    let mut input: Vec<[f32; 3]> = [
        -1.0_f32,
        -0.01,
        -f32::MIN_POSITIVE,
        -f32::from_bits(1),
        0.0,
        f32::from_bits(1),
        f32::MIN_POSITIVE,
        1e-12,
        1e-6,
        0.01,
        0.125,
        0.5,
        f32::from_bits(1.0_f32.to_bits() - 1),
        1.0,
        f32::from_bits(1.0_f32.to_bits() + 1),
        1.1,
    ]
    .into_iter()
    .map(|v| [v; 3])
    .collect();
    input.extend([
        [0.0, 0.5, 1.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
    ]);
    let width = input.len() as u32;
    input.extend_from_within(..); // P010 qualification geometry must be even.
    let reference: Vec<_> = input.iter().map(|p| scientific(p.map(f64::from))).collect();
    write_expected(directory, "power-edges", &reference);
    let mut gpu = VulkanHdrToSdrQualification::new_fp64_experiment().unwrap();
    let output = gpu
        .process_precision_experiment(width, 2, &input, false, 3)
        .unwrap();
    assert_eq!((output.width, output.height), (width, 2));
    let case = capture_case(directory, "power-edges", 3, &output, &reference, &gpu);
    let source = output.display_linear_2020.as_ref().unwrap();
    let vectors: Vec<_> = input.iter().enumerate().map(|(i, p)| format!(
        "{{\"input_rgb\":{:?},\"cpu_source_rgb\":{:?},\"gpu_source_rgb\":{:?},\"cpu_final_rgb\":{:?},\"gpu_final_rgb\":{:?}}}",
        p, reference[i].source, source[i], reference[i].final_rgb, output.nonlinear_709[i])).collect();
    write!(
        report,
        "{{\"case\":{case},\"vectors\":[{}]}}",
        vectors.join(",")
    )
    .unwrap();
    report.flush().unwrap();
}

#[test]
fn quantization_rounding_and_histogram_contract() {
    assert_eq!(rgb_codes([0.0, 0.5, 1.0], 8), [0, 128, 255]);
    assert_eq!(limited_codes([0.0; 3], 8), [16, 128, 128]);
    assert_eq!(limited_codes([1.0; 3], 10), [940, 512, 512]);
    let mut histogram = Histogram::new();
    for delta in [0, 0, 1, 3] {
        histogram.add(delta);
    }
    assert_eq!(histogram.bins, [2, 1, 0, 1]);
    assert_eq!(histogram.percentile(0.5), 1);
    assert_eq!(histogram.percentile(0.999), 3);
}
