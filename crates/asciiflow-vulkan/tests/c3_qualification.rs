#![cfg(feature = "hdr-to-sdr-qualification")]

//! Real GPU tests, never substitutes for production or decoder qualification.
use asciiflow_core::hdr_pq::LinearRgb;
use asciiflow_core::sdr_target_volume::{self as target, SdrConversionOutput};
use asciiflow_core::tone_map_bt2446::{MethodA, SdrBt2020NonlinearRgb};
use asciiflow_vulkan::{C3Fault, VulkanHdrToSdrQualification};
use std::io::Write;
use std::path::Path;

fn method(rgb: [f64; 3]) -> [f64; 3] {
    let mapped = MethodA::new()
        .map(LinearRgb {
            r: rgb[0],
            g: rgb[1],
            b: rgb[2],
        })
        .unwrap()
        .rgb;
    [mapped.r, mapped.g, mapped.b]
}
fn convert(rgb: [f64; 3]) -> SdrConversionOutput {
    target::convert_method_a_rgb(SdrBt2020NonlinearRgb {
        r: rgb[0],
        g: rgb[1],
        b: rgb[2],
    })
    .unwrap()
}

fn limited_yuv10([r, g, b]: [f64; 3]) -> [i32; 3] {
    let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    [
        (64.0 + 876.0 * y + 0.5).floor() as i32,
        (512.0 + 896.0 * (b - y) / 1.8556 + 0.5).floor() as i32,
        (512.0 + 896.0 * (r - y) / 1.5748 + 0.5).floor() as i32,
    ]
}

fn n3_pass(actual: &[[f32; 3]], expected: &[[f64; 3]]) -> bool {
    actual.len() == expected.len()
        && actual.iter().zip(expected).all(|(actual, expected)| {
            let actual = actual.map(f64::from);
            actual.iter().zip(expected).all(|(a, e)| {
                a.is_finite() && (0.0..=1.0).contains(a) && (a - e).abs() <= 1.0 / 1792.0
            }) && limited_yuv10(actual)
                .iter()
                .zip(limited_yuv10(*expected))
                .all(|(a, e)| a.abs_diff(e) <= 1)
        })
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
fn unorm16(v: f64) -> u32 {
    (v.clamp(0.0, 1.0) * 65535.0 + 0.5).floor() as u32
}

// Per-channel distribution plus worst coordinate, with no preselected generous
// tolerance. The independent final <=2-code gate is enforced after all reports.
fn metrics(
    label: &str,
    actual: &[[f32; 3]],
    expected: &[[f64; 3]],
    width: u32,
    quantized: bool,
) -> (String, u32, u64) {
    assert_eq!(actual.len(), expected.len());
    let mut channels = Vec::new();
    let mut max_code = 0;
    let mut over_two = 0;
    for c in 0..3 {
        let mut errors = Vec::with_capacity(actual.len());
        let mut codes = Vec::new();
        let mut worst = (0usize, 0.0f64);
        for (i, (gpu, cpu)) in actual.iter().zip(expected).enumerate() {
            assert!(gpu[c].is_finite(), "{label}: nonfinite at {i}/{c}");
            let error = (f64::from(gpu[c]) - cpu[c]).abs();
            if error > worst.1 {
                worst = (i, error);
            }
            errors.push(error);
            if quantized {
                let difference = unorm16(f64::from(gpu[c])).abs_diff(unorm16(cpu[c]));
                max_code = max_code.max(difference);
                over_two += u64::from(difference > 2);
                codes.push(difference);
            }
        }
        errors.sort_unstable_by(f64::total_cmp);
        let p = |fraction: f64| errors[((errors.len() - 1) as f64 * fraction).ceil() as usize];
        let code_metrics = if quantized {
            codes.sort_unstable();
            let q = |fraction: f64| codes[((codes.len() - 1) as f64 * fraction).ceil() as usize];
            format!(
                "{{\"p50\":{},\"p95\":{},\"p99\":{},\"p999\":{},\"max\":{}}}",
                q(0.5),
                q(0.95),
                q(0.99),
                q(0.999),
                codes[codes.len() - 1]
            )
        } else {
            "null".into()
        };
        channels.push(format!("{{\"channel\":{c},\"p50\":{},\"p95\":{},\"p99\":{},\"p999\":{},\"max\":{},\"worst_xy\":[{},{}],\"gpu\":{},\"cpu\":{},\"unorm16\":{code_metrics}}}",p(0.5),p(0.95),p(0.99),p(0.999),worst.1,worst.0%width as usize,worst.0/width as usize,actual[worst.0][c],expected[worst.0][c]));
    }
    eprintln!(
        "{label}: max UNORM16={max_code}, >2 samples={over_two}; {}",
        channels.join(",")
    );
    (
        format!(
            "{{\"label\":\"{label}\",\"channels\":[{}],\"unorm16_evaluated\":{quantized},\"unorm16_max\":{max_code},\"unorm16_over_two\":{over_two}}}",
            channels.join(",")
        ),
        max_code,
        over_two,
    )
}

fn read_rgb(path: &Path, magic: &[u8], stride: usize) -> (u32, u32, Vec<[f64; 3]>) {
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

#[test]
#[ignore = "real Vulkan dispatch; explicit C1_INPUT/C1_OUTPUT and C3_REPORT paths required"]
fn canonical_f64_and_f32_input_oracles() {
    let input_path = std::env::var("C1_INPUT").expect("C1_INPUT");
    let c1_path = std::env::var("C1_OUTPUT").expect("C1_OUTPUT");
    let report_path = std::env::var("C3_REPORT").expect("C3_REPORT");
    let (width, height, original) = read_rgb(Path::new(&input_path), b"AF-C1-IN-v1\0", 3);
    let (cw, ch, c1) = read_rgb(Path::new(&c1_path), b"AF-C1-OUT-v1\0", 7);
    assert_eq!((width, height), (1920, 1080));
    assert_eq!((width, height), (cw, ch));
    let input: Vec<_> = original.iter().map(|p| p.map(|v| v as f32)).collect();
    let promoted: Vec<_> = input.iter().map(|p| method(p.map(f64::from))).collect();
    let cpu: Vec<_> = c1.iter().map(|p| convert(*p)).collect();
    let cpu_promoted: Vec<_> = promoted.iter().map(|p| convert(*p)).collect();
    let final_cpu: Vec<_> = cpu.iter().map(|v| v.nonlinear.components()).collect();
    let final_promoted: Vec<_> = cpu_promoted
        .iter()
        .map(|v| v.nonlinear.components())
        .collect();
    let mut gpu = VulkanHdrToSdrQualification::new().unwrap();
    eprintln!("C3 device: {:?}", gpu.device_info());
    let output = gpu.process_linear(width, height, &input).unwrap();
    assert!(
        output
            .nonlinear_709
            .iter()
            .flatten()
            .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
    );
    let mut records = Vec::new();
    records.push(
        metrics(
            "B-original-f64",
            output.nonlinear_2020.as_ref().unwrap(),
            &c1,
            width,
            false,
        )
        .0,
    );
    records.push(
        metrics(
            "B-promoted-f32-input",
            output.nonlinear_2020.as_ref().unwrap(),
            &promoted,
            width,
            false,
        )
        .0,
    );
    let pre: Vec<_> = cpu.iter().map(|v| v.unbounded.components()).collect();
    let pre_promoted: Vec<_> = cpu_promoted
        .iter()
        .map(|v| v.unbounded.components())
        .collect();
    records.push(
        metrics(
            "BC-pre-original-f64",
            output.pre_limit_709.as_ref().unwrap(),
            &pre,
            width,
            false,
        )
        .0,
    );
    records.push(
        metrics(
            "BC-pre-promoted-f32-input",
            output.pre_limit_709.as_ref().unwrap(),
            &pre_promoted,
            width,
            false,
        )
        .0,
    );
    let final_metrics = metrics(
        "BC-final-original-f64",
        &output.nonlinear_709,
        &final_cpu,
        width,
        true,
    );
    records.push(final_metrics.0);
    records.push(
        metrics(
            "BC-final-promoted-f32-input",
            &output.nonlinear_709,
            &final_promoted,
            width,
            true,
        )
        .0,
    );
    // Isolate Pass C from Pass B using the retained sealed C1 output, not GPU B.
    let c1_f32: Vec<_> = c1.iter().map(|p| p.map(|v| v as f32)).collect();
    let c1_promoted: Vec<_> = c1_f32.iter().map(|p| convert(p.map(f64::from))).collect();
    let isolated = gpu.process_c1(width, height, &c1_f32).unwrap();
    let isolated_expected: Vec<_> = c1_promoted
        .iter()
        .map(|v| v.nonlinear.components())
        .collect();
    let isolated_metrics = metrics(
        "C-final-original-f64",
        &isolated.nonlinear_709,
        &final_cpu,
        width,
        true,
    );
    records.push(isolated_metrics.0);
    let isolated_promoted_metrics = metrics(
        "C-final-promoted-f32-input",
        &isolated.nonlinear_709,
        &isolated_expected,
        width,
        true,
    );
    records.push(isolated_promoted_metrics.0);
    let mut failures = Vec::new();
    for (path, result, expected) in [("BC", &output, &final_cpu), ("C", &isolated, &final_cpu)] {
        for (i, (actual, expected)) in result.nonlinear_709.iter().zip(expected).enumerate() {
            for c in 0..3 {
                let difference = unorm16(f64::from(actual[c])).abs_diff(unorm16(expected[c]));
                if difference > 2 {
                    let bounded = result.bounded_709.as_ref().unwrap()[i][c];
                    failures.push(format!("{{\"path\":\"{path}\",\"xy\":[{},{}],\"channel\":{c},\"code_error\":{difference},\"hdr_original\":{:?},\"B_gpu\":{:?},\"B_cpu\":{:?},\"C_gpu_pre\":{:?},\"C_cpu_pre\":{:?},\"gpu_final\":{},\"cpu_final\":{},\"gpu_inverse_power_error\":{}}}",i%width as usize,i/width as usize,original[i],result.nonlinear_2020.as_ref().unwrap()[i],c1[i],result.pre_limit_709.as_ref().unwrap()[i],cpu[i].unbounded.components(),actual[c],expected[c],(f64::from(actual[c])-f64::from(bounded).powf(1.0/2.4)).abs()));
                }
            }
        }
    }
    let mut mismatch = 0u64;
    let mut first_mismatches = Vec::new();
    for (i, (&actual, expected)) in isolated
        .clip_masks
        .as_ref()
        .unwrap()
        .iter()
        .zip(&c1_promoted)
        .enumerate()
    {
        let cpu_mask = mask(expected.unbounded.components());
        if actual != cpu_mask {
            mismatch += 1;
            if first_mismatches.len() < 16 {
                first_mismatches.push(format!("{{\"xy\":[{},{}],\"input\":{:?},\"gpu_pre\":{:?},\"cpu_pre\":{:?},\"gpu_mask\":{actual},\"cpu_mask\":{cpu_mask}}}",i%width as usize,i/width as usize,c1_f32[i],isolated.pre_limit_709.as_ref().unwrap()[i],expected.unbounded.components()));
            }
        }
    }
    let cpu_counts = cpu.iter().fold([0u64; 4], |mut counts, v| {
        counts[mask(v.unbounded.components()).count_ones() as usize] += 1;
        counts
    });
    assert_eq!(cpu_counts, [1138470, 343484, 161835, 429811]);
    eprintln!(
        "C3 clip counts CPU={cpu_counts:?}, GPU={:?}, promoted-input mask mismatches={mismatch}; first={}",
        &isolated.diagnostics[5..],
        first_mismatches.join(",")
    );
    let historical_gate_pass = final_metrics.1 <= 2
        && isolated_metrics.1 <= 2
        && isolated_promoted_metrics.1 <= 2
        && mismatch == 0;
    let numerical_pass = n3_pass(&output.nonlinear_709, &final_cpu)
        && n3_pass(&isolated.nonlinear_709, &final_cpu)
        && [&output, &isolated].into_iter().all(|result| {
            result
                .clip_masks
                .as_ref()
                .unwrap()
                .iter()
                .zip(&cpu)
                .all(|(actual, expected)| *actual == mask(expected.unbounded.components()))
        })
        && mismatch == 0;
    println!(
        "Historical UNORM16 gate PASS={historical_gate_pass}; selected C3B N3 PASS={numerical_pass}"
    );
    let report = format!(
        "{{\"status\":\"{}\",\"scope\":\"C3B N3 selected mode262 canonical B+C and isolated C, not full hardware closure\",\"historical_unorm16_gate_pass\":{historical_gate_pass},\"width\":{width},\"height\":{height},\"float_bytes_per_slot\":{},\"diagnostic_bytes_per_slot\":{},\"diagnostics\":{:?},\"validation_errors\":{},\"cpu_clip_counts\":{cpu_counts:?},\"gpu_clip_counts\":{:?},\"promoted_clip_mask_mismatches\":{mismatch},\"first_clip_mismatches\":[{}],\"over_two_code_samples\":[{}],\"metrics\":[{}]}}\n",
        if numerical_pass { "PASS" } else { "FAIL" },
        gpu.float_bytes_per_slot(),
        gpu.qualification_bytes_per_slot(),
        output.diagnostics,
        gpu.validation_error_count(),
        &isolated.diagnostics[5..],
        first_mismatches.join(","),
        failures.join(","),
        records.join(",")
    );
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(report_path)
        .unwrap()
        .write_all(report.as_bytes())
        .unwrap();
    assert_eq!(gpu.validation_error_count(), 0);
    assert_eq!(mismatch, 0, "exact promoted-input clip-mask gate");
    assert!(
        numerical_pass,
        "derived C3B N3 contract; historical UNORM16 diagnostics retained separately"
    );
}

#[test]
#[ignore = "real GPU domain diagnostics, limiter and fault cleanup"]
fn gpu_domains_limiter_and_fault_cleanup() {
    let mut gpu = VulkanHdrToSdrQualification::new().unwrap();
    let mut vectors = Vec::new();
    for nits in [0.0, 1.0, 10.0, 100.0, 203.0, 400.0, 1000.0] {
        vectors.push([nits; 3]);
    }
    vectors.extend([
        [1000.0, 0.0, 0.0],
        [0.0, 1000.0, 0.0],
        [0.0, 0.0, 1000.0],
        [0.0, 1000.0, 1000.0],
        [1000.0, 0.0, 1000.0],
        [1000.0, 1000.0, 0.0],
        [203.0, 120.0, 80.0],
        [0.0001; 3],
        [0.0; 3],
    ]);
    let rh = 32.0 * (0.1f64).powf(1.0 / 2.4);
    for threshold in [0.7399f64, 0.9909] {
        for offset in [-1e-6, 0.0, 1e-6] {
            let encoded = ((threshold + offset) * (1.0 + rh).ln()).exp_m1() / rh;
            vectors.push([(1000.0 * encoded.powf(2.4)) as f32; 3]);
        }
    }
    vectors.extend([[0.0; 3]; 2]);
    let output = gpu.process_linear(12, 2, &vectors).unwrap();
    let b = output.nonlinear_2020.as_ref().unwrap();
    for group in [&b[..7], &b[16..19], &b[19..22]] {
        for pair in group.windows(2) {
            for (&previous, &next) in pair[0].iter().zip(&pair[1]) {
                assert!(previous <= next);
            }
        }
    }
    let cpu: Vec<_> = vectors.iter().map(|p| method(p.map(f64::from))).collect();
    metrics(
        "Method-A-vectors",
        output.nonlinear_2020.as_ref().unwrap(),
        &cpu,
        12,
        false,
    );
    let expected: Vec<_> = cpu
        .iter()
        .map(|p| convert(*p).nonlinear.components())
        .collect();
    let (_, max, over) = metrics(
        "Method-A-vectors-final",
        &output.nonlinear_709,
        &expected,
        12,
        true,
    );
    assert!(max <= 2 && over == 0);
    for (name, index) in [("green", 8), ("cyan", 10), ("yellow", 12)] {
        let expected = convert(cpu[index]);
        let actual = output.pre_limit_709.as_ref().unwrap()[index];
        assert_eq!(
            output.clip_masks.as_ref().unwrap()[index],
            mask(expected.unbounded.components())
        );
        assert_eq!(
            output.bounded_709.as_ref().unwrap()[index],
            expected.bounded.components().map(|v| v as f32)
        );
        eprintln!(
            "C2A {name}: GPU pre={actual:?}, CPU pre={:?}, exact mask={}, bounded={:?}; no luminance-preservation claim",
            expected.unbounded.components(),
            output.clip_masks.as_ref().unwrap()[index],
            expected.bounded.components()
        );
    }
    for (value, counter) in [(-1.0, 0), (1000.1, 1), (f32::NAN, 2), (f32::INFINITY, 3)] {
        assert!(gpu.process_linear(2, 2, &[[value; 3]; 4]).is_err());
        assert_eq!(gpu.last_diagnostics()[counter], 12);
        assert!(gpu.process_linear(2, 2, &[[0.0; 3]; 4]).is_ok());
        assert_eq!(&gpu.last_diagnostics()[..5], &[0; 5]);
    }
    let epsilon = 2.0f32.powi(-12);
    let samples = [
        -epsilon,
        -0.0,
        0.0,
        epsilon,
        0.25,
        0.5,
        1.0 - epsilon,
        1.0,
        1.0 + epsilon,
    ];
    let mut inputs: Vec<_> = samples.into_iter().map(|v| [v; 3]).collect();
    inputs.extend([[0.1, 0.5, 0.9]; 3]);
    let clipped = gpu.process_limiter(6, 2, &inputs).unwrap();
    let bounded = clipped.bounded_709.as_ref().unwrap();
    for ((source, actual), &actual_mask) in inputs
        .iter()
        .zip(bounded)
        .zip(clipped.clip_masks.as_ref().unwrap())
    {
        assert_eq!(actual_mask, mask(source.map(f64::from)));
        for c in 0..3 {
            assert_eq!(actual[c].to_bits(), source[c].clamp(0.0, 1.0).to_bits());
        }
    }
    let again = gpu.process_limiter(6, 2, bounded).unwrap();
    assert_eq!(again.bounded_709.as_ref().unwrap(), bounded);
    for pair in bounded[..9].windows(2) {
        for (&previous, &next) in pair[0].iter().zip(&pair[1]) {
            assert!(previous <= next);
        }
    }
    eprintln!(
        "limiter epsilon={epsilon}, negative-zero bits={:08x}; exact interior/idempotence/monotonicity PASS",
        bounded[1][0].to_bits()
    );
    for fault in [
        C3Fault::LinearRenderPipeline,
        C3Fault::FloatAAllocation,
        C3Fault::FloatBAllocation,
        C3Fault::ToneMapDescriptor,
        C3Fault::BeforeSubmit,
        C3Fault::BeforeC2bDispatch,
        C3Fault::DiagnosticReadback,
        C3Fault::AfterFence,
    ] {
        let mut slot = gpu.try_fork().unwrap();
        slot.inject_fault(fault);
        assert!(
            slot.process_linear(2, 2, &[[1.0; 3]; 4])
                .unwrap_err()
                .to_string()
                .contains("injected")
        );
        slot.process_linear(2, 2, &[[1.0; 3]; 4]).unwrap();
        assert_eq!(slot.validation_error_count(), 0);
    }
    assert_eq!(gpu.validation_error_count(), 0);
}
