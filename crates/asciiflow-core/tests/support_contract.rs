use asciiflow_core::{
    AudioPolicy, CapabilitySnapshot, CapabilitySupport, ChromaLocation, ChromaSubsampling,
    ColorMatrix, ColorPrimaries, ColorRange, ColorResolutionPolicy, ColorSpace, InputRequirements,
    InteropCapabilities, InteropRequest, MediaCapabilities, MediaRequest, OutputDynamicRange,
    PipelinePlanner, PipelinePolicy, PixelFormat, ProcessingBackend, ProcessingCapabilities,
    Rational, ResolvedColorSemantics, TransferCharacteristic, VideoCodec, VideoProfile,
    VulkanDeviceKind,
};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Contract {
    schema_version: u32,
    support_contract_version: String,
    states: BTreeMap<String, String>,
    hardware_scope: HardwareScope,
    production_limits: ProductionLimits,
    evidence: Vec<Evidence>,
    dimensions: BTreeMap<String, Vec<DimensionValue>>,
    cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProductionLimits {
    hdr_to_sdr: HdrToSdrLimit,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct HardwareScope {
    gpu: String,
    vulkan: String,
    vaapi: String,
    ffmpeg: String,
    media_device: String,
    limitations: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct HdrToSdrLimit {
    decoded_max_nits: u32,
    unit: String,
    applies_to: String,
    measurement: String,
    applies_to_pq_preserve: bool,
    enforced_before_output_pack: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Evidence {
    id: String,
    stage: String,
    kind: String,
    artifact: String,
    status: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DimensionValue {
    value: String,
    status: String,
    evidence: Vec<String>,
    conditions: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    id: String,
    input_codec: String,
    input_profile: String,
    input_depth: u8,
    color: String,
    output_codec: String,
    output_depth: u8,
    output_dynamic_range: String,
    backend: String,
    decode: String,
    encode: String,
    input_interop: String,
    output_interop: String,
    missing_capability: Option<String>,
    status: String,
    expected_planner: ExpectedPlanner,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedPlanner {
    result: String,
    backend: Option<String>,
    decode: Option<String>,
    encode: Option<String>,
    output_pixel_format: Option<String>,
    color_processing: Option<String>,
    error_variant: Option<String>,
    color_reason: Option<String>,
}

fn fact() -> CapabilitySupport {
    CapabilitySupport::Supported
}

fn qualified_capabilities() -> CapabilitySnapshot {
    CapabilitySnapshot {
        media: MediaCapabilities {
            software_decode: fact(),
            software_encode: fact(),
            vaapi_device: fact(),
            h264_vaapi_decode: fact(),
            hevc_vaapi_decode: fact(),
            av1_vaapi_decode: fact(),
            h264_vaapi_encode: fact(),
            hevc_vaapi_encode: fact(),
            av1_vaapi_encode: fact(),
            hevc_main10_vaapi_decode: fact(),
            av1_10bit_vaapi_decode: fact(),
            hevc_main10_vaapi_encode: fact(),
            av1_10bit_vaapi_encode: fact(),
            nv12_hardware_frames: fact(),
            nv12_hardware_upload: fact(),
            p010_hardware_frames: fact(),
            p010_hardware_upload: fact(),
        },
        processing: ProcessingCapabilities {
            cpu: fact(),
            vulkan: fact(),
            vulkan_pq: fact(),
            vulkan_hdr_to_sdr: fact(),
            vulkan_auto_eligible: true,
            vulkan_device_name: Some("contract test device".into()),
            vulkan_device_kind: Some(VulkanDeviceKind::IntegratedGpu),
            compute_queue: fact(),
            storage_buffer_8bit: fact(),
            shader_int64: fact(),
            synchronization2: fact(),
        },
        interop: InteropCapabilities {
            input: fact(),
            hevc_input: fact(),
            av1_input: fact(),
            output: fact(),
            hevc_output: fact(),
            av1_output: fact(),
            p010_input: fact(),
            p010_output: fact(),
            av1_p010_output: fact(),
        },
    }
}

fn remove_capability(snapshot: &mut CapabilitySnapshot, capability: &str) {
    let unavailable = CapabilitySupport::unsupported("removed by contract test");
    match capability {
        "vulkan_hdr_to_sdr" => snapshot.processing.vulkan_hdr_to_sdr = unavailable,
        "p010_input" => snapshot.interop.p010_input = unavailable,
        "hevc_main10_encode" => snapshot.media.hevc_main10_vaapi_encode = unavailable,
        other => panic!("unknown missing capability {other}"),
    }
}

fn codec(name: &str) -> VideoCodec {
    match name {
        "h264" => VideoCodec::H264,
        "hevc" => VideoCodec::Hevc,
        "av1" => VideoCodec::Av1,
        other => panic!("unknown contract codec {other}"),
    }
}

fn profile(name: &str) -> VideoProfile {
    match name {
        "h264-baseline" => VideoProfile::H264Baseline,
        "h264-main" => VideoProfile::H264Main,
        "h264-high" => VideoProfile::H264High,
        "hevc-main" => VideoProfile::HevcMain,
        "hevc-main10" => VideoProfile::HevcMain10,
        "av1-main" => VideoProfile::Av1Main,
        other => VideoProfile::Other(other.into()),
    }
}

fn color_space(color: &str) -> ColorSpace {
    match color {
        "sdr709" => ColorSpace::default(),
        "sdr601" => ColorSpace {
            matrix: ColorMatrix::Bt601,
            primaries: ColorPrimaries::Smpte170M,
            transfer: TransferCharacteristic::Smpte170M,
            ..ColorSpace::default()
        },
        "pq" => ColorSpace::pq_bt2020(),
        "hlg" => ColorSpace {
            transfer: TransferCharacteristic::Hlg,
            ..ColorSpace::pq_bt2020()
        },
        "full_pq" => ColorSpace {
            range: ColorRange::Full,
            ..ColorSpace::pq_bt2020()
        },
        "wide_sdr" => ColorSpace {
            matrix: ColorMatrix::Bt2020,
            primaries: ColorPrimaries::Bt2020,
            transfer: TransferCharacteristic::Bt709,
            ..ColorSpace::default()
        },
        "unknown" => ColorSpace {
            matrix: ColorMatrix::Unspecified,
            range: ColorRange::Unspecified,
            primaries: ColorPrimaries::Unspecified,
            transfer: TransferCharacteristic::Unspecified,
            chroma_location: asciiflow_core::ChromaLocation::Unspecified,
        },
        "conflict" => ColorSpace::default(),
        other => panic!("unknown contract color {other}"),
    }
}

fn requirements(case: &Case) -> InputRequirements {
    let space = color_space(&case.color);
    let raw = asciiflow_core::ColorMetadataRaw {
        space,
        ..asciiflow_core::ColorMetadataRaw::unspecified()
    };
    let frame_raw = if case.color == "conflict" {
        asciiflow_core::ColorMetadataRaw {
            space: ColorSpace {
                matrix: ColorMatrix::Bt601,
                ..space
            },
            ..raw
        }
    } else {
        raw
    };
    let semantics = ResolvedColorSemantics::resolve(
        raw,
        frame_raw,
        if case.input_depth == 10 {
            ColorResolutionPolicy::StrictTenBit
        } else {
            ColorResolutionPolicy::LegacyEightBit
        },
    )
    .unwrap();
    InputRequirements {
        codec: codec(&case.input_codec),
        profile: Some(profile(&case.input_profile)),
        pixel_format: Some(
            if case.input_depth == 10 {
                "p010le"
            } else {
                "nv12"
            }
            .into(),
        ),
        bit_depth: Some(case.input_depth),
        chroma_subsampling: ChromaSubsampling::Yuv420,
        width: 1920,
        height: 1080,
        frame_rate: Rational::new(30, 1).unwrap(),
        color_space: space,
        color_semantics: Some(semantics),
    }
}

fn media_request(value: &str) -> MediaRequest {
    match value {
        "software" => MediaRequest::Software,
        "hardware" => MediaRequest::Hardware,
        "auto" => MediaRequest::Auto,
        other => panic!("unknown media request {other}"),
    }
}

fn interop_request(value: &str) -> InteropRequest {
    match value {
        "off" => InteropRequest::Off,
        "on" => InteropRequest::On,
        "auto" => InteropRequest::Auto,
        other => panic!("unknown interop request {other}"),
    }
}

fn policy(case: &Case) -> PipelinePolicy {
    PipelinePolicy {
        backend: match case.backend.as_str() {
            "cpu" => ProcessingBackend::Cpu,
            "vulkan" => ProcessingBackend::Vulkan,
            "auto" => ProcessingBackend::Auto,
            other => panic!("unknown backend {other}"),
        },
        decode: media_request(&case.decode),
        encode: media_request(&case.encode),
        input_interop: interop_request(&case.input_interop),
        output_interop: interop_request(&case.output_interop),
        output_codec: codec(&case.output_codec),
        output_bit_depth: case.output_depth,
        output_dynamic_range: match case.output_dynamic_range.as_str() {
            "preserve" => OutputDynamicRange::Preserve,
            "sdr" => OutputDynamicRange::Sdr,
            other => panic!("unknown output dynamic range {other}"),
        },
    }
}

fn assert_enum_dimension(contract: &Contract, name: &str, actual: &[&str]) {
    let expected = contract
        .dimensions
        .get(name)
        .unwrap_or_else(|| panic!("missing {name} dimension"));
    let actual = actual
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    let expected = expected
        .iter()
        .map(|item| item.value.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(expected, actual, "{name} enum coverage differs from core");
}

fn assert_enum_members_present(contract: &Contract, name: &str, actual: &[&str]) {
    let values = contract
        .dimensions
        .get(name)
        .unwrap_or_else(|| panic!("missing {name} dimension"));
    let actual = actual
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    let declared = values
        .iter()
        .map(|item| item.value.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    assert!(
        actual.is_subset(&declared),
        "{name} omits core enum values: {:?}",
        actual.difference(&declared).collect::<Vec<_>>()
    );
}

fn codec_value(value: &VideoCodec) -> &'static str {
    match value {
        VideoCodec::H264 => "h264",
        VideoCodec::Hevc => "hevc",
        VideoCodec::Av1 => "av1",
        VideoCodec::Other(_) => "other",
    }
}

fn profile_value(value: &VideoProfile) -> &'static str {
    match value {
        VideoProfile::H264Baseline => "h264-baseline",
        VideoProfile::H264Main => "h264-main",
        VideoProfile::H264High => "h264-high",
        VideoProfile::HevcMain => "hevc-main",
        VideoProfile::HevcMain10 => "hevc-main10",
        VideoProfile::Av1Main => "av1-main",
        VideoProfile::Other(_) => "other",
    }
}

fn subsampling_value(value: ChromaSubsampling) -> &'static str {
    match value {
        ChromaSubsampling::Yuv420 => "yuv420",
        ChromaSubsampling::Other => "other",
        ChromaSubsampling::Unknown => "unknown",
    }
}

fn chroma_location_value(value: ChromaLocation) -> &'static str {
    match value {
        ChromaLocation::Left => "left",
        ChromaLocation::Center => "center",
        ChromaLocation::Unspecified => "unspecified",
    }
}

fn pixel_format_value(value: PixelFormat) -> &'static str {
    match value {
        PixelFormat::Nv12 => "nv12",
        PixelFormat::P010Le => "p010le",
    }
}

fn primaries_value(value: ColorPrimaries) -> &'static str {
    match value {
        ColorPrimaries::Bt709 => "bt709",
        ColorPrimaries::Bt2020 => "bt2020",
        ColorPrimaries::Bt470Bg => "bt470bg",
        ColorPrimaries::Smpte170M => "smpte170m",
        ColorPrimaries::Smpte240M => "smpte240m",
        ColorPrimaries::DisplayP3 => "display-p3",
        ColorPrimaries::Other => "other",
        ColorPrimaries::Unspecified => "unspecified",
        ColorPrimaries::Unknown(_) => "unknown",
    }
}

fn transfer_value(value: TransferCharacteristic) -> &'static str {
    match value {
        TransferCharacteristic::Bt709 => "bt709",
        TransferCharacteristic::Smpte170M => "smpte170m",
        TransferCharacteristic::Srgb => "srgb",
        TransferCharacteristic::Gamma22 => "gamma22",
        TransferCharacteristic::Gamma28 => "gamma28",
        TransferCharacteristic::Linear => "linear",
        TransferCharacteristic::Pq => "pq",
        TransferCharacteristic::Hlg => "hlg",
        TransferCharacteristic::Other => "other",
        TransferCharacteristic::Unspecified => "unspecified",
        TransferCharacteristic::Unknown(_) => "unknown",
    }
}

fn matrix_value(value: ColorMatrix) -> &'static str {
    match value {
        ColorMatrix::Bt601 => "bt601",
        ColorMatrix::Bt709 => "bt709",
        ColorMatrix::Bt2020 => "bt2020-ncl",
        ColorMatrix::Bt2020Constant => "bt2020-constant",
        ColorMatrix::Identity => "identity",
        ColorMatrix::Other => "other",
        ColorMatrix::Unspecified => "unspecified",
        ColorMatrix::Unknown(_) => "unknown",
    }
}

fn range_value(value: ColorRange) -> &'static str {
    match value {
        ColorRange::Limited => "limited",
        ColorRange::Full => "full",
        ColorRange::Unspecified => "unspecified",
    }
}

fn dynamic_range_value(value: asciiflow_core::DynamicRangeClass) -> &'static str {
    match value {
        asciiflow_core::DynamicRangeClass::Sdr => "sdr",
        asciiflow_core::DynamicRangeClass::HdrPq => "pq",
        asciiflow_core::DynamicRangeClass::HdrHlg => "hlg",
        asciiflow_core::DynamicRangeClass::Unknown => "unknown",
        asciiflow_core::DynamicRangeClass::Conflicting => "conflicting",
    }
}

fn media_request_value(value: MediaRequest) -> &'static str {
    match value {
        MediaRequest::Auto => "auto",
        MediaRequest::Software => "software",
        MediaRequest::Hardware => "vaapi",
    }
}

fn backend_value(value: ProcessingBackend) -> &'static str {
    match value {
        ProcessingBackend::Auto => "auto",
        ProcessingBackend::Cpu => "cpu",
        ProcessingBackend::Vulkan => "vulkan",
    }
}

fn interop_value(value: InteropRequest) -> &'static str {
    match value {
        InteropRequest::Auto => "auto",
        InteropRequest::Off => "off",
        InteropRequest::On => "on",
    }
}

fn output_range_value(value: OutputDynamicRange) -> &'static str {
    match value {
        OutputDynamicRange::Preserve => "preserve",
        OutputDynamicRange::Sdr => "sdr",
    }
}

fn audio_policy_value(value: AudioPolicy) -> &'static str {
    match value {
        AudioPolicy::Auto => "auto",
        AudioPolicy::Copy => "copy",
        AudioPolicy::None => "none",
    }
}

fn color_reason_name(reason: asciiflow_core::ColorSupportReason) -> &'static str {
    use asciiflow_core::ColorSupportReason::*;
    match reason {
        UnsupportedHdrPq => "UnsupportedHdrPq",
        UnsupportedHdrHlg => "UnsupportedHdrHlg",
        UnsupportedWideGamutSdr => "UnsupportedWideGamutSdr",
        Unknown => "Unknown",
        Conflicting => "Conflicting",
        UnsupportedSdrProfile => "UnsupportedSdrProfile",
        UnsupportedFullRange => "UnsupportedFullRange",
    }
}

fn assert_planner_error(error: asciiflow_core::Error, expected: &ExpectedPlanner, id: &str) {
    match (expected.error_variant.as_deref().unwrap(), error) {
        ("InvalidConfig", asciiflow_core::Error::InvalidConfig(_)) => {}
        ("UnsupportedFrame", asciiflow_core::Error::UnsupportedFrame(_)) => {}
        ("UnsupportedColor", asciiflow_core::Error::UnsupportedColor { reason, .. }) => {
            assert_eq!(
                color_reason_name(reason),
                expected.color_reason.as_deref().unwrap(),
                "{id}"
            );
        }
        (wanted, actual) => panic!("{id}: expected {wanted}, got {actual:?}"),
    }
}

#[test]
fn versioned_support_contract_matches_core_planner() {
    let contract: Contract = serde_json::from_str(include_str!(
        "../../../tests/support/production-support-v1.json"
    ))
    .expect("support contract must match its strict schema");
    assert_eq!(contract.schema_version, 1);
    assert_eq!(contract.support_contract_version, "1.0.0");
    assert_eq!(
        contract
            .states
            .keys()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>(),
        [
            "ConditionallySupported",
            "Supported",
            "Unqualified",
            "Unsupported"
        ]
        .into_iter()
        .collect()
    );
    assert!(!contract.hardware_scope.gpu.is_empty());
    assert!(!contract.hardware_scope.vulkan.is_empty());
    assert!(!contract.hardware_scope.vaapi.is_empty());
    assert!(!contract.hardware_scope.ffmpeg.is_empty());
    assert!(!contract.hardware_scope.media_device.is_empty());
    assert!(!contract.hardware_scope.limitations.is_empty());
    let limit = &contract.production_limits.hdr_to_sdr;
    assert_eq!(limit.decoded_max_nits, 1000);
    assert_eq!(limit.unit, "cd/m2");
    assert_eq!(limit.applies_to, "explicit_pq_to_sdr_only");
    assert_eq!(
        limit.measurement,
        "per_source_pixel_before_average_or_coverage"
    );
    assert!(!limit.applies_to_pq_preserve);
    assert!(limit.enforced_before_output_pack);
    let mut unique_evidence_ids = std::collections::BTreeSet::new();
    let repository_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let evidence_ids = contract
        .evidence
        .iter()
        .map(|item| {
            assert!(!item.stage.is_empty() && !item.kind.is_empty() && !item.artifact.is_empty());
            assert!(
                unique_evidence_ids.insert(item.id.as_str()),
                "duplicate evidence id {}",
                item.id
            );
            assert!(
                repository_root.join(&item.artifact).is_file(),
                "missing evidence artifact {}",
                item.artifact
            );
            assert!(matches!(
                item.status.as_str(),
                "SEALED" | "QUALIFIED" | "CURRENT"
            ));
            item.id.as_str()
        })
        .collect::<std::collections::BTreeSet<_>>();
    for values in contract.dimensions.values() {
        let mut dimension_values = std::collections::BTreeSet::new();
        for value in values {
            assert!(dimension_values.insert(value.value.as_str()));
            assert!(matches!(
                value.status.as_str(),
                "Supported" | "ConditionallySupported" | "Unsupported" | "Unqualified"
            ));
            assert!(
                value
                    .evidence
                    .iter()
                    .all(|id| evidence_ids.contains(id.as_str()))
            );
            if matches!(
                value.status.as_str(),
                "Supported" | "ConditionallySupported"
            ) {
                assert!(
                    !value.evidence.is_empty(),
                    "qualified dimension value {} has no evidence",
                    value.value
                );
            }
            if value.status == "ConditionallySupported" {
                assert!(
                    value
                        .conditions
                        .as_ref()
                        .is_some_and(|text| !text.trim().is_empty()),
                    "conditional dimension value {} has no conditions",
                    value.value
                );
            }
            if value.status == "Unqualified" {
                assert!(value.evidence.is_empty());
            }
            let _ = &value.conditions;
        }
    }
    let codecs = [
        VideoCodec::H264,
        VideoCodec::Hevc,
        VideoCodec::Av1,
        VideoCodec::Other(String::new()),
    ];
    let codec_values = codecs.iter().map(codec_value).collect::<Vec<_>>();
    assert_enum_dimension(&contract, "input_codec", &codec_values);
    assert_enum_dimension(&contract, "output_codec", &codec_values);
    let profiles = [
        VideoProfile::H264Baseline,
        VideoProfile::H264Main,
        VideoProfile::H264High,
        VideoProfile::HevcMain,
        VideoProfile::HevcMain10,
        VideoProfile::Av1Main,
        VideoProfile::Other(String::new()),
    ];
    let profile_values = profiles.iter().map(profile_value).collect::<Vec<_>>();
    assert_enum_dimension(&contract, "input_profile", &profile_values);
    let subsampling = [
        ChromaSubsampling::Yuv420,
        ChromaSubsampling::Other,
        ChromaSubsampling::Unknown,
    ];
    let subsampling_values = subsampling.map(subsampling_value);
    assert_enum_members_present(&contract, "input_chroma_subsampling", &subsampling_values);
    let chroma_locations = [
        ChromaLocation::Left,
        ChromaLocation::Center,
        ChromaLocation::Unspecified,
    ];
    assert_enum_dimension(
        &contract,
        "chroma_location",
        &chroma_locations.map(chroma_location_value),
    );
    let pixel_formats = [PixelFormat::Nv12, PixelFormat::P010Le];
    let pixel_values = pixel_formats.map(pixel_format_value);
    assert_enum_members_present(&contract, "input_pixel_format", &pixel_values);
    let primaries = [
        ColorPrimaries::Bt709,
        ColorPrimaries::Bt2020,
        ColorPrimaries::Bt470Bg,
        ColorPrimaries::Smpte170M,
        ColorPrimaries::Smpte240M,
        ColorPrimaries::DisplayP3,
        ColorPrimaries::Other,
        ColorPrimaries::Unspecified,
        ColorPrimaries::Unknown(0),
    ];
    assert_enum_dimension(
        &contract,
        "color_primaries",
        &primaries.map(primaries_value),
    );
    let transfers = [
        TransferCharacteristic::Bt709,
        TransferCharacteristic::Smpte170M,
        TransferCharacteristic::Srgb,
        TransferCharacteristic::Gamma22,
        TransferCharacteristic::Gamma28,
        TransferCharacteristic::Linear,
        TransferCharacteristic::Pq,
        TransferCharacteristic::Hlg,
        TransferCharacteristic::Other,
        TransferCharacteristic::Unspecified,
        TransferCharacteristic::Unknown(0),
    ];
    assert_enum_dimension(&contract, "color_transfer", &transfers.map(transfer_value));
    let matrices = [
        ColorMatrix::Bt601,
        ColorMatrix::Bt709,
        ColorMatrix::Bt2020,
        ColorMatrix::Bt2020Constant,
        ColorMatrix::Identity,
        ColorMatrix::Other,
        ColorMatrix::Unspecified,
        ColorMatrix::Unknown(0),
    ];
    assert_enum_dimension(&contract, "color_matrix", &matrices.map(matrix_value));
    let ranges = [
        ColorRange::Limited,
        ColorRange::Full,
        ColorRange::Unspecified,
    ];
    assert_enum_dimension(&contract, "color_range", &ranges.map(range_value));
    assert_enum_dimension(&contract, "input_bit_depth", &["8", "10", "12", "other"]);
    assert_enum_dimension(&contract, "output_bit_depth", &["8", "10", "12", "other"]);
    let dynamic_ranges = [
        asciiflow_core::DynamicRangeClass::Sdr,
        asciiflow_core::DynamicRangeClass::HdrPq,
        asciiflow_core::DynamicRangeClass::HdrHlg,
        asciiflow_core::DynamicRangeClass::Unknown,
        asciiflow_core::DynamicRangeClass::Conflicting,
    ];
    assert_enum_dimension(
        &contract,
        "dynamic_range",
        &dynamic_ranges.map(dynamic_range_value),
    );
    assert_enum_dimension(
        &contract,
        "input_color",
        &[
            "sdr709",
            "sdr601",
            "pq",
            "hlg",
            "full_pq",
            "wide_sdr",
            "unknown",
            "conflicting",
        ],
    );
    let media_requests = [
        MediaRequest::Auto,
        MediaRequest::Software,
        MediaRequest::Hardware,
    ];
    assert_enum_dimension(
        &contract,
        "decode",
        &media_requests.map(media_request_value),
    );
    assert_enum_dimension(
        &contract,
        "encode",
        &media_requests.map(media_request_value),
    );
    let backends = [
        ProcessingBackend::Auto,
        ProcessingBackend::Cpu,
        ProcessingBackend::Vulkan,
    ];
    assert_enum_dimension(&contract, "backend", &backends.map(backend_value));
    let interop_requests = [
        InteropRequest::Auto,
        InteropRequest::Off,
        InteropRequest::On,
    ];
    assert_enum_dimension(
        &contract,
        "input_interop",
        &interop_requests.map(interop_value),
    );
    assert_enum_dimension(
        &contract,
        "output_interop",
        &interop_requests.map(interop_value),
    );
    let output_ranges = [OutputDynamicRange::Preserve, OutputDynamicRange::Sdr];
    assert_enum_dimension(
        &contract,
        "output_dynamic_range",
        &output_ranges.map(output_range_value),
    );
    let audio_policies = [AudioPolicy::Auto, AudioPolicy::Copy, AudioPolicy::None];
    assert_enum_dimension(
        &contract,
        "audio_policy",
        &audio_policies.map(audio_policy_value),
    );

    let mut unique_case_ids = std::collections::BTreeSet::new();
    for case in &contract.cases {
        assert!(
            unique_case_ids.insert(case.id.as_str()),
            "duplicate case id {}",
            case.id
        );
        let input = requirements(case);
        let mut capabilities = qualified_capabilities();
        if let Some(missing) = &case.missing_capability {
            remove_capability(&mut capabilities, missing);
        }
        let planning = PipelinePlanner::select(&capabilities, &input, policy(case));
        match case.expected_planner.result.as_str() {
            "selected" => {
                let plan = planning
                    .unwrap_or_else(|error| panic!("{}: {error}", case.id))
                    .selected;
                assert_eq!(
                    format!("{:?}", plan.backend).to_lowercase(),
                    case.expected_planner.backend.as_deref().unwrap(),
                    "{}",
                    case.id
                );
                assert_eq!(
                    format!("{:?}", plan.decode).to_lowercase(),
                    case.expected_planner.decode.as_deref().unwrap(),
                    "{}",
                    case.id
                );
                assert_eq!(
                    format!("{:?}", plan.encode).to_lowercase(),
                    case.expected_planner.encode.as_deref().unwrap(),
                    "{}",
                    case.id
                );
                let output_format = match plan.output.pixel_format {
                    PixelFormat::Nv12 => "nv12",
                    PixelFormat::P010Le => "p010le",
                };
                assert_eq!(
                    output_format,
                    case.expected_planner
                        .output_pixel_format
                        .as_deref()
                        .unwrap(),
                    "{}",
                    case.id
                );
                assert_eq!(
                    format!("{:?}", plan.color_processing).to_lowercase(),
                    case.expected_planner.color_processing.as_deref().unwrap(),
                    "{}",
                    case.id
                );
            }
            "rejected" => {
                let error = planning.unwrap_err();
                assert_planner_error(error, &case.expected_planner, &case.id);
            }
            other => panic!("{}: unknown planner result {other}", case.id),
        }
        assert!(matches!(
            case.status.as_str(),
            "Supported" | "ConditionallySupported" | "Unsupported" | "Unqualified"
        ));
    }
    assert_eq!(
        contract
            .cases
            .iter()
            .filter(|case| case.id.starts_with("sdr-"))
            .count(),
        5
    );
    assert_eq!(
        contract
            .cases
            .iter()
            .filter(|case| case.id.starts_with("pq-preserve-"))
            .count(),
        2
    );
    assert_eq!(
        contract
            .cases
            .iter()
            .filter(|case| case.id.starts_with("matrix-"))
            .count(),
        10
    );
    assert_eq!(
        contract
            .cases
            .iter()
            .filter(|case| case.missing_capability.is_some())
            .count(),
        2
    );
}
