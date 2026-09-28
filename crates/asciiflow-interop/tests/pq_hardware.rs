#![cfg(feature = "hdr-pq-qualification")]

use asciiflow_core::{AsciiConfig, FrameSource, HostFrame, VideoFrame};
use asciiflow_cpu::hdr::HdrPqReference;
use asciiflow_font::GlyphAtlas;
use asciiflow_interop::{DrmPrimeMapping, VaapiVulkanFullInteropProcessor};
use asciiflow_media::{
    DecodeMode, Decoder, VaapiDecodedFrame, VaapiDiagnosticP010Pool, VaapiOptions,
};
use asciiflow_vulkan::{DiagnosticOutputFault, VulkanPqQualification};
use ffmpeg_sys_next as ffi;
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
};

const FIXTURES: [&str; 2] = [
    "hevc-main10-pq-qualified.mp4",
    "av1-main10-pq-qualified.mp4",
];

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/codecs")
        .join(name)
}

fn config() -> AsciiConfig {
    AsciiConfig {
        grid_width: 8,
        grid_height: Some(8),
        charset: "@%#*+=-:. ".into(),
        font: "builtin-8x8".into(),
        color: true,
    }
}

fn decoder(path: &Path) -> Decoder {
    Decoder::open_pq_qualification(path, DecodeMode::Vaapi, VaapiOptions::default()).unwrap()
}

// Download the very surface handed to Vulkan, so decoder rounding cannot be
// mistaken for a processing mismatch. The scoped guard owns the transfer frame.
fn downloaded(frame: &VaapiDecodedFrame) -> VideoFrame {
    struct TransferFrame(*mut ffi::AVFrame);
    impl Drop for TransferFrame {
        fn drop(&mut self) {
            unsafe { ffi::av_frame_free(&mut self.0) };
        }
    }
    let transfer = TransferFrame(unsafe { ffi::av_frame_alloc() });
    assert!(!transfer.0.is_null());
    unsafe {
        (*transfer.0).format = ffi::AVPixelFormat::AV_PIX_FMT_P010LE as i32;
    }
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
                )
            };
        }
    }
    VideoFrame::new_host(desc.clone(), frame.pts(), storage).unwrap()
}

#[derive(Clone, Copy, Default)]
struct PlaneDifferences {
    counts: [u64; 3], // delta 0, delta 1, delta > 1
    maximum: u16,
}

#[derive(Default)]
struct Differences([PlaneDifferences; 3]);

impl Differences {
    fn add(&mut self, other: Self) {
        for (total, next) in self.0.iter_mut().zip(other.0) {
            for (count, addition) in total.counts.iter_mut().zip(next.counts) {
                *count += addition;
            }
            total.maximum = total.maximum.max(next.maximum);
        }
    }

    fn report(&self, fixture: &str, path: &str, frames: usize) {
        for (plane, differences) in ["Y", "Cb", "Cr"].into_iter().zip(self.0) {
            let samples: u64 = differences.counts.iter().sum();
            // Passing runs contain only 0/1 deltas, so exact nearest-rank
            // percentiles follow directly from these integer histogram bins.
            let quantile = |per_mille: u64| {
                u16::from(samples.saturating_mul(per_mille).div_ceil(1000) > differences.counts[0])
            };
            println!(
                "{fixture} path={path} frames={frames} plane={plane} delta0={} delta1={} delta_gt1={} max={} p50={} p95={} p99={} p99.9={}",
                differences.counts[0],
                differences.counts[1],
                differences.counts[2],
                differences.maximum,
                quantile(500),
                quantile(950),
                quantile(990),
                quantile(999)
            );
        }
    }
}

fn assert_codes(actual: &VideoFrame, expected: &VideoFrame, context: &str) -> Differences {
    assert_eq!(actual.desc(), expected.desc(), "{context}");
    assert_eq!(
        actual.host().as_slice().len(),
        expected.host().as_slice().len()
    );
    let y_samples = actual.desc().y_stride() * actual.desc().height as usize / 2;
    let mut differences = Differences::default();
    for (index, (a, e)) in actual
        .host()
        .as_slice()
        .chunks_exact(2)
        .zip(expected.host().as_slice().chunks_exact(2))
        .enumerate()
    {
        let a = u16::from_le_bytes([a[0], a[1]]);
        let e = u16::from_le_bytes([e[0], e[1]]);
        assert_eq!(a & 63, 0, "{context} padding at {index}");
        let delta = (a >> 6).abs_diff(e >> 6);
        let plane = if index < y_samples {
            0
        } else {
            1 + (index - y_samples) % 2
        };
        differences.0[plane].counts[usize::from(delta.min(2))] += 1;
        differences.0[plane].maximum = differences.0[plane].maximum.max(delta);
        assert!(
            delta <= 1,
            "{context} sample {index}: {} vs {}",
            a >> 6,
            e >> 6
        );
    }
    differences
}

#[test]
#[ignore = "requires Intel iHD/ANV; real PQ HEVC/AV1 DMA-BUF input and output parity"]
fn pq_real_decoder_one_slot_import_and_output_parity() {
    let cfg = config();
    let atlas = GlyphAtlas::builtin(&cfg.font, &cfg.charset).unwrap();
    let reference = HdrPqReference::new(&atlas, &cfg.charset).unwrap();
    for name in FIXTURES {
        let mut decoded = decoder(&fixture(name));
        let mut next = decoded.next_vaapi_frame().unwrap();
        let desc = next.as_ref().unwrap().desc().clone();
        let pool = VaapiDiagnosticP010Pool::new_pq_qualification(
            Path::new("/dev/dri/renderD128"),
            desc.clone(),
        )
        .unwrap();
        let mut backend = VulkanPqQualification::new()
            .unwrap()
            .with_atlas(atlas.clone(), &cfg)
            .unwrap();
        println!("{name} device: {:?}", backend.device_info());
        assert_eq!(backend.device_info().vendor_id, 0x8086);
        let mut count = 0;
        let mut host_differences = Differences::default();
        let mut output_differences = Differences::default();
        let mut active_samples = 0usize;
        let mut nonzero_low_two_bits = 0usize;
        while let Some(frame) = next.take() {
            let host = downloaded(&frame);
            for sample in host.host().as_slice().chunks_exact(2) {
                active_samples += 1;
                nonzero_low_two_bits +=
                    usize::from((u16::from_le_bytes([sample[0], sample[1]]) >> 6) & 3 != 0);
            }
            let expected = reference.process(&host, 8, 8, true).unwrap().0;
            let cells = reference.map(&host, 8, 8).unwrap();
            let input = DrmPrimeMapping::map_direct_read(frame).unwrap();
            if count == 0 {
                println!("{name} actual input descriptor: {:#?}", input.descriptor());
            }
            let imported = backend
                .read_external_p010(
                    &desc,
                    input.pts(),
                    &cfg,
                    input.duplicate_external_p010_planes().unwrap(),
                )
                .unwrap();
            assert_eq!(imported, host, "{name} imported frame {count}");
            let actual = backend
                .process_external_p010(
                    &desc,
                    input.pts(),
                    &cfg,
                    input.duplicate_external_p010_planes().unwrap(),
                )
                .unwrap()
                .frame;
            host_differences.add(assert_codes(&actual, &expected, name));
            let gpu_cells = backend.diagnostics().unwrap();
            assert_eq!(gpu_cells.len(), cells.cells.len());
            for (gpu, cpu) in gpu_cells.iter().zip(&cells.cells) {
                assert_eq!(
                    gpu.counts[0],
                    u32::from(cpu.glyph),
                    "{name} glyph frame {count}"
                );
                assert_eq!(gpu.counts[2], 0, "{name} nonfinite components");
            }
            let output = DrmPrimeMapping::map_direct_write(pool.acquire(count).unwrap()).unwrap();
            if count == 0 {
                println!(
                    "{name} actual output descriptor: {:#?}",
                    output.descriptor()
                );
            }
            backend
                .process_external_to_external(
                    &desc,
                    &cfg,
                    input.duplicate_external_p010_planes().unwrap(),
                    output.duplicate_external_p010_planes().unwrap(),
                )
                .unwrap();
            output_differences.add(assert_codes(
                &output.into_source().download_p010().unwrap(),
                &expected,
                name,
            ));
            count += 1;
            next = decoded.next_vaapi_frame().unwrap();
        }
        assert_eq!(count, 36);
        host_differences.report(name, "one-slot-external-input-host-output", count as usize);
        output_differences.report(
            name,
            "one-slot-external-input-external-output",
            count as usize,
        );
        assert!(
            nonzero_low_two_bits > 0,
            "{name} decoded samples must retain actual 10-bit low bits"
        );
        println!(
            "{name} active_samples={active_samples} nonzero_low_two_bits={nonzero_low_two_bits}"
        );
        assert_eq!(backend.validation_error_count(), 0);
    }
}

fn fd_count() -> usize {
    std::fs::read_dir("/proc/self/fd").unwrap().count()
}

fn two_slot_run(name: &str, target: usize) {
    let before = fd_count();
    let cfg = config();
    let atlas = GlyphAtlas::builtin(&cfg.font, &cfg.charset).unwrap();
    let reference = HdrPqReference::new(&atlas, &cfg.charset).unwrap();
    let mut peak = before;
    let mut active = before;
    let mut differences = Differences::default();
    {
        let mut decoded = decoder(&fixture(name));
        let mut next = decoded.next_vaapi_frame().unwrap();
        let desc = next.as_ref().unwrap().desc().clone();
        let pool = VaapiDiagnosticP010Pool::new_pq_qualification(
            Path::new("/dev/dri/renderD128"),
            desc.clone(),
        )
        .unwrap();
        let backend = VulkanPqQualification::new()
            .unwrap()
            .with_atlas(atlas.clone(), &cfg)
            .unwrap();
        println!("{name} two-slot device: {:?}", backend.device_info());
        assert_eq!(backend.device_info().vendor_id, 0x8086);
        let mut processor = VaapiVulkanFullInteropProcessor::new(
            backend.into_qualification_backend(),
            pool.output_frames().unwrap(),
            desc,
            cfg.clone(),
        )
        .unwrap();
        let mut expected = VecDeque::new();
        let mut submitted = 0;
        let mut completed = 0;
        while submitted < target {
            let Some(frame) = next.take() else {
                // Intel decode-context status state must outlive all its
                // surfaces. Drain before replacing the decoder for a repeat.
                while let Some(output) = processor.drain().unwrap() {
                    assert_eq!(output.frame.pts(), completed as i64);
                    differences.add(assert_codes(
                        &output.frame.download_p010().unwrap(),
                        &expected.pop_front().unwrap(),
                        name,
                    ));
                    completed += 1;
                }
                decoded = decoder(&fixture(name));
                next = decoded.next_vaapi_frame().unwrap();
                continue;
            };
            let host = downloaded(&frame);
            expected.push_back(reference.process(&host, 8, 8, true).unwrap().0);
            if let Some(output) = processor.submit(frame).unwrap() {
                assert_eq!(output.frame.pts(), completed as i64);
                differences.add(assert_codes(
                    &output.frame.download_p010().unwrap(),
                    &expected.pop_front().unwrap(),
                    name,
                ));
                completed += 1;
            }
            submitted += 1;
            let count = fd_count();
            if submitted == 30 {
                active = count;
            }
            peak = peak.max(count);
            if submitted > 30 {
                assert!(
                    count <= active + 12,
                    "{name} FD growth at {submitted}: {count} vs active {active}"
                );
            }
            next = decoded.next_vaapi_frame().unwrap();
        }
        while let Some(output) = processor.drain().unwrap() {
            assert_eq!(output.frame.pts(), completed as i64);
            differences.add(assert_codes(
                &output.frame.download_p010().unwrap(),
                &expected.pop_front().unwrap(),
                name,
            ));
            completed += 1;
        }
        assert_eq!(completed, target);
        assert!(expected.is_empty());
        assert_eq!(processor.validation_error_count(), 0);
    }
    let after = fd_count();
    differences.report(name, "two-slot-external-input-external-output", target);
    println!("{name} frames={target} FD before={before} active={active} peak={peak} after={after}");
    assert_eq!(after, before, "{name} FD baseline not restored");
}

#[test]
#[ignore = "requires Intel iHD/ANV; real PQ two-slot DMA-BUF parity and ordered drain"]
fn pq_real_decoder_two_slot_full_interop_parity() {
    for name in FIXTURES {
        two_slot_run(name, 36);
    }
}

#[test]
#[ignore = "requires Intel iHD/ANV; 3000 real decoded PQ frames per codec and FD stability"]
fn pq_real_decoder_3000_frame_fd_stress() {
    for name in FIXTURES {
        two_slot_run(name, 3000);
    }
}

#[test]
fn pq_qualification_feature_keeps_production_software_rejection() {
    for name in FIXTURES {
        let mut normal = Decoder::open(fixture(name)).unwrap();
        let error = normal.next_frame().expect_err("production must reject PQ");
        assert!(error.to_string().contains("HDR PQ"), "{name}: {error}");
    }
    let mut conflict = Decoder::open_pq_qualification(
        fixture("hevc-main10-pq-bt709-conflict.mp4"),
        DecodeMode::Software,
        VaapiOptions::default(),
    )
    .unwrap();
    assert!(
        conflict.next_frame().is_err(),
        "PQ qualification admitted conflicting BT.709 metadata"
    );
}

#[test]
#[ignore = "requires Intel iHD; qualification feature must preserve production VAAPI HDR rejection"]
fn pq_qualification_feature_keeps_production_vaapi_rejection() {
    for name in FIXTURES {
        let mut normal =
            Decoder::open_with(fixture(name), DecodeMode::Vaapi, VaapiOptions::default()).unwrap();
        let error = normal
            .next_vaapi_frame()
            .err()
            .expect("production must reject PQ");
        assert!(error.to_string().contains("HDR PQ"), "{name}: {error}");
    }
    let mut conflict = decoder(&fixture("hevc-main10-pq-bt709-conflict.mp4"));
    assert!(
        conflict.next_vaapi_frame().is_err(),
        "PQ qualification admitted conflicting VAAPI BT.709 metadata"
    );
}

#[test]
#[ignore = "requires Intel iHD/ANV; original unrestricted metadata fixture is rejected by internal PQ pixels"]
fn pq_invalid_fixture_pixels_are_rejected() {
    let cfg = config();
    let atlas = GlyphAtlas::builtin(&cfg.font, &cfg.charset).unwrap();
    let reference = HdrPqReference::new(&atlas, &cfg.charset).unwrap();
    for name in ["hevc-main10-pq-reject.mp4", "av1-main10-pq.mp4"] {
        let mut decoded = decoder(&fixture(name));
        let frame = match decoded.next_vaapi_frame() {
            Ok(Some(frame)) => frame,
            Err(error) => {
                // This historical metadata fixture does not signal the
                // left-sited chroma required by the internal pixel contract.
                assert_eq!(name, "av1-main10-pq.mp4", "{error}");
                assert!(error.to_string().contains("left-sited"), "{error}");
                continue;
            }
            Ok(None) => panic!("{name}: unexpected EOF"),
        };
        let host = downloaded(&frame);
        assert!(reference.process(&host, 8, 8, true).is_err());
        let mapping = DrmPrimeMapping::map_direct_read(frame).unwrap();
        let mut backend = VulkanPqQualification::new()
            .unwrap()
            .with_atlas(atlas.clone(), &cfg)
            .unwrap();
        let error = backend
            .process_external_p010(
                host.desc(),
                host.pts(),
                &cfg,
                mapping.duplicate_external_p010_planes().unwrap(),
            )
            .err()
            .expect("invalid PQ samples must fail");
        assert!(error.to_string().contains("invalid"), "{name}: {error}");
        assert_eq!(backend.validation_error_count(), 0);
    }
}

#[test]
#[ignore = "requires Intel iHD/ANV; host-to-output PQ DMA-BUF diagnostic lifecycle checkpoints"]
fn pq_host_output_faults_preserve_cause_and_recover_fds() {
    let cfg = config();
    let atlas = GlyphAtlas::builtin(&cfg.font, &cfg.charset).unwrap();
    let reference = HdrPqReference::new(&atlas, &cfg.charset).unwrap();
    for fault in [
        DiagnosticOutputFault::ImageCreate,
        DiagnosticOutputFault::MemoryImport,
        DiagnosticOutputFault::QueueSubmit,
        DiagnosticOutputFault::FenceWaitAfterCompletion,
    ] {
        let before = fd_count();
        {
            let mut decoded = decoder(&fixture(FIXTURES[0]));
            let frame = decoded.next_vaapi_frame().unwrap().unwrap();
            let host = downloaded(&frame);
            let expected = reference.process(&host, 8, 8, true).unwrap().0;
            let pool = VaapiDiagnosticP010Pool::new_pq_qualification(
                Path::new("/dev/dri/renderD128"),
                host.desc().clone(),
            )
            .unwrap();
            let mut backend = VulkanPqQualification::new()
                .unwrap()
                .with_atlas(atlas.clone(), &cfg)
                .unwrap()
                .into_qualification_backend()
                .with_diagnostic_output_fault(fault);
            backend.prepare(host.desc(), &cfg).unwrap();
            {
                let output = DrmPrimeMapping::map_direct_write(pool.acquire(0).unwrap()).unwrap();
                let error = backend
                    .process_host_to_external(
                        host.clone(),
                        &cfg,
                        output.duplicate_external_p010_planes().unwrap(),
                    )
                    .expect_err("fault checkpoint must fail");
                assert!(error.to_string().contains(&format!("{fault:?}")), "{error}");
            }
            // These hooks fire before submission or after real fence completion,
            // so a fresh surface can safely prove one-shot backend recovery.
            let output = DrmPrimeMapping::map_direct_write(pool.acquire(1).unwrap()).unwrap();
            backend
                .process_host_to_external(
                    host,
                    &cfg,
                    output.duplicate_external_p010_planes().unwrap(),
                )
                .unwrap();
            assert_codes(
                &output.into_source().download_p010().unwrap(),
                &expected,
                &format!("{fault:?} recovery"),
            );
            assert_eq!(backend.validation_error_count(), 0);
        }
        assert_eq!(fd_count(), before, "{fault:?} leaked FDs");
    }
}
