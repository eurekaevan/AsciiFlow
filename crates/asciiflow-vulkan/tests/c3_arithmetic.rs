#![cfg(feature = "hdr-to-sdr-qualification")]

//! Exhaustive arithmetic experiments, separate from immutable scientific oracles.
//! CPU f32 intrinsics are diagnostic estimates, never substitutes for GPU capture.
use asciiflow_core::hdr_pq::LinearRgb;
use asciiflow_core::sdr_target_volume::{self as target, BT2020_TO_BT709};
use asciiflow_core::tone_map_bt2446::{MethodA, SdrBt2020NonlinearRgb};
use asciiflow_vulkan::{C3MatrixExperiment, C3Output, VulkanHdrToSdrQualification};
use std::io::Write;
use std::path::Path;

const HISTORY: [(&str, usize, usize, usize); 11] = [
    ("BC", 237, 725, 0),
    ("BC", 328, 787, 0),
    ("BC", 472, 942, 0),
    ("BC", 267, 952, 2),
    ("BC", 593, 1024, 0),
    ("BC", 51, 1027, 2),
    ("BC", 1064, 1032, 2),
    ("C", 565, 1005, 0),
    ("C", 1064, 1032, 2),
    ("C", 618, 1041, 0),
    ("C", 668, 1075, 0),
];
const STAGES: [&str; 5] = ["B", "C-source", "C-pre", "C-bounded", "C-final"];
type F32Stages = [[f32; 3]; 5];
type F64Stages = [[f64; 3]; 5];

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

fn scientific(signal: [f64; 3]) -> F64Stages {
    let converted = target::convert_method_a_rgb(SdrBt2020NonlinearRgb {
        r: signal[0],
        g: signal[1],
        b: signal[2],
    })
    .unwrap();
    [
        signal,
        converted.source_linear.components(),
        converted.unbounded.components(),
        converted.bounded.components(),
        converted.nonlinear.components(),
    ]
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

// Match shader operation order and shaderc's folded f32 rho constants. Every
// operation below is f32; the f64 reference is computed separately above.
fn method_f32(input: [f32; 3], cancel_inverse_ncl: bool) -> [f32; 3] {
    let nonlinear = input.map(|v| (v / 1000.0).powf(1.0 / 2.4));
    let y = (0.2627 * nonlinear[0] + 0.6780 * nonlinear[1]) + 0.0593 * nonlinear[2];
    if y == 0.0 {
        return [0.0; 3];
    }
    let rh = f32::from_bits(0x4144_2822);
    let rs = f32::from_bits(0x4096_4d7a);
    let p = (1.0 + rh * y).ln() / (1.0 + rh).ln();
    let compressed = if p <= 0.7399 {
        1.0770 * p
    } else if p < 0.9909 {
        ((-1.1510 * p) * p + 2.7811 * p) - 0.6302
    } else {
        0.5 * p + 0.5
    };
    let mapped = ((compressed * (1.0 + rs).ln()).exp() - 1.0) / rs;
    let scale = mapped / (1.1 * y);
    if cancel_inverse_ncl {
        let offset = 0.1 * y - (0.1 * (nonlinear[0] - y) / 1.4746).max(0.0);
        return nonlinear.map(|v| scale * (v + offset));
    }
    let cb = scale * (nonlinear[2] - y) / 1.8814;
    let cr = scale * (nonlinear[0] - y) / 1.4746;
    let corrected = mapped - (0.1 * cr).max(0.0);
    let r = corrected + 1.4746 * cr;
    let b = corrected + 1.8814 * cb;
    let g = ((corrected - 0.2627 * r) - 0.0593 * b) / 0.6780;
    [r, g, b]
}

fn two_sum(a: f32, b: f32) -> (f32, f32) {
    let sum = a + b;
    let virtual_b = sum - a;
    (sum, (a - (sum - virtual_b)) + (b - virtual_b))
}

fn product_residual(a: f32, b: f32, product: f32) -> f32 {
    let ca = 4097.0 * a;
    let cb = 4097.0 * b;
    let ah = ca - (ca - a);
    let bh = cb - (cb - b);
    let al = a - ah;
    let bl = b - bh;
    (((ah * bh - product) + ah * bl) + al * bh) + al * bl
}

fn compensated_neutral(source: [f32; 3], row: [f64; 3], split_product: bool) -> f32 {
    compensated_neutral_using(source, row, |a, b, p| {
        if split_product {
            product_residual(a, b, p)
        } else {
            a.mul_add(b, -p)
        }
    })
}

fn compensated_neutral_using(
    source: [f32; 3],
    row: [f64; 3],
    residual: impl Fn(f32, f32, f32) -> f32,
) -> f32 {
    let (dr, dr_error) = two_sum(source[0], -source[1]);
    let (db, db_error) = two_sum(source[2], -source[1]);
    let a = row[0] as f32;
    let b = row[2] as f32;
    let p = a * dr;
    let q = b * db;
    // Modes 4/5 intentionally retain host correctly fused estimates. GLSL
    // Fma can be unfused; mode 6 matches the explicit shader split operations.
    let p_residual = residual(a, dr, p);
    let q_residual = residual(b, db, q);
    let p_error = p_residual + a * dr_error;
    let q_error = q_residual + b * db_error;
    let (sum, first_error) = two_sum(source[1], p);
    let (sum, second_error) = two_sum(sum, q);
    let coefficient_error =
        ((row[0] - f64::from(a)) as f32) * dr + ((row[2] - f64::from(b)) as f32) * db;
    sum + (((first_error + second_error) + (p_error + q_error)) + coefficient_error)
}

#[test]
fn sealed_coefficient_splits_and_neutral_identity() {
    let expected = [
        [(0x3fd4_8af8, 0x3255_6e96), (0xbd95_324f, 0xb08f_4362)],
        [(0xbdff_1452, 0x3113_a7e2), (0xbc08_cc04, 0xaf83_5c29)],
        [(0xbc94_b0e9, 0x2f7e_4567), (0x3f8f_3289, 0xb353_6192)],
    ];
    for (row, split) in BT2020_TO_BT709.into_iter().zip(expected) {
        for (coefficient, (high_bits, low_bits)) in [row[0], row[2]].into_iter().zip(split) {
            let high = coefficient as f32;
            let low = (coefficient - f64::from(high)) as f32;
            assert_eq!(high.to_bits(), high_bits);
            assert_eq!(low.to_bits(), low_bits);
        }
    }
    // Exact neutral identity includes retained negative and above-white signals.
    for v in [-2.0_f32, -0.25, 0.0, 0.125, 1.0, 4.0] {
        for row in BT2020_TO_BT709 {
            for split_product in [false, true] {
                assert_eq!(
                    compensated_neutral([v; 3], row, split_product).to_bits(),
                    v.to_bits()
                );
            }
        }
    }
}

fn matrix_f32(source: [f32; 3], mode: u32) -> [f32; 3] {
    BT2020_TO_BT709.map(|sealed| {
        let row = sealed.map(|v| v as f32);
        let a = row[0] * source[0];
        let b = row[1] * source[1];
        let c = row[2] * source[2];
        match mode {
            0 => (source[1] + row[0] * (source[0] - source[1])) + row[2] * (source[2] - source[1]),
            2 => (a + b) + c,
            3 => a + (b + c),
            4 => row[0].mul_add(source[0], row[1].mul_add(source[1], c)),
            5 => compensated_neutral(source, sealed, false),
            6 => compensated_neutral(source, sealed, true),
            _ => unreachable!("unknown experiment mode"),
        }
    })
}

fn conversion_f32(signal: [f32; 3], mode: u32) -> F32Stages {
    let source = signal.map(|v| {
        if v == 0.0 {
            0.0
        } else {
            v.abs().powf(2.4).copysign(v)
        }
    });
    let pre = matrix_f32(source, mode);
    let bounded = pre.map(|v| v.clamp(0.0, 1.0));
    [
        signal,
        source,
        pre,
        bounded,
        bounded.map(|v| v.powf(1.0 / 2.4)),
    ]
}

fn gpu_stage(output: &C3Output, stage: usize) -> &[[f32; 3]] {
    match stage {
        0 => output.nonlinear_2020.as_ref().unwrap(),
        1 => output.display_linear_2020.as_ref().unwrap(),
        2 => output.pre_limit_709.as_ref().unwrap(),
        3 => output.bounded_709.as_ref().unwrap(),
        4 => &output.nonlinear_709,
        _ => unreachable!(),
    }
}

fn u16_code(v: f64) -> u32 {
    (v.clamp(0.0, 1.0) * 65535.0 + 0.5).floor() as u32
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

#[derive(Default)]
struct MaskMismatches {
    pixels: usize,
    channels: usize,
    exactly_zero_channels: usize,
    exactly_one_channels: usize,
    nonthreshold_channels: usize,
}

impl MaskMismatches {
    fn json(&self) -> String {
        format!(
            "{{\"pixels\":{},\"channels\":{},\"actual_pre_exactly_zero_channels\":{},\"actual_pre_exactly_one_channels\":{},\"actual_pre_nonthreshold_channels\":{}}}",
            self.pixels,
            self.channels,
            self.exactly_zero_channels,
            self.exactly_one_channels,
            self.nonthreshold_channels
        )
    }
}

fn mask_mismatches(output: &C3Output, expected: impl Fn(usize) -> [f64; 3]) -> MaskMismatches {
    let mut result = MaskMismatches::default();
    for (i, &actual) in output.clip_masks.as_ref().unwrap().iter().enumerate() {
        let difference = actual ^ clip_mask(expected(i));
        if difference == 0 {
            continue;
        }
        result.pixels += 1;
        for c in 0..3 {
            if difference & ((1 << c) | (1 << (c + 3))) == 0 {
                continue;
            }
            result.channels += 1;
            let pre = output.pre_limit_709.as_ref().unwrap()[i][c];
            if pre == 0.0 {
                result.exactly_zero_channels += 1;
            } else if pre == 1.0 {
                result.exactly_one_channels += 1;
            } else {
                result.nonthreshold_channels += 1;
            }
        }
    }
    result
}

fn matrix_f64(source: [f64; 3]) -> [f64; 3] {
    BT2020_TO_BT709
        .map(|row| source[1] + row[0] * (source[0] - source[1]) + row[2] * (source[2] - source[1]))
}

// Accessors avoid allocating promoted copies of full GPU and f32 stage arrays.
fn distribution(
    label: &str,
    count: usize,
    actual: impl Fn(usize, usize) -> f64,
    expected: impl Fn(usize, usize) -> f64,
    quantized: bool,
    width: usize,
) -> (String, u32, usize) {
    let mut channels = Vec::new();
    let mut max_code = 0;
    let mut over_two = 0;
    for c in 0..3 {
        let mut errors = Vec::with_capacity(count);
        let mut codes = Vec::with_capacity(if quantized { count } else { 0 });
        let mut worst = (0, 0.0_f64);
        for i in 0..count {
            let a = actual(i, c);
            let e = expected(i, c);
            assert!(
                a.is_finite() && e.is_finite(),
                "nonfinite metric {label} at {i}/{c}"
            );
            let error = (a - e).abs();
            if error > worst.1 {
                worst = (i, error);
            }
            errors.push(error);
            if quantized {
                let code = u16_code(a).abs_diff(u16_code(e));
                max_code = max_code.max(code);
                over_two += usize::from(code > 2);
                codes.push(code);
            }
        }
        errors.sort_unstable_by(f64::total_cmp);
        let p = |f: f64| errors[((count - 1) as f64 * f).ceil() as usize];
        let code_json = if quantized {
            codes.sort_unstable();
            let q = |f: f64| codes[((count - 1) as f64 * f).ceil() as usize];
            format!(
                "{{\"p50\":{},\"p95\":{},\"p99\":{},\"p999\":{},\"max\":{}}}",
                q(0.5),
                q(0.95),
                q(0.99),
                q(0.999),
                codes[count - 1]
            )
        } else {
            "null".into()
        };
        channels.push(format!("{{\"channel\":{c},\"p50\":{},\"p95\":{},\"p99\":{},\"p999\":{},\"max\":{},\"worst_xy\":[{},{}],\"actual\":{},\"expected\":{},\"unorm16\":{code_json}}}", p(0.5), p(0.95), p(0.99), p(0.999), worst.1, worst.0 % width, worst.0 / width, actual(worst.0,c), expected(worst.0,c)));
    }
    (
        format!(
            "{{\"label\":\"{label}\",\"channels\":[{}],\"unorm16_max\":{max_code},\"unorm16_over_two\":{over_two}}}",
            channels.join(",")
        ),
        max_code,
        over_two,
    )
}

fn trace(
    output: &C3Output,
    cpu: &[F32Stages],
    reference: &[F64Stages],
    i: usize,
    c: usize,
    mode: u32,
    width: usize,
) -> String {
    let b = output.nonlinear_2020.as_ref().unwrap()[i];
    let source = output.display_linear_2020.as_ref().unwrap()[i];
    let conditioned = conversion_f32(b, mode);
    let from_gpu_source = matrix_f32(source, mode);
    let unfused =
        BT2020_TO_BT709.map(|row| compensated_neutral_using(source, row, |a, b, p| (a * b) - p));
    let scientific_gpu_b = scientific(b.map(f64::from));
    let gpu_pre = output.pre_limit_709.as_ref().unwrap()[i];
    let first_divergence = if b.map(f32::to_bits) != cpu[i][0].map(f32::to_bits) {
        "B host intrinsic/operation estimate"
    } else if source.map(f32::to_bits) != conditioned[1].map(f32::to_bits) {
        "C source Pow"
    } else if gpu_pre.map(f32::to_bits) != from_gpu_source.map(f32::to_bits) {
        "C matrix arithmetic"
    } else if output.nonlinear_709[i].map(f32::to_bits)
        != output.bounded_709.as_ref().unwrap()[i]
            .map(|v| v.powf(1.0 / 2.4))
            .map(f32::to_bits)
    {
        "C inverse Pow"
    } else {
        "none against host f32 estimate"
    };
    let stages: Vec<_> = (0..5).map(|s| format!("{{\"stage\":\"{}\",\"gpu\":{:?},\"gpu_bits\":{:?},\"cpu_f32\":{:?},\"cpu_f32_bits\":{:?},\"sealed_f64\":{:?}}}", STAGES[s], gpu_stage(output,s)[i], gpu_stage(output,s)[i].map(f32::to_bits), cpu[i][s], cpu[i][s].map(f32::to_bits), reference[i][s])).collect();
    format!(
        "{{\"xy\":[{},{}],\"channel\":{c},\"code_error\":{},\"gpu_clip_mask\":{},\"original_f64_clip_mask\":{},\"shader_contract_f32_clip_mask\":{},\"first_gpu_host_f32_divergence\":\"{first_divergence}\",\"stages\":[{}],\"gpu_matrix_terms\":{:?},\"gpu_matrix_terms_bits\":{:?},\"cpu_C_from_gpu_B\":{:?},\"cpu_matrix_from_gpu_source\":{:?},\"cpu_unfused_residual_matrix_from_gpu_source\":{:?},\"cpu_unfused_residual_matrix_bits\":{:?},\"unfused_residual_reproduces_gpu_channel_bits\":{},\"sealed_matrix_from_gpu_source\":{:?},\"sealed_C_from_gpu_B\":{:?},\"gpu_inverse_power_error\":{}}}",
        i % width,
        i / width,
        u16_code(f64::from(output.nonlinear_709[i][c])).abs_diff(u16_code(reference[i][4][c])),
        output.clip_masks.as_ref().unwrap()[i],
        clip_mask(reference[i][2]),
        clip_mask(cpu[i][2].map(f64::from)),
        stages.join(","),
        output.matrix_terms.as_ref().unwrap()[i],
        output.matrix_terms.as_ref().unwrap()[i].map(|row| row.map(f32::to_bits)),
        conditioned,
        from_gpu_source,
        unfused,
        unfused.map(f32::to_bits),
        unfused[c].to_bits() == gpu_pre[c].to_bits(),
        matrix_f64(source.map(f64::from)),
        scientific_gpu_b,
        (f64::from(output.nonlinear_709[i][c])
            - f64::from(output.bounded_709.as_ref().unwrap()[i][c]).powf(1.0 / 2.4))
        .abs()
    )
}

#[test]
#[ignore = "full canonical GPU arithmetic sweep; C1_INPUT/C1_OUTPUT/C3_ARITHMETIC_REPORT required"]
fn canonical_arithmetic_sweep() {
    let input_path = std::env::var("C1_INPUT").expect("C1_INPUT");
    let c1_path = std::env::var("C1_OUTPUT").expect("C1_OUTPUT");
    let report_path = std::env::var("C3_ARITHMETIC_REPORT").expect("C3_ARITHMETIC_REPORT");
    let (width, height, original) = read_fixture(Path::new(&input_path), b"AF-C1-IN-v1\0", 3);
    let (cw, ch, c1) = read_fixture(Path::new(&c1_path), b"AF-C1-OUT-v1\0", 7);
    assert_eq!((width, height), (1920, 1080));
    assert_eq!((cw, ch), (width, height));
    let count = original.len();
    let reference: Vec<_> = c1.iter().copied().map(scientific).collect();
    let input: Vec<_> = original.iter().map(|p| p.map(|v| v as f32)).collect();
    let isolated_input: Vec<_> = c1.iter().map(|p| p.map(|v| v as f32)).collect();
    let method = MethodA::new();
    let promoted_bc: Vec<_> = input
        .iter()
        .map(|p| scientific(method_f64(&method, p.map(f64::from))))
        .collect();
    let promoted_c: Vec<_> = isolated_input
        .iter()
        .map(|p| scientific(p.map(f64::from)))
        .collect();
    let mut gpu = VulkanHdrToSdrQualification::new().unwrap();
    let mut cases = Vec::new();
    let mut passing_bc = 0;
    let mut passing_c = 0;
    for includes_method_a in [true, false] {
        for cancel_inverse_ncl in [false, true] {
            if !includes_method_a && cancel_inverse_ncl {
                continue;
            }
            let pixels = if includes_method_a {
                &input
            } else {
                &isolated_input
            };
            let promoted = if includes_method_a {
                &promoted_bc
            } else {
                &promoted_c
            };
            let signals: Vec<_> = pixels
                .iter()
                .map(|p| {
                    if includes_method_a {
                        method_f32(*p, cancel_inverse_ncl)
                    } else {
                        *p
                    }
                })
                .collect();
            for (mode, matrix, name) in [
                (0, C3MatrixExperiment::NeutralPrecise, "neutral"),
                (2, C3MatrixExperiment::RowLeft, "row-left"),
                (3, C3MatrixExperiment::RowRight, "row-right"),
                (4, C3MatrixExperiment::RowFma, "row-fma"),
                (
                    5,
                    C3MatrixExperiment::NeutralCompensated,
                    "neutral-compensated",
                ),
                (
                    6,
                    C3MatrixExperiment::NeutralTwoProduct,
                    "neutral-two-product",
                ),
            ] {
                let path = if includes_method_a { "BC" } else { "C" };
                eprintln!("C3 arithmetic {path}/{name}/common-scale={cancel_inverse_ncl}");
                let output = gpu
                    .process_arithmetic_experiment(
                        width,
                        height,
                        pixels,
                        includes_method_a,
                        matrix,
                        cancel_inverse_ncl,
                    )
                    .unwrap();
                let cpu: Vec<_> = signals.iter().map(|p| conversion_f32(*p, mode)).collect();
                let mut metrics = Vec::new();
                let mut original_gate = (0, 0);
                for stage in 0..5 {
                    let captured = gpu_stage(&output, stage);
                    for comparison in 0..3 {
                        let comparison_name =
                            ["GPU-vs-f32", "f32-vs-f64", "GPU-vs-f64"][comparison];
                        let label = format!("{}-{comparison_name}", STAGES[stage]);
                        let actual = |i: usize, c: usize| {
                            if comparison == 1 {
                                f64::from(cpu[i][stage][c])
                            } else {
                                f64::from(captured[i][c])
                            }
                        };
                        let expected = |i: usize, c: usize| {
                            if comparison == 0 {
                                f64::from(cpu[i][stage][c])
                            } else {
                                reference[i][stage][c]
                            }
                        };
                        let result = distribution(
                            &label,
                            count,
                            actual,
                            expected,
                            stage == 4,
                            width as usize,
                        );
                        if stage == 4 && comparison == 2 {
                            original_gate = (result.1, result.2);
                        }
                        metrics.push(result.0);
                    }
                }
                let promoted_metric = distribution(
                    "C-final-GPU-vs-promoted-f32-input-f64",
                    count,
                    |i, c| f64::from(output.nonlinear_709[i][c]),
                    |i, c| promoted[i][4][c],
                    true,
                    width as usize,
                );
                let promoted_gate = (promoted_metric.1, promoted_metric.2);
                metrics.push(promoted_metric.0);
                let captured_b = output.nonlinear_2020.as_ref().unwrap();
                let captured_source = output.display_linear_2020.as_ref().unwrap();
                metrics.push(
                    distribution(
                        "C-source-GPU-vs-f32-conditioned-on-GPU-B",
                        count,
                        |i, c| f64::from(captured_source[i][c]),
                        |i, c| f64::from(conversion_f32(captured_b[i], mode)[1][c]),
                        false,
                        width as usize,
                    )
                    .0,
                );
                metrics.push(
                    distribution(
                        "C-pre-GPU-vs-f32-conditioned-on-GPU-source",
                        count,
                        |i, c| f64::from(output.pre_limit_709.as_ref().unwrap()[i][c]),
                        |i, c| f64::from(matrix_f32(captured_source[i], mode)[c]),
                        false,
                        width as usize,
                    )
                    .0,
                );
                let promoted_masks = mask_mismatches(&output, |i| promoted[i][2]);
                let original_masks = mask_mismatches(&output, |i| reference[i][2]);
                let f32_masks = mask_mismatches(&output, |i| cpu[i][2].map(f64::from));
                let mismatches = promoted_masks.pixels;
                let original_mask_mismatches = original_masks.pixels;
                let shader_contract_f32_mask_mismatches = f32_masks.pixels;
                let historical: Vec<_> = HISTORY
                    .into_iter()
                    .map(|(historical_path, x, y, c)| {
                        format!(
                            "{{\"historical_path\":\"{historical_path}\",\"trace\":{}}}",
                            trace(
                                &output,
                                &cpu,
                                &reference,
                                y * width as usize + x,
                                c,
                                mode,
                                width as usize
                            )
                        )
                    })
                    .collect();
                let first_failures: Vec<_> = output
                    .nonlinear_709
                    .iter()
                    .zip(&reference)
                    .enumerate()
                    .flat_map(|(i, (actual, expected))| {
                        (0..3).filter_map(move |c| {
                            (u16_code(f64::from(actual[c])).abs_diff(u16_code(expected[4][c])) > 2)
                                .then_some((i, c))
                        })
                    })
                    .take(16)
                    .map(|(i, c)| trace(&output, &cpu, &reference, i, c, mode, width as usize))
                    .collect();
                let validation_errors = gpu.validation_error_count();
                let passes = original_gate.0 <= 2
                    && promoted_gate.0 <= 2
                    && mismatches == 0
                    && original_mask_mismatches == 0
                    && validation_errors == 0;
                if passes {
                    if includes_method_a {
                        passing_bc += 1;
                    } else {
                        passing_c += 1;
                    }
                }
                eprintln!(
                    "{path}/{name}: original max{} over{}, promoted max{} over{}, masks{mismatches}",
                    original_gate.0, original_gate.1, promoted_gate.0, promoted_gate.1
                );
                cases.push(format!("{{\"path\":\"{path}\",\"matrix\":\"{name}\",\"mode\":{},\"cancel_inverse_ncl\":{cancel_inverse_ncl},\"strict_gate_pass\":{passes},\"original_mask_mismatches\":{original_mask_mismatches},\"shader_contract_f32_mask_mismatches\":{shader_contract_f32_mask_mismatches},\"promoted_mask_mismatches\":{mismatches},\"mask_mismatch_classes\":{{\"original_f64\":{},\"promoted_input_f64\":{},\"shader_contract_f32_host_estimate\":{}}},\"validation_errors\":{validation_errors},\"diagnostics\":{:?},\"metrics\":[{}],\"historical_traces\":[{}],\"first_over_two\":[{}]}}",mode+if cancel_inverse_ncl {256}else{0},original_masks.json(),promoted_masks.json(),f32_masks.json(),output.diagnostics,metrics.join(","),historical.join(","),first_failures.join(",")));
            }
        }
    }
    let report = format!(
        "{{\"schema_version\":1,\"scope\":\"arithmetic experiments; no winner selected and no production qualification\",\"width\":{width},\"height\":{height},\"f32_oracle\":\"true per-operation f32 with host pow/log/exp; modes 4/5 use correctly fused host mul_add estimates while GLSL Fma may be unfused; mode 6 uses explicit split products; GPU intrinsic precision remains separately observed\",\"passing_BC_cases\":{passing_bc},\"passing_C_cases\":{passing_c},\"cases\":[{}]}}\n",
        cases.join(",")
    );
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(report_path)
        .unwrap()
        .write_all(report.as_bytes())
        .unwrap();
    assert!(
        passing_bc > 0 && passing_c > 0,
        "no strict-gate arithmetic closure candidate; inspect complete exclusive report"
    );
}
