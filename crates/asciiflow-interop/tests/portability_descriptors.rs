//! Actual owned surfaces, not reconstructed canonical layouts.
#![cfg(feature = "hdr-to-sdr-production")]
use asciiflow_core::PixelFormat;
use asciiflow_interop::{DrmPrimeFrameDesc, DrmPrimeMapping};
use asciiflow_media::{DecodeMode, Decoder, VaapiOptions, probe_vaapi_encoder_for};
use asciiflow_vulkan::{ExternalImageAccess, VulkanAsciiBackend};
use serde_json::{Value, json};
use std::{fs::OpenOptions, path::PathBuf};

fn descriptor(value: &DrmPrimeFrameDesc) -> Value {
    json!({
        "width": value.width, "height": value.height,
        "object_count": value.objects.len(), "layer_count": value.layers.len(),
        "plane_count": value.layers.iter().map(|layer| layer.planes.len()).sum::<usize>(),
        // FD numbers are process-local handles, never layout identity.
        "objects": value.objects.iter().map(|object| json!({
            "size": object.size, "modifier": object.format_modifier
        })).collect::<Vec<_>>(),
        "layers": value.layers.iter().map(|layer| json!({
            "drm_format": layer.format,
            "planes": layer.planes.iter().map(|plane| json!({
                "object_index": plane.object_index, "offset": plane.offset, "pitch": plane.pitch
            })).collect::<Vec<_>>()
        })).collect::<Vec<_>>()
    })
}

fn fd_count() -> usize {
    std::fs::read_dir("/proc/self/fd").unwrap().count()
}

#[test]
#[ignore = "actual Intel VAAPI/ANV NV12/P010 owned surface descriptor capture"]
fn actual_input_and_encoder_output_descriptors() {
    assert_eq!(
        std::env::var("ASCIIFLOW_VULKAN_VALIDATION").as_deref(),
        Ok("1"),
        "descriptor qualification requires enabled Khronos Validation"
    );
    let report = PathBuf::from(std::env::var_os("ASCIIFLOW_DESCRIPTOR_REPORT").unwrap());
    let nv12 = PathBuf::from(std::env::var_os("ASCIIFLOW_TEST_VIDEO").unwrap());
    let p010 = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/codecs/hevc-main10-canonical-v1.mp4");
    let baseline = fd_count();
    let mut peak = baseline;
    let mut records = Vec::new();
    {
        let mut vulkan = VulkanAsciiBackend::new().unwrap();
        assert_eq!(vulkan.device_info().vendor_id, 0x8086);
        let observer = vulkan.validation_observer();
        for (format, path) in [(PixelFormat::Nv12, nv12), (PixelFormat::P010Le, p010)] {
            let mut decoder =
                Decoder::open_with(path, DecodeMode::Vaapi, VaapiOptions::default()).unwrap();
            let frame = decoder.next_vaapi_frame().unwrap().unwrap();
            let desc = frame.desc().clone();
            assert_eq!(desc.format, format);
            let input = DrmPrimeMapping::map_direct_read(frame).unwrap();
            let codec = if format == PixelFormat::Nv12 {
                asciiflow_core::VideoCodec::H264
            } else {
                asciiflow_core::VideoCodec::Hevc
            };
            let encoder = probe_vaapi_encoder_for(
                codec,
                desc.clone(),
                decoder.info().frame_rate,
                VaapiOptions::default(),
            )
            .unwrap();
            let output =
                DrmPrimeMapping::map_direct_write(encoder.frames.acquire(0).unwrap()).unwrap();
            for (direction, mapping) in [
                ("input", input.descriptor()),
                ("output", output.descriptor()),
            ] {
                records.push(
                    json!({"format": format!("{format:?}"), "direction": direction,
                                    "descriptor": descriptor(mapping)}),
                );
            }
            for (access, planes) in [
                (
                    ExternalImageAccess::Read,
                    input.duplicate_external_planes_for(format).unwrap(),
                ),
                (
                    ExternalImageAccess::Write,
                    output.duplicate_external_planes_for(format).unwrap(),
                ),
            ] {
                if format == PixelFormat::Nv12 {
                    vulkan.probe_external_nv12(&desc, planes, access).unwrap();
                } else {
                    vulkan.probe_external_p010(&desc, planes, access).unwrap();
                }
            }
            peak = peak.max(fd_count());
        }
        assert_eq!(observer(), 0);
        drop(vulkan);
        assert_eq!(observer(), 0);
    }
    let after = fd_count();
    assert_eq!(after, baseline, "surface capture leaked descriptors");
    let document = json!({"schema_version": 1, "surfaces": records,
                          "fd": {"baseline": baseline, "peak": peak, "after": after},
                          "validation_errors": 0});
    serde_json::to_writer_pretty(
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(report)
            .unwrap(),
        &document,
    )
    .unwrap();
}
