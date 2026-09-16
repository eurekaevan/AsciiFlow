use asciiflow_core::{
    AsciiConfig, AudioPlan, CapabilitySnapshot, CapabilitySupport, FrameSource,
    InteropCapabilities, MediaCapabilities, PipelinePlan, PipelineStage, ProcessingCapabilities,
    VideoCodec,
};
use asciiflow_interop::DrmPrimeMapping;
use asciiflow_media::{
    DecodeMode, Decoder, MediaInfo, VaapiOptions, probe_vaapi_build, probe_vaapi_encoder_for,
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
            snapshot.media.hevc_vaapi_decode =
                CapabilitySupport::not_probed("VAAPI device probe failed");
            snapshot.media.av1_vaapi_decode =
                CapabilitySupport::not_probed("VAAPI device probe failed");
            snapshot.media.h264_vaapi_encode =
                CapabilitySupport::not_probed("VAAPI device probe failed");
            snapshot.media.hevc_vaapi_encode =
                CapabilitySupport::not_probed("VAAPI device probe failed");
            snapshot.media.nv12_hardware_frames =
                CapabilitySupport::not_probed("VAAPI device probe failed");
            snapshot.media.nv12_hardware_upload =
                CapabilitySupport::not_probed("VAAPI device probe failed");
            snapshot.interop.input = CapabilitySupport::not_probed("VAAPI device probe failed");
            snapshot.interop.hevc_input =
                CapabilitySupport::not_probed("VAAPI device probe failed");
            snapshot.interop.av1_input = CapabilitySupport::not_probed("VAAPI device probe failed");
            snapshot.interop.output = CapabilitySupport::not_probed("VAAPI device probe failed");
            snapshot.interop.hevc_output =
                CapabilitySupport::not_probed("VAAPI device probe failed");
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
            snapshot.interop.hevc_input = CapabilitySupport::not_probed(reason);
            snapshot.interop.av1_input = CapabilitySupport::not_probed(reason);
            snapshot.interop.output = CapabilitySupport::not_probed(reason);
            snapshot.interop.hevc_output = CapabilitySupport::not_probed(reason);
        }
        ProbeFaultPoint::InputInteropQualification => {
            snapshot.interop.input = failed("qualify input DMA-BUF import");
        }
        ProbeFaultPoint::OutputInteropQualification => {
            snapshot.interop.output = failed("qualify output DMA-BUF import");
            snapshot.interop.hevc_output = failed("qualify output DMA-BUF import");
        }
    }
}

pub fn probe(
    input: &Path,
    vaapi: &VaapiOptions,
    config: &AsciiConfig,
) -> asciiflow_core::Result<CapabilityProbe> {
    let started = Instant::now();
    // Font validity is not a hardware capability. Use portable diagnostic masks.
    let diagnostic_config = AsciiConfig {
        font: "builtin-8x8".into(),
        ..config.clone()
    };
    let config = &diagnostic_config;
    let mut software = Decoder::open(input).map_err(|error| {
        asciiflow_core::Error::pipeline(PipelineStage::InputProbe, "open input media", error)
    })?;
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
    let media_info = software.info().clone();
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

    let mut decode_facts = vaapi.probe_decode_profiles();
    let codecs = [
        asciiflow_core::VideoCodec::H264,
        asciiflow_core::VideoCodec::Hevc,
        asciiflow_core::VideoCodec::Av1,
    ];
    for (codec, fact) in codecs.iter().zip(&mut decode_facts) {
        if let Err(error) = asciiflow_media::probe_decoder_build(codec, DecodeMode::Vaapi) {
            *fact = CapabilitySupport::unsupported(error.to_string());
        }
    }
    let mut input_interop = CapabilitySupport::not_probed("VAAPI decode is unavailable");
    let selected_codec = codecs
        .iter()
        .position(|codec| codec == &media_info.requirements.codec);
    if let Some(index) = selected_codec
        && decode_facts[index].is_supported()
    {
        decode_facts[index] = {
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
                                        Err(error) => {
                                            CapabilitySupport::unsupported(error.to_string())
                                        }
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
    }

    let encode_profiles = vaapi.probe_encode_profiles();
    let mut output_interop = [
        CapabilitySupport::not_probed("H.264 VAAPI encode is unavailable"),
        CapabilitySupport::not_probed("HEVC VAAPI encode is unavailable"),
    ];
    let mut nv12_frames = CapabilitySupport::not_probed("VAAPI encode is unavailable");
    let mut nv12_upload = CapabilitySupport::not_probed("VAAPI encode is unavailable");
    let mut encode_facts = [
        CapabilitySupport::not_probed("H.264 encoder not probed"),
        CapabilitySupport::not_probed("HEVC encoder not probed"),
    ];
    for (index, (codec, build_available)) in [
        (VideoCodec::H264, build.h264_encoder),
        (VideoCodec::Hevc, build.hevc_encoder),
    ]
    .into_iter()
    .enumerate()
    {
        if !build_available {
            encode_facts[index] = CapabilitySupport::unsupported(format!(
                "FFmpeg exposes no {}_vaapi encoder",
                codec.to_string().to_ascii_lowercase().replace('.', "")
            ));
            continue;
        }
        if let Some(reason) = encode_profiles[index].unavailable_reason() {
            encode_facts[index] = CapabilitySupport::unsupported(reason);
            continue;
        }
        encode_facts[index] = match probe_vaapi_encoder_for(
            codec,
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
                output_interop[index] = match probe
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
        };
    }

    Ok(CapabilityProbe {
        snapshot: CapabilitySnapshot {
            media: MediaCapabilities {
                software_decode: CapabilitySupport::supported(),
                software_encode,
                vaapi_device,
                h264_vaapi_decode: decode_facts[0].clone(),
                hevc_vaapi_decode: decode_facts[1].clone(),
                av1_vaapi_decode: decode_facts[2].clone(),
                h264_vaapi_encode: encode_facts[0].clone(),
                hevc_vaapi_encode: encode_facts[1].clone(),
                nv12_hardware_frames: nv12_frames,
                nv12_hardware_upload: nv12_upload,
            },
            processing,
            interop: InteropCapabilities {
                input: if selected_codec == Some(0) {
                    input_interop.clone()
                } else {
                    CapabilitySupport::not_probed("no H.264 stream qualified")
                },
                hevc_input: if selected_codec == Some(1) {
                    input_interop.clone()
                } else {
                    CapabilitySupport::not_probed("no HEVC stream qualified")
                },
                av1_input: if selected_codec == Some(2) {
                    input_interop
                } else {
                    CapabilitySupport::not_probed("no AV1 stream qualified")
                },
                output: output_interop[0].clone(),
                hevc_output: output_interop[1].clone(),
            },
        },
        media_info,
        duration: started.elapsed(),
    })
}

pub fn print(
    snapshot: &CapabilitySnapshot,
    media: &MediaInfo,
    audio: &AudioPlan,
    duration: Duration,
) {
    let requirements = &media.requirements;
    println!("Input:");
    println!(
        "  codec/profile: {:?} / {:?}",
        requirements.codec, requirements.profile
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
    println!("Video decode:");
    print_fact("software decode", &snapshot.media.software_decode);
    print_fact("VAAPI device", &snapshot.media.vaapi_device);
    print_fact("VAAPI H.264 decode", &snapshot.media.h264_vaapi_decode);
    print_fact(
        "VAAPI HEVC Main 8-bit decode",
        &snapshot.media.hevc_vaapi_decode,
    );
    print_fact(
        "VAAPI AV1 Main 8-bit decode",
        &snapshot.media.av1_vaapi_decode,
    );
    for codec in [
        asciiflow_core::VideoCodec::H264,
        asciiflow_core::VideoCodec::Hevc,
        asciiflow_core::VideoCodec::Av1,
    ] {
        let fact = match asciiflow_media::probe_decoder_build(&codec, DecodeMode::Software) {
            Ok(_) => CapabilitySupport::supported(),
            Err(error) => CapabilitySupport::unsupported(error.to_string()),
        };
        print_fact(&format!("software {codec} decoder build"), &fact);
    }
    println!("Video encode:");
    print_fact(
        "H.264 8-bit 4:2:0 software",
        &snapshot.media.software_encode,
    );
    print_fact("H.264 8-bit 4:2:0 VAAPI", &snapshot.media.h264_vaapi_encode);
    print_fact(
        "HEVC Main 8-bit 4:2:0 software",
        &CapabilitySupport::unsupported("not implemented"),
    );
    print_fact(
        "HEVC Main 8-bit 4:2:0 VAAPI",
        &snapshot.media.hevc_vaapi_encode,
    );
    print_fact("VAAPI NV12 frames", &snapshot.media.nv12_hardware_frames);
    print_fact(
        "Host to VAAPI NV12 upload",
        &snapshot.media.nv12_hardware_upload,
    );
    println!("Audio:");
    if media.audio_streams.is_empty() {
        println!("  input streams: none");
    }
    for stream in &media.audio_streams {
        println!(
            "  stream #{}: {}{} (codec id {}) · {} Hz · {} ch · {} kb/s · time base {}/{} · start {} · language {} · default {} · forced {} · MP4 copy {}",
            stream.input_index,
            stream.codec,
            stream
                .profile
                .as_deref()
                .map(|profile| format!(" / {profile}"))
                .unwrap_or_default(),
            stream.codec_id,
            stream
                .sample_rate
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".into()),
            stream
                .channels
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".into()),
            stream
                .bit_rate
                .map(|value| format!("{:.1}", value as f64 / 1000.0))
                .unwrap_or_else(|| "unknown".into()),
            stream.time_base.numerator,
            stream.time_base.denominator,
            stream
                .start_time
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".into()),
            stream.language.as_deref().unwrap_or("unknown"),
            stream.default,
            stream.forced,
            if stream.mp4_compatible {
                "supported"
            } else {
                "unsupported"
            },
        );
        if let Some(reason) = &stream.compatibility_reason {
            println!("    {reason}");
        }
    }
    println!("  selected for passthrough: {}", audio.selected.len());
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
    print_fact(
        "VAAPI -> Vulkan (this stream)",
        snapshot.interop.input_for(&requirements.codec),
    );
    print_fact("Vulkan -> VAAPI H.264 output", &snapshot.interop.output);
    print_fact("Vulkan -> VAAPI HEVC output", &snapshot.interop.hevc_output);
    println!("Probe CPU wall: {:.3} ms", duration.as_secs_f64() * 1e3);
}

pub fn print_plan(plan: &PipelinePlan) {
    println!("Selected pipeline:\n  {plan}");
    let profile = plan
        .output
        .profile
        .as_ref()
        .map_or_else(|| "encoder-selected".into(), |value| format!("{value:?}"));
    println!(
        "Output: {} {profile} · {}-bit {:?}",
        plan.output.codec, plan.output.bit_depth, plan.output.chroma_subsampling
    );
    println!("Encoder backend: {:?}", plan.encode);
    println!("Pixel path: {}", plan.pixel_path);
    println!("Reasons:");
    for reason in &plan.reasons {
        println!("  - {reason}");
    }
}

pub fn print_audio_plan(plan: &AudioPlan) {
    println!("Audio plan: {:?}", plan.policy);
    if plan.selected.is_empty() {
        println!("  no audio streams will be copied");
    }
    for stream in &plan.selected {
        println!(
            "  input stream #{}: {} · language {} · default {} → compressed packet copy",
            stream.input_index,
            stream.codec,
            stream.language.as_deref().unwrap_or("unknown"),
            stream.default
        );
    }
    for stream in &plan.skipped {
        println!(
            "  skipped input stream #{} ({}): {}",
            stream.input_index, stream.codec, stream.reason
        );
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
        ChromaSubsampling, ColorSpace, InputRequirements, InteropRequest, MediaRequest,
        PipelinePlanner, PipelinePolicy, ProcessingBackend, Rational, VideoCodec, VulkanDeviceKind,
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
                hevc_vaapi_decode: supported(),
                av1_vaapi_decode: supported(),
                h264_vaapi_encode: supported(),
                hevc_vaapi_encode: supported(),
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
                hevc_input: supported(),
                av1_input: supported(),
                output: supported(),
                hevc_output: supported(),
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
