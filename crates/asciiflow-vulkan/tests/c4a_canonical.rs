#![cfg(feature = "hdr-to-sdr-qualification")]

use asciiflow_core::PixelFormat;
use asciiflow_cpu::sdr_output::{NonlinearBt709Rgb, pack};
use asciiflow_vulkan::{SdrPackFormat, VulkanHdrToSdrQualification};
use std::{fs::OpenOptions, io::Write, process::Command};

#[test]
#[ignore = "canonical 1080p C-3 input; C1_INPUT and exclusive C4A_CANONICAL_REPORT required"]
fn unchanged_c3_canonical_signal_to_both_formats() {
    let input = std::env::var_os("C1_INPUT").expect("C1_INPUT");
    let hash = Command::new("sha256sum").arg(&input).output().unwrap();
    assert!(hash.status.success());
    assert_eq!(
        String::from_utf8(hash.stdout)
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap(),
        "a6246d4e5e34c1c69c9cd629e35d82a0f1e8af387436044c82483511bd749f33"
    );
    let bytes = std::fs::read(input).unwrap();
    let magic = b"AF-C1-IN-v1\0";
    assert!(bytes.starts_with(magic));
    let offset = magic.len();
    let width = u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
    let height = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap());
    assert_eq!((width, height), (1920, 1080));
    assert_eq!(
        bytes.len(),
        offset + 8 + width as usize * height as usize * 24
    );
    let input: Vec<[f32; 3]> = bytes[offset + 8..]
        .as_chunks::<24>()
        .0
        .iter()
        .map(|p| {
            std::array::from_fn(|c| {
                f64::from_le_bytes(p[c * 8..c * 8 + 8].try_into().unwrap()) as f32
            })
        })
        .collect();
    let mut slot = VulkanHdrToSdrQualification::new().unwrap();
    let observer = slot.validation_observer();
    let output = slot.process_linear(width, height, &input).unwrap();
    let reference: Vec<_> = output
        .nonlinear_709
        .iter()
        .map(|v| NonlinearBt709Rgb(v.map(f64::from)))
        .collect();
    let mut records = Vec::new();
    for (format, gpu_format) in [
        (PixelFormat::Nv12, SdrPackFormat::Nv12),
        (PixelFormat::P010Le, SdrPackFormat::P010),
    ] {
        let cpu = pack(width, height, &reference, format).unwrap();
        let gpu = slot.pack_completed_sdr(gpu_format, None, None).unwrap();
        let differences: Vec<_> = cpu
            .iter()
            .zip(&gpu.bytes)
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .collect();
        if !differences.is_empty() {
            eprintln!(
                "canonical {format:?} byte mismatches={} first={:?}",
                differences.len(),
                &differences[..differences.len().min(32)]
            );
        }
        assert!(
            differences.is_empty(),
            "canonical packer must match f64 oracle on identical C-3 output"
        );
        assert_eq!(gpu.diagnostics, [0; 3]);
        records.push(format!(
            "{{\"format\":\"{format:?}\",\"bytes\":{},\"mismatches\":0,\"gpu_pack_ms\":{}}}",
            gpu.bytes.len(),
            gpu.gpu_pack.as_secs_f64() * 1000.
        ));
    }
    drop(slot);
    assert_eq!(observer(), 0);
    let destination = std::env::var_os("C4A_CANONICAL_REPORT").expect("C4A_CANONICAL_REPORT");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .unwrap();
    writeln!(file,"{{\"input_sha256\":\"a6246d4e5e34c1c69c9cd629e35d82a0f1e8af387436044c82483511bd749f33\",\"width\":{width},\"height\":{height},\"oracle_scope\":\"f64 packing of identical resident C-3 f32 output; not exact end-to-end f64 tone-map parity\",\"validation_errors_through_teardown\":0,\"formats\":[{}]}}",records.join(",")).unwrap();
}
