#![cfg(all(
    feature = "hdr-to-sdr-production",
    feature = "hdr-to-sdr-qualification"
))]

//! Actual Intel constructor rollback, including failure after slot 0 is ready.
//! These faults precede GPU submission; device-hang quarantine is a separate gate.
use asciiflow_core::{
    AsciiConfig, ColorProcessing, ColorSpace, Error, FrameDesc, PixelFormat, VideoCodec,
};
use asciiflow_font::GlyphAtlas;
use asciiflow_interop::VaapiVulkanFullInteropProcessor;
use asciiflow_media::{DecodeMode, Decoder, VaapiOptions, probe_vaapi_encoder_for};
use asciiflow_vulkan::{C3Fault, SdrPackFault, VulkanAsciiBackend};
use std::path::Path;

#[derive(Clone, Copy, Debug)]
enum InitializationFault {
    Color(C3Fault),
    Pack(SdrPackFault),
}

impl InitializationFault {
    fn options(self, slot: usize) -> (usize, Option<C3Fault>, Option<SdrPackFault>) {
        match self {
            Self::Color(fault) => (slot, Some(fault), None),
            Self::Pack(fault) => (slot, None, Some(fault)),
        }
    }

    fn message(self) -> String {
        match self {
            Self::Color(fault) => format!("C-3 injected {fault:?}"),
            Self::Pack(fault) => format!("SDR pack injected fault: {fault:?}"),
        }
    }
}

fn fd_count() -> usize {
    std::fs::read_dir("/proc/self/fd").unwrap().count()
}

fn exercise(input: &Path, format: PixelFormat, injected: Option<(usize, InitializationFault)>) {
    let config = AsciiConfig {
        color: true,
        ..Default::default()
    };
    let vaapi = VaapiOptions::new(Some("/dev/dri/renderD128".into()));
    let mut decoder =
        Decoder::open_with_pq_preserve(input, DecodeMode::Vaapi, vaapi.clone()).unwrap();
    let frame = decoder
        .next_vaapi_frame()
        .unwrap()
        .expect("legal PQ fixture frame");
    let desc = frame.desc().clone();
    let output_desc = match format {
        PixelFormat::Nv12 => FrameDesc::host_nv12(desc.width, desc.height, ColorSpace::default()),
        PixelFormat::P010Le => {
            FrameDesc::host_p010_le(desc.width, desc.height, ColorSpace::default())
        }
    }
    .unwrap();
    let codec = match format {
        PixelFormat::Nv12 => VideoCodec::H264,
        PixelFormat::P010Le => VideoCodec::Hevc,
    };
    let encoder =
        probe_vaapi_encoder_for(codec, output_desc, decoder.info().frame_rate, vaapi).unwrap();
    let backend = VulkanAsciiBackend::new_for_color_processing(ColorProcessing::HdrPqToSdrBt709)
        .unwrap()
        .with_atlas(
            GlyphAtlas::builtin(&config.font, &config.charset).unwrap(),
            &config,
        )
        .unwrap();
    assert_eq!(
        backend.device_info().vendor_id,
        0x8086,
        "Intel evidence gate"
    );
    assert!(backend.device_info().dma_buf_interop);
    let observe = backend.validation_observer();
    let result = match injected {
        Some((slot, fault)) => VaapiVulkanFullInteropProcessor::new_hdr_to_sdr_with_fault(
            backend,
            encoder.frames,
            desc,
            config,
            format,
            fault.options(slot),
        ),
        None => VaapiVulkanFullInteropProcessor::new_hdr_to_sdr(
            backend,
            encoder.frames,
            desc,
            config,
            format,
        ),
    };
    if let Some((slot, fault)) = injected {
        match result {
            Err(Error::Vulkan(message)) => assert_eq!(
                message,
                fault.message(),
                "{format:?} slot {slot}: {fault:?}"
            ),
            Err(other) => {
                panic!("unexpected root error for {format:?} slot {slot} {fault:?}: {other}")
            }
            Ok(_) => panic!("constructor accepted {format:?} slot {slot} {fault:?}"),
        }
        drop(frame);
    } else {
        let mut processor = result.unwrap();
        assert!(processor.submit(frame).unwrap().is_none());
        let converted = processor.drain().unwrap().expect("resident SDR output");
        assert_eq!(converted.frame.desc().format, format);
        assert_eq!(converted.frame.desc().color_space, ColorSpace::default());
        let native = unsafe { &*converted.frame.as_raw_ptr() };
        assert_eq!(
            native.color_primaries,
            ffmpeg_sys_next::AVColorPrimaries::AVCOL_PRI_BT709
        );
        assert_eq!(
            native.color_trc,
            ffmpeg_sys_next::AVColorTransferCharacteristic::AVCOL_TRC_BT709
        );
        assert_eq!(
            native.colorspace,
            ffmpeg_sys_next::AVColorSpace::AVCOL_SPC_BT709
        );
        assert_eq!(
            native.color_range,
            ffmpeg_sys_next::AVColorRange::AVCOL_RANGE_MPEG
        );
        assert_eq!(
            native.nb_side_data, 0,
            "fresh SDR surface must not clone HDR side data"
        );
        assert_eq!(processor.validation_error_count(), 0);
        drop(converted);
        drop(processor);
    }
    // Decoder owns the input pool and outlives every submitted surface.
    drop(decoder);
    assert_eq!(
        observe(),
        0,
        "Validation must remain clean through destruction"
    );
}

#[test]
#[ignore = "requires actual Intel VAAPI/DMA-BUF with Vulkan Validation; run serially"]
fn c4b_both_slot_initialization_failures_release_resources_and_recover() {
    assert_eq!(std::env::var("ASCIIFLOW_VULKAN_VALIDATION").unwrap(), "1");
    let input = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/codecs/hevc-main10-pq-c3-legal-v1.mp4");
    let faults = [
        InitializationFault::Color(C3Fault::LinearRenderPipeline),
        InitializationFault::Color(C3Fault::FloatAAllocation),
        InitializationFault::Color(C3Fault::FloatBAllocation),
        InitializationFault::Color(C3Fault::ToneMapDescriptor),
        InitializationFault::Pack(SdrPackFault::Pipeline),
        InitializationFault::Pack(SdrPackFault::Buffer),
        InitializationFault::Pack(SdrPackFault::Descriptor),
    ];
    for format in [PixelFormat::Nv12, PixelFormat::P010Le] {
        // Warm lazy driver/library state before measuring exact descriptor return.
        exercise(&input, format, None);
        let baseline = fd_count();
        for slot in 0..2 {
            for fault in faults {
                exercise(&input, format, Some((slot, fault)));
                assert_eq!(
                    fd_count(),
                    baseline,
                    "FD leak after {format:?} slot {slot} {fault:?}"
                );
                exercise(&input, format, None);
                assert_eq!(
                    fd_count(),
                    baseline,
                    "FD leak in healthy recovery after {format:?} slot {slot} {fault:?}"
                );
            }
        }
    }
}
