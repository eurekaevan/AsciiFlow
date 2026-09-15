use asciiflow_core::{
    AsciiConfig, CapabilitySnapshot, CapabilitySupport, FrameSource, InputRequirements,
    InteropCapabilities, MediaCapabilities, PipelinePlan, PipelineStage, ProcessingCapabilities,
};
use asciiflow_interop::DrmPrimeMapping;
use asciiflow_media::{
    DecodeMode, Decoder, MediaInfo, VaapiOptions, probe_vaapi_build, probe_vaapi_encoder,
};
use asciiflow_vulkan::VulkanAsciiBackend;
use std::{
    path::Path,
    time::{Duration, Instant},
};

pub struct CapabilityProbe {
    pub snapshot: CapabilitySnapshot,
    pub media_info: MediaInfo,
    pub duration: Duration,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug)]
pub(crate) enum ProbeFaultPoint {
    VaapiDevice,
    VulkanLoader,
    InputInteropQualification,
    OutputInteropQualification,
}

#[cfg(test)]
pub(crate) fn inject_probe_failure(snapshot: &mut CapabilitySnapshot, point: ProbeFaultPoint) {
    let failed = |operation: &str| {
        CapabilitySupport::unsupported(format!("injected capability probe failure: {operation}"))
    };
    match point {
        ProbeFaultPoint::VaapiDevice => {
            snapshot.media.vaapi_device = failed("open VAAPI device");
            snapshot.media.h264_vaapi_decode =
                CapabilitySupport::not_probed("VAAPI device probe failed");
            snapshot.media.h264_vaapi_encode =
                CapabilitySupport::not_probed("VAAPI device probe failed");
            snapshot.media.nv12_hardware_frames =
                CapabilitySupport::not_probed("VAAPI device probe failed");
            snapshot.media.nv12_hardware_upload =
                CapabilitySupport::not_probed("VAAPI device probe failed");
            snapshot.interop.input = CapabilitySupport::not_probed("VAAPI device probe failed");
            snapshot.interop.output = CapabilitySupport::not_probed("VAAPI device probe failed");
        }
        ProbeFaultPoint::VulkanLoader => {
            let reason = "injected Vulkan loader failure";
            snapshot.processing.vulkan = failed("load Vulkan");
            snapshot.processing.vulkan_auto_eligible = false;
            snapshot.processing.vulkan_device_name = None;
            snapshot.processing.vulkan_device_kind = None;
            snapshot.processing.compute_queue = CapabilitySupport::not_probed(reason);
            snapshot.processing.storage_buffer_8bit = CapabilitySupport::not_probed(reason);
            snapshot.processing.shader_int64 = CapabilitySupport::not_probed(reason);
            snapshot.processing.synchronization2 = CapabilitySupport::not_probed(reason);
            snapshot.interop.input = CapabilitySupport::not_probed(reason);
            snapshot.interop.output = CapabilitySupport::not_probed(reason);
        }
        ProbeFaultPoint::InputInteropQualification => {
            snapshot.interop.input = failed("qualify input DMA-BUF import");
        }
        ProbeFaultPoint::OutputInteropQualification => {
            snapshot.interop.output = failed("qualify output DMA-BUF import");
        }
    }
}

pub fn probe(
    input: &Path,
    vaapi: &VaapiOptions,
    config: &AsciiConfig,
) -> asciiflow_core::Result<CapabilityProbe> {
    let started = Instant::now();
    let mut software = Decoder::open(input).map_err(|error| {
        asciiflow_core::Error::pipeline(PipelineStage::InputProbe, "open input media", error)
    })?;
    let media_info = software.info().clone();
    let software_frame = software
        .next_frame()
        .map_err(|error| {
            asciiflow_core::Error::pipeline(
                PipelineStage::InputProbe,
                "decode input qualification frame",
                error,
            )
        })?
        .ok_or_else(|| {
            asciiflow_core::Error::pipeline_message(
                PipelineStage::InputProbe,
                "decode input qualification frame",
                "input contains no decodable video frame",
            )
        })?;
    drop(software);
    let build = probe_vaapi_build();
    let software_encode = if build.software_h264_encoder {
        CapabilitySupport::supported()
    } else {
        CapabilitySupport::unsupported("FFmpeg exposes no software H.264 encoder")
    };
    let vaapi_device = match vaapi.probe_device() {
        Ok(()) => CapabilitySupport::supported(),
        Err(error) => CapabilitySupport::unsupported(error.to_string()),
    };

    let backend_probe = VulkanAsciiBackend::new();
    let (processing, mut backend) = match backend_probe {
        Ok(value) => {
            let info = value.device_info().clone();
            (
                ProcessingCapabilities {
                    cpu: CapabilitySupport::supported(),
                    vulkan: CapabilitySupport::supported(),
                    vulkan_auto_eligible: info.auto_eligible(),
                    vulkan_device_name: Some(info.name.clone()),
                    vulkan_device_kind: Some(info.planner_device_kind()),
                    compute_queue: CapabilitySupport::supported(),
                    storage_buffer_8bit: CapabilitySupport::supported(),
                    shader_int64: CapabilitySupport::supported(),
                    synchronization2: CapabilitySupport::supported(),
                },
                Some(value),
            )
        }
        Err(error) => {
            let reason = error.to_string();
            (
                ProcessingCapabilities {
                    cpu: CapabilitySupport::supported(),
                    vulkan: CapabilitySupport::unsupported(reason.clone()),
                    vulkan_auto_eligible: false,
                    vulkan_device_name: None,
                    vulkan_device_kind: None,
                    compute_queue: CapabilitySupport::unsupported(reason.clone()),
                    storage_buffer_8bit: CapabilitySupport::unsupported(reason.clone()),
                    shader_int64: CapabilitySupport::unsupported(reason.clone()),
                    synchronization2: CapabilitySupport::unsupported(reason),
                },
                None,
            )
        }
    };

    let mut input_interop = CapabilitySupport::not_probed("VAAPI decode is unavailable");
    let h264_decode = if !build.h264_decoder {
        CapabilitySupport::unsupported("FFmpeg exposes no H.264 decoder")
    } else if !matches!(
        media_info.requirements.codec,
        asciiflow_core::VideoCodec::H264
    ) {
        CapabilitySupport::not_probed("the input is not H.264")
    } else {
        match Decoder::open_with(input, DecodeMode::Vaapi, vaapi.clone()) {
            Ok(mut decoder) => match decoder.next_vaapi_frame() {
                Ok(Some(frame)) => {
                    input_interop = match DrmPrimeMapping::map_direct_read(frame) {
                        Ok(mapping) => match backend.as_mut() {
                            Some(vulkan) => {
                                match mapping.duplicate_external_planes().and_then(|planes| {
                                    vulkan
                                        .read_external_nv12(
                                            &media_info.frame_desc,
                                            None,
                                            config,
                                            planes,
                                        )
                                        .map(|_| ())
                                }) {
                                    Ok(()) => CapabilitySupport::supported(),
                                    Err(error) => CapabilitySupport::unsupported(error.to_string()),
                                }
                            }
                            None => CapabilitySupport::not_probed("Vulkan is unavailable"),
                        },
                        Err(error) => CapabilitySupport::unsupported(error.to_string()),
                    };
                    CapabilitySupport::supported()
                }
                Ok(None) => CapabilitySupport::unsupported("input contains no video frame"),
                Err(error) => CapabilitySupport::unsupported(error.to_string()),
            },
            Err(error) => CapabilitySupport::unsupported(error.to_string()),
        }
    };

    let mut output_interop = CapabilitySupport::not_probed("VAAPI encode is unavailable");
    let mut nv12_frames = CapabilitySupport::not_probed("VAAPI encode is unavailable");
    let mut nv12_upload = CapabilitySupport::not_probed("VAAPI encode is unavailable");
    let h264_encode = if !build.h264_encoder {
        CapabilitySupport::unsupported("FFmpeg exposes no h264_vaapi encoder")
    } else {
        match probe_vaapi_encoder(
            media_info.frame_desc.clone(),
            media_info.frame_rate,
            vaapi.clone(),
        ) {
            Ok(probe) => {
                nv12_frames = CapabilitySupport::supported();
                nv12_upload = if probe.host_upload_supported {
                    CapabilitySupport::supported()
                } else {
                    CapabilitySupport::unsupported("VAAPI frames context cannot upload Host NV12")
                };
                output_interop = match probe
                    .frames
                    .acquire(0)
                    .and_then(DrmPrimeMapping::map_direct_write)
                {
                    Ok(mapping) => match backend.as_mut() {
                        Some(vulkan) => {
                            match mapping.duplicate_external_planes().and_then(|planes| {
                                vulkan
                                    .process_nv12_to_external(
                                        software_frame.clone(),
                                        config,
                                        planes,
                                    )
                                    .map(|_| ())
                            }) {
                                Ok(()) => CapabilitySupport::supported(),
                                Err(error) => CapabilitySupport::unsupported(error.to_string()),
                            }
                        }
                        None => CapabilitySupport::not_probed("Vulkan is unavailable"),
                    },
                    Err(error) => CapabilitySupport::unsupported(error.to_string()),
                };
                CapabilitySupport::supported()
            }
            Err(error) => CapabilitySupport::unsupported(error.to_string()),
        }
    };

    Ok(CapabilityProbe {
        snapshot: CapabilitySnapshot {
            media: MediaCapabilities {
                software_decode: CapabilitySupport::supported(),
                software_encode,
                vaapi_device,
                h264_vaapi_decode: h264_decode,
                h264_vaapi_encode: h264_encode,
                nv12_hardware_frames: nv12_frames,
                nv12_hardware_upload: nv12_upload,
            },
            processing,
            interop: InteropCapabilities {
                input: input_interop,
                output: output_interop,
            },
        },
        media_info,
        duration: started.elapsed(),
    })
}

pub fn print(snapshot: &CapabilitySnapshot, requirements: &InputRequirements, duration: Duration) {
    println!("Input:");
    println!(
        "  codec/profile: {:?} / {}",
        requirements.codec,
        requirements.profile.as_deref().unwrap_or("unknown")
    );
    let bit_depth = requirements
        .bit_depth
        .map(|value| format!("{value}-bit"))
        .unwrap_or_else(|| "unknown bit depth".into());
    println!(
        "  format: {} · {} · {:?} · {}x{}",
        requirements.pixel_format.as_deref().unwrap_or("unknown"),
        bit_depth,
        requirements.chroma_subsampling,
        requirements.width,
        requirements.height
    );
    println!(
        "  frame rate: {}/{} · color: {:?}",
        requirements.frame_rate.numerator,
        requirements.frame_rate.denominator,
        requirements.color_space
    );
    println!("Media:");
    print_fact("software decode", &snapshot.media.software_decode);
    print_fact("software H.264 encode", &snapshot.media.software_encode);
    print_fact("VAAPI device", &snapshot.media.vaapi_device);
    print_fact("VAAPI H.264 decode", &snapshot.media.h264_vaapi_decode);
    print_fact("VAAPI H.264 encode", &snapshot.media.h264_vaapi_encode);
    print_fact("VAAPI NV12 frames", &snapshot.media.nv12_hardware_frames);
    print_fact(
        "Host to VAAPI NV12 upload",
        &snapshot.media.nv12_hardware_upload,
    );
    println!("Vulkan:");
    print_fact("CPU processing", &snapshot.processing.cpu);
    print_fact("processing", &snapshot.processing.vulkan);
    if let Some(name) = &snapshot.processing.vulkan_device_name {
        println!("  device: {name}");
    }
    println!(
        "  auto eligible: {}",
        snapshot.processing.vulkan_auto_eligible
    );
    print_fact("compute queue", &snapshot.processing.compute_queue);
    print_fact(
        "storageBuffer8BitAccess",
        &snapshot.processing.storage_buffer_8bit,
    );
    print_fact("shaderInt64", &snapshot.processing.shader_int64);
    print_fact("Synchronization2", &snapshot.processing.synchronization2);
    println!("Interop:");
    print_fact("VAAPI -> Vulkan", &snapshot.interop.input);
    print_fact("Vulkan -> VAAPI", &snapshot.interop.output);
    println!("Probe CPU wall: {:.3} ms", duration.as_secs_f64() * 1e3);
}

pub fn print_plan(plan: &PipelinePlan) {
    println!("Selected pipeline:\n  {plan}");
    println!("Pixel path: {}", plan.pixel_path);
    println!("Reasons:");
    for reason in &plan.reasons {
        println!("  - {reason}");
    }
}

fn print_fact(label: &str, fact: &CapabilitySupport) {
    match fact {
        CapabilitySupport::Supported => println!("  {label}: supported"),
        CapabilitySupport::Unsupported(reason) => println!("  {label}: unsupported ({reason})"),
        CapabilitySupport::NotProbed(reason) => println!("  {label}: not probed ({reason})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use asciiflow_core::{
        ChromaSubsampling, ColorSpace, InteropRequest, MediaRequest, PipelinePlanner,
        PipelinePolicy, ProcessingBackend, Rational, VideoCodec, VulkanDeviceKind,
    };

    fn supported() -> CapabilitySupport {
        CapabilitySupport::supported()
    }

    fn full_snapshot() -> CapabilitySnapshot {
        CapabilitySnapshot {
            media: MediaCapabilities {
                software_decode: supported(),
                software_encode: supported(),
                vaapi_device: supported(),
                h264_vaapi_decode: supported(),
                h264_vaapi_encode: supported(),
                nv12_hardware_frames: supported(),
                nv12_hardware_upload: supported(),
            },
            processing: ProcessingCapabilities {
                cpu: supported(),
                vulkan: supported(),
                vulkan_auto_eligible: true,
                vulkan_device_name: Some("synthetic GPU".into()),
                vulkan_device_kind: Some(VulkanDeviceKind::IntegratedGpu),
                compute_queue: supported(),
                storage_buffer_8bit: supported(),
                shader_int64: supported(),
                synchronization2: supported(),
            },
            interop: InteropCapabilities {
                input: supported(),
                output: supported(),
            },
        }
    }

    fn requirements() -> InputRequirements {
        InputRequirements {
            codec: VideoCodec::H264,
            profile: Some("High".into()),
            pixel_format: Some("yuv420p".into()),
            bit_depth: Some(8),
            chroma_subsampling: ChromaSubsampling::Yuv420,
            width: 1920,
            height: 1080,
            frame_rate: Rational::new(50, 1).unwrap(),
            color_space: ColorSpace::default(),
        }
    }

    #[test]
    fn probe_faults_degrade_auto_without_panicking_or_poisoning_unrelated_facts() {
        for point in [
            ProbeFaultPoint::VaapiDevice,
            ProbeFaultPoint::VulkanLoader,
            ProbeFaultPoint::InputInteropQualification,
            ProbeFaultPoint::OutputInteropQualification,
        ] {
            let mut snapshot = full_snapshot();
            inject_probe_failure(&mut snapshot, point);
            let selected =
                PipelinePlanner::select(&snapshot, &requirements(), PipelinePolicy::default())
                    .unwrap()
                    .selected;
            match point {
                ProbeFaultPoint::VaapiDevice => {
                    assert_eq!(
                        selected.decode,
                        asciiflow_core::MediaImplementation::Software
                    );
                    assert_eq!(
                        selected.encode,
                        asciiflow_core::MediaImplementation::Software
                    );
                    assert!(snapshot.processing.vulkan.is_supported());
                }
                ProbeFaultPoint::VulkanLoader => {
                    assert_eq!(selected.backend, ProcessingBackend::Cpu);
                    assert!(snapshot.media.vaapi_device.is_supported());
                }
                ProbeFaultPoint::InputInteropQualification => {
                    assert!(!selected.hardware_input_interop);
                    assert!(snapshot.interop.output.is_supported());
                }
                ProbeFaultPoint::OutputInteropQualification => {
                    assert!(!selected.hardware_output_interop);
                    assert!(snapshot.interop.input.is_supported());
                }
            }
        }
    }

    #[test]
    fn explicit_policy_reports_probe_failure_during_planning() {
        let cases = [
            (
                ProbeFaultPoint::VaapiDevice,
                PipelinePolicy {
                    decode: MediaRequest::Hardware,
                    ..PipelinePolicy::default()
                },
            ),
            (
                ProbeFaultPoint::VulkanLoader,
                PipelinePolicy {
                    backend: ProcessingBackend::Vulkan,
                    ..PipelinePolicy::default()
                },
            ),
            (
                ProbeFaultPoint::InputInteropQualification,
                PipelinePolicy {
                    input_interop: InteropRequest::On,
                    ..PipelinePolicy::default()
                },
            ),
            (
                ProbeFaultPoint::OutputInteropQualification,
                PipelinePolicy {
                    output_interop: InteropRequest::On,
                    ..PipelinePolicy::default()
                },
            ),
        ];
        for (point, policy) in cases {
            let mut snapshot = full_snapshot();
            inject_probe_failure(&mut snapshot, point);
            let error = PipelinePlanner::select(&snapshot, &requirements(), policy).unwrap_err();
            assert!(error.to_string().contains("injected"));
        }
    }
}
