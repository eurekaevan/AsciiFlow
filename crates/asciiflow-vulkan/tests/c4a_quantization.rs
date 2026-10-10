#![cfg(feature = "hdr-to-sdr-qualification")]

use asciiflow_core::PixelFormat;
use asciiflow_cpu::sdr_output::{self, NonlinearBt709Rgb};
use asciiflow_vulkan::{SdrPackFormat, VulkanSdrPackQualification};
use std::{fs::OpenOptions, io::Write, path::PathBuf};

const WIDTH: u32 = 2;
const MAX_MISMATCH_SAMPLES_PER_FORMAT: usize = 64;
const MAX_SAMPLES_PER_CHANNEL: usize = 22;

#[derive(Clone, Copy)]
struct BoundaryVector {
    rgb: [f32; 3],
    intended_channel: &'static str,
    intended_code: f64,
    at_half_boundary: bool,
}

struct MismatchSample {
    format: &'static str,
    vector_index: usize,
    rgb_bits: [u32; 3],
    cpu_code: u16,
    gpu_code: u16,
    channel_position: String,
    intended_channel: &'static str,
    intended_code: f64,
    at_half_boundary: bool,
    f64_prequantized_code: f64,
    distance_from_half: f64,
}

#[test]
#[ignore = "requires Vulkan 1.3 qualification device; writes exclusive C4A_BOUNDARY_REPORT"]
fn sdr_pack_matches_f64_oracle_at_y_and_reachable_chroma_boundaries() {
    let mut packer = VulkanSdrPackQualification::new().unwrap();
    let mut mismatch_counts = [0usize; 2];
    let mut vector_counts = [0usize; 2];
    let mut packed_sample_counts = [0usize; 2];
    let mut rgb_diagnostics = [[0u32; 3]; 2];
    let mut p010_nonzero_padding = 0usize;
    let mut mismatch_samples = Vec::new();

    for (format_index, format, gpu_format, name) in [
        (0, PixelFormat::Nv12, SdrPackFormat::Nv12, "NV12"),
        (1, PixelFormat::P010Le, SdrPackFormat::P010, "P010"),
    ] {
        let vectors = boundary_vectors(format);
        vector_counts[format_index] = vectors.len();
        let height = u32::try_from(vectors.len())
            .unwrap()
            .checked_mul(2)
            .expect("test image height fits u32");
        let rgb: Vec<[f32; 3]> = vectors.iter().flat_map(|vector| [vector.rgb; 4]).collect();
        let pixels: Vec<_> = rgb
            .iter()
            .map(|value| NonlinearBt709Rgb(value.map(f64::from)))
            .collect();
        let expected = sdr_output::pack(WIDTH, height, &pixels, format).unwrap();
        let output = packer
            .process(WIDTH, height, &rgb, gpu_format, None)
            .unwrap();
        rgb_diagnostics[format_index] = output.diagnostics;
        if format == PixelFormat::P010Le {
            for word in output.bytes.as_chunks::<2>().0 {
                p010_nonzero_padding +=
                    usize::from(u16::from_le_bytes([word[0], word[1]]) & 63 != 0);
            }
        }

        let sample_bytes = if format == PixelFormat::Nv12 { 1 } else { 2 };
        let samples = output.bytes.len() / sample_bytes;
        packed_sample_counts[format_index] = samples;
        for sample_index in 0..samples {
            let expected_code = decode_code(&expected, format, sample_index);
            let actual_code = decode_code(&output.bytes, format, sample_index);
            if expected_code == actual_code {
                continue;
            }
            mismatch_counts[format_index] += 1;
            let captured_for_format = mismatch_samples
                .iter()
                .filter(|sample: &&MismatchSample| sample.format == name)
                .count();
            if captured_for_format < MAX_MISMATCH_SAMPLES_PER_FORMAT {
                let pixels_per_vector = 4;
                let (vector_index, channel_position) =
                    sample_location(sample_index, pixels_per_vector, vectors.len());
                let channel_group = if channel_position.starts_with('Y') {
                    "Y"
                } else {
                    channel_position.as_str()
                };
                let captured_for_channel = mismatch_samples
                    .iter()
                    .filter(|sample: &&MismatchSample| {
                        sample.format == name
                            && if sample.channel_position.starts_with('Y') {
                                channel_group == "Y"
                            } else {
                                sample.channel_position == channel_group
                            }
                    })
                    .count();
                if captured_for_channel >= MAX_SAMPLES_PER_CHANNEL {
                    continue;
                }
                let vector = vectors[vector_index];
                let prequantized = prequantized_code(
                    &rgb[vector_index * pixels_per_vector..(vector_index + 1) * pixels_per_vector],
                    format,
                    sample_index,
                    pixels_per_vector * vectors.len(),
                );
                mismatch_samples.push(MismatchSample {
                    format: name,
                    vector_index,
                    rgb_bits: vector.rgb.map(f32::to_bits),
                    cpu_code: expected_code,
                    gpu_code: actual_code,
                    channel_position,
                    intended_channel: vector.intended_channel,
                    intended_code: vector.intended_code,
                    at_half_boundary: vector.at_half_boundary,
                    f64_prequantized_code: prequantized,
                    distance_from_half: (prequantized - (prequantized.floor() + 0.5)).abs(),
                });
            }
        }
    }

    let validation_errors = packer.validation_error_count();
    write_report(
        mismatch_counts,
        vector_counts,
        packed_sample_counts,
        rgb_diagnostics,
        p010_nonzero_padding,
        validation_errors,
        &mismatch_samples,
    );
    assert_eq!(
        mismatch_counts,
        [0, 0],
        "CPU/GPU packed-code boundary mismatches; see report"
    );
    assert_eq!(rgb_diagnostics, [[0; 3]; 2], "RGB diagnostic counters");
    assert_eq!(p010_nonzero_padding, 0, "P010 low six bits");
    assert_eq!(validation_errors, 0, "Vulkan validation errors");
}

fn boundary_vectors(format: PixelFormat) -> Vec<BoundaryVector> {
    let codes = sdr_output::LimitedCodes::for_format(format);
    let mut vectors = Vec::new();
    for code in (codes.y_offset as u16)..(codes.y_offset as u16 + codes.y_scale as u16) {
        let threshold = (f64::from(code) + 0.5 - codes.y_offset) / codes.y_scale;
        for value in neighbors(threshold) {
            vectors.push(BoundaryVector {
                rgb: [value; 3],
                intended_channel: "Y",
                intended_code: f64::from(code) + 0.5,
                at_half_boundary: true,
            });
        }
    }

    // One-channel excursions from neutral isolate the reachable Cb/Cr boundary
    // subset while keeping one colored pixel repeated across each 4:2:0 block.
    for (channel, offset, scale, factor) in [
        (
            "Cb",
            codes.c_offset,
            codes.c_scale,
            (1.0 - sdr_output::KB) / 1.8556,
        ),
        (
            "Cr",
            codes.c_offset,
            codes.c_scale,
            (1.0 - sdr_output::KR) / 1.5748,
        ),
    ] {
        let center = codes.c_offset as u16;
        let half_range = (codes.c_scale / 2.0) as u16;
        let low = center - half_range;
        let high = center + half_range;
        for code in low..high {
            let chroma = (f64::from(code) + 0.5 - offset) / scale;
            let component = (0.5 + chroma / factor) as f32;
            for value in neighbors(f64::from(component)) {
                if !(0.0..=1.0).contains(&value) {
                    continue;
                }
                let mut rgb = [0.5f32; 3];
                rgb[if channel == "Cb" { 2 } else { 0 }] = value;
                vectors.push(BoundaryVector {
                    rgb,
                    intended_channel: channel,
                    intended_code: f64::from(code) + 0.5,
                    at_half_boundary: true,
                });
            }
        }
    }
    // Dyadic axis deltas create analytically exact chroma half-code ties on
    // every binary-exact neutral base while keeping RGB components in range.
    for base_numerator in 8..=248 {
        let base = base_numerator as f32 / 256.0;
        for (channel, axis) in [("Cb", 2usize), ("Cr", 0usize)] {
            for delta in [-1.0f32 / 32.0, 1.0f32 / 32.0] {
                let mut rgb = [base; 3];
                rgb[axis] += delta;
                let signal = sdr_output::to_ycbcr(NonlinearBt709Rgb(rgb.map(f64::from))).unwrap();
                let chroma = if channel == "Cb" {
                    signal.cb
                } else {
                    signal.cr
                };
                let intended_code = codes.c_offset + codes.c_scale * chroma;
                vectors.push(BoundaryVector {
                    rgb,
                    intended_channel: channel,
                    intended_code,
                    at_half_boundary: intended_code.fract() == 0.5,
                });
            }
        }
    }
    vectors
}

fn neighbors(value: f64) -> [f32; 3] {
    let nearest = value as f32;
    [adjacent(nearest, false), nearest, adjacent(nearest, true)]
}

fn adjacent(value: f32, up: bool) -> f32 {
    if value == 0.0 {
        return f32::from_bits(if up { 1 } else { 0x8000_0001 });
    }
    let positive = value.is_sign_positive();
    let increment = up == positive;
    f32::from_bits(if increment {
        value.to_bits() + 1
    } else {
        value.to_bits() - 1
    })
}

fn decode_code(bytes: &[u8], format: PixelFormat, sample_index: usize) -> u16 {
    match format {
        PixelFormat::Nv12 => u16::from(bytes[sample_index]),
        PixelFormat::P010Le => {
            let offset = sample_index * 2;
            u16::from_le_bytes([bytes[offset], bytes[offset + 1]]) >> 6
        }
    }
}

fn sample_location(
    sample_index: usize,
    pixels_per_vector: usize,
    vector_count: usize,
) -> (usize, String) {
    let pixel_count = pixels_per_vector * vector_count;
    if sample_index < pixel_count {
        let vector = sample_index / pixels_per_vector;
        let pixel = sample_index % pixels_per_vector;
        return (vector, format!("Y[{pixel}]"));
    }
    let uv_index = sample_index - pixel_count;
    let vector = uv_index / 2;
    let channel = if uv_index.is_multiple_of(2) {
        "Cb"
    } else {
        "Cr"
    };
    (vector, channel.into())
}

fn prequantized_code(
    rgb: &[[f32; 3]],
    format: PixelFormat,
    sample_index: usize,
    pixel_count: usize,
) -> f64 {
    let codes = sdr_output::LimitedCodes::for_format(format);
    if sample_index < pixel_count {
        let pixel = NonlinearBt709Rgb(rgb[sample_index % rgb.len()].map(f64::from));
        let signal = sdr_output::to_ycbcr(pixel).unwrap();
        return codes.y_offset + codes.y_scale * signal.y;
    }
    let local_uv = (sample_index - pixel_count) % 2;
    let signals: Vec<_> = rgb
        .iter()
        .map(|pixel| sdr_output::to_ycbcr(NonlinearBt709Rgb(pixel.map(f64::from))).unwrap())
        .collect();
    let mean = signals
        .iter()
        .map(|signal| if local_uv == 0 { signal.cb } else { signal.cr })
        .sum::<f64>()
        / 4.0;
    codes.c_offset + codes.c_scale * mean
}

fn write_report(
    mismatch_counts: [usize; 2],
    vector_counts: [usize; 2],
    packed_sample_counts: [usize; 2],
    rgb_diagnostics: [[u32; 3]; 2],
    p010_nonzero_padding: usize,
    validation_errors: usize,
    samples: &[MismatchSample],
) {
    let path = std::env::var_os("C4A_BOUNDARY_REPORT")
        .map(PathBuf::from)
        .expect("C4A_BOUNDARY_REPORT must name a new report file");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap_or_else(|error| {
            panic!(
                "create exclusive boundary report {}: {error}",
                path.display()
            )
        });
    write!(
        file,
        "{{\"tested_vector_counts\":{{\"NV12\":{},\"P010\":{}}},\"tested_packed_sample_counts\":{{\"NV12\":{},\"P010\":{}}},\"mismatch_counts\":{{\"NV12\":{},\"P010\":{}}},\"rgb_diagnostics\":{{\"NV12\":[{},{},{}],\"P010\":[{},{},{}]}},\"p010_nonzero_padding\":{p010_nonzero_padding},\"validation_errors\":{validation_errors},\"captured\":{},\"samples\":[",
        vector_counts[0],
        vector_counts[1],
        packed_sample_counts[0],
        packed_sample_counts[1],
        mismatch_counts[0],
        mismatch_counts[1],
        rgb_diagnostics[0][0],
        rgb_diagnostics[0][1],
        rgb_diagnostics[0][2],
        rgb_diagnostics[1][0],
        rgb_diagnostics[1][1],
        rgb_diagnostics[1][2],
        samples.len()
    )
    .unwrap();
    for (index, sample) in samples.iter().enumerate() {
        if index != 0 {
            file.write_all(b",").unwrap();
        }
        write!(
            file,
            "{{\"format\":\"{}\",\"vector_index\":{},\"rgb_f32_bits\":[{},{},{}],\"cpu_active_code\":{},\"gpu_active_code\":{},\"channel_position\":\"{}\",\"intended_channel\":\"{}\",\"intended_code\":{:.17},\"at_half_boundary\":{},\"f64_prequantized_code\":{:.17},\"distance_from_half\":{:.17}}}",
            sample.format,
            sample.vector_index,
            sample.rgb_bits[0],
            sample.rgb_bits[1],
            sample.rgb_bits[2],
            sample.cpu_code,
            sample.gpu_code,
            sample.channel_position,
            sample.intended_channel,
            sample.intended_code,
            sample.at_half_boundary,
            sample.f64_prequantized_code,
            sample.distance_from_half,
        )
        .unwrap();
    }
    file.write_all(b"]}\n").unwrap();
}
