#![cfg(feature = "hdr-to-sdr-qualification")]

//! The B3 canonical source contains 4000/10000-nit patches. It is not a legal
//! fixed-1000-nit MethodA input. Verify actual import and fail-closed behavior,
//! not a selected-valid-subset conversion or a fabricated full-path PASS.
use asciiflow_core::{AsciiConfig, HostFrame, VideoFrame};
use asciiflow_cpu::hdr::HdrPqReference;
use asciiflow_font::GlyphAtlas;
use asciiflow_interop::DrmPrimeMapping;
use asciiflow_media::{DecodeMode, Decoder, VaapiDecodedFrame, VaapiOptions};
use asciiflow_vulkan::VulkanHdrToSdrQualification;
use ffmpeg_sys_next as ffi;
use std::path::Path;

fn downloaded(frame: &VaapiDecodedFrame) -> VideoFrame {
    struct Transfer(*mut ffi::AVFrame);
    impl Drop for Transfer {
        fn drop(&mut self) {
            unsafe { ffi::av_frame_free(&mut self.0) };
        }
    }
    let transfer = Transfer(unsafe { ffi::av_frame_alloc() });
    assert!(!transfer.0.is_null());
    unsafe {
        (*transfer.0).format = ffi::AVPixelFormat::AV_PIX_FMT_P010LE as i32;
    }
    assert!(unsafe { ffi::av_hwframe_transfer_data(transfer.0, frame.as_raw_ptr(), 0) } >= 0);
    let native = unsafe { &*transfer.0 };
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

#[test]
#[ignore = "real Intel VAAPI/DMA-BUF; B3 canonical full frames must reject C1 domain overflow"]
fn b3_canonical_real_import_rejects_above_1000_nits() {
    let config = AsciiConfig::default();
    let atlas = GlyphAtlas::builtin(&config.font, &config.charset).unwrap();
    let reference = HdrPqReference::new(&atlas, &config.charset).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/codecs");
    for name in [
        "hevc-main10-pq-canonical-v1.mp4",
        "av1-main10-pq-canonical-v1.mp4",
    ] {
        let mut decoder = Decoder::open_pq_qualification(
            root.join(name),
            DecodeMode::Vaapi,
            VaapiOptions::default(),
        )
        .unwrap();
        let frame = decoder.next_vaapi_frame().unwrap().unwrap();
        let host = downloaded(&frame);
        let (gw, gh) = config
            .resolved_grid(host.desc().width, host.desc().height)
            .unwrap();
        let grid = reference.map(&host, gw, gh).unwrap();
        let linear = reference.render_linear(&grid, host.desc(), true).unwrap();
        let above = linear
            .pixels()
            .iter()
            .flat_map(|v| [v.r, v.g, v.b])
            .filter(|v| *v > 1000.0)
            .count();
        let maximum = linear
            .pixels()
            .iter()
            .flat_map(|v| [v.r, v.g, v.b])
            .fold(0.0f64, f64::max);
        assert!(above > 0);
        let mapping = DrmPrimeMapping::map_direct_read(frame).unwrap();
        let mut gpu = VulkanHdrToSdrQualification::new().unwrap();
        gpu.submit_external(
            host.desc(),
            mapping.pts(),
            &config,
            mapping.duplicate_external_p010_planes().unwrap(),
            false,
        )
        .unwrap();
        assert!(
            gpu.complete()
                .unwrap_err()
                .to_string()
                .contains("invalid GPU input")
        );
        let diagnostics = gpu.last_diagnostics();
        assert!(diagnostics[1] > 0);
        assert_eq!(diagnostics[0], 0);
        assert_eq!(&diagnostics[2..5], &[0; 3]);
        assert_eq!(gpu.validation_error_count(), 0);
        println!(
            "C3 {name}: real VAAPI/DMA-BUF complete frame: CPU >1000 components={above}, max nits={maximum}; GPU diagnostics={diagnostics:?}; domain rejection PASS, conversion parity NOT QUALIFIED"
        );
    }
}
