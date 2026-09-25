use crate::{
    ChromaLocation, ColorMatrix, ColorPrimaries, ColorRange, ColorSpace, Error, PixelFormat,
    ProcessingBackend, Rational, Result, TransferCharacteristic,
};
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CapabilitySupport {
    Supported,
    Unsupported(String),
    NotProbed(String),
}

impl CapabilitySupport {
    pub fn supported() -> Self {
        Self::Supported
    }
    pub fn unsupported(reason: impl Into<String>) -> Self {
        Self::Unsupported(reason.into())
    }
    pub fn not_probed(reason: impl Into<String>) -> Self {
        Self::NotProbed(reason.into())
    }
    pub fn is_supported(&self) -> bool {
        matches!(self, Self::Supported)
    }
    pub fn unavailable_reason(&self) -> Option<&str> {
        match self {
            Self::Supported => None,
            Self::Unsupported(reason) | Self::NotProbed(reason) => Some(reason),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MediaCapabilities {
    pub software_decode: CapabilitySupport,
    pub software_encode: CapabilitySupport,
    pub vaapi_device: CapabilitySupport,
    pub h264_vaapi_decode: CapabilitySupport,
    pub hevc_vaapi_decode: CapabilitySupport,
    pub av1_vaapi_decode: CapabilitySupport,
    pub h264_vaapi_encode: CapabilitySupport,
    pub hevc_vaapi_encode: CapabilitySupport,
    pub hevc_main10_vaapi_decode: CapabilitySupport,
    pub av1_10bit_vaapi_decode: CapabilitySupport,
    pub hevc_main10_vaapi_encode: CapabilitySupport,
    pub av1_vaapi_encode: CapabilitySupport,
    pub av1_10bit_vaapi_encode: CapabilitySupport,
    pub nv12_hardware_frames: CapabilitySupport,
    pub nv12_hardware_upload: CapabilitySupport,
    pub p010_hardware_frames: CapabilitySupport,
    pub p010_hardware_upload: CapabilitySupport,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VulkanDeviceKind {
    DiscreteGpu,
    IntegratedGpu,
    VirtualGpu,
    Cpu,
    Other,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessingCapabilities {
    pub cpu: CapabilitySupport,
    pub vulkan: CapabilitySupport,
    pub vulkan_auto_eligible: bool,
    pub vulkan_device_name: Option<String>,
    pub vulkan_device_kind: Option<VulkanDeviceKind>,
    pub compute_queue: CapabilitySupport,
    pub storage_buffer_8bit: CapabilitySupport,
    pub shader_int64: CapabilitySupport,
    pub synchronization2: CapabilitySupport,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InteropCapabilities {
    /// Qualification of the actual H.264 stream, never a global interop promise.
    pub input: CapabilitySupport,
    pub hevc_input: CapabilitySupport,
    pub av1_input: CapabilitySupport,
    pub output: CapabilitySupport,
    pub hevc_output: CapabilitySupport,
    pub av1_output: CapabilitySupport,
    pub p010_input: CapabilitySupport,
    /// HEVC Main10 encoder-owned P010 output qualification (legacy field name).
    pub p010_output: CapabilitySupport,
    /// AV1 Main 10-bit encoder-owned P010 output qualification.
    pub av1_p010_output: CapabilitySupport,
}

/// Format-level output qualification is independent of codec-specific encoder
/// availability. Production uses profile-specific facts in `InteropCapabilities`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FormatOutputInteropCapabilities {
    pub nv12: CapabilitySupport,
    pub p010: CapabilitySupport,
}

impl FormatOutputInteropCapabilities {
    pub fn for_format(&self, format: PixelFormat) -> &CapabilitySupport {
        match format {
            PixelFormat::Nv12 => &self.nv12,
            PixelFormat::P010Le => &self.p010,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapabilitySnapshot {
    pub media: MediaCapabilities,
    pub processing: ProcessingCapabilities,
    pub interop: InteropCapabilities,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VideoCodec {
    H264,
    Hevc,
    Av1,
    Other(String),
}

impl fmt::Display for VideoCodec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::H264 => f.write_str("H.264"),
            Self::Hevc => f.write_str("HEVC"),
            Self::Av1 => f.write_str("AV1"),
            Self::Other(name) => f.write_str(name),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VideoProfile {
    H264Baseline,
    H264Main,
    H264High,
    HevcMain,
    HevcMain10,
    Av1Main,
    Other(String),
}

impl From<&str> for VideoProfile {
    fn from(name: &str) -> Self {
        match name {
            "High" => Self::H264High,
            "Baseline" | "Constrained Baseline" => Self::H264Baseline,
            _ => Self::Other(name.into()),
        }
    }
}

impl MediaCapabilities {
    pub fn decode_for_input(&self, input: &InputRequirements) -> &CapabilitySupport {
        match (&input.codec, input.bit_depth) {
            (VideoCodec::Hevc, Some(10)) => &self.hevc_main10_vaapi_decode,
            (VideoCodec::Av1, Some(10)) => &self.av1_10bit_vaapi_decode,
            _ => self.decode_for(&input.codec),
        }
    }

    pub fn encode_for_output(&self, output: &OutputVideoRequirements) -> &CapabilitySupport {
        match (&output.codec, output.bit_depth) {
            (VideoCodec::Hevc, 10) => &self.hevc_main10_vaapi_encode,
            (VideoCodec::Av1, 10) => &self.av1_10bit_vaapi_encode,
            _ => self.encode_for(&output.codec),
        }
    }

    pub fn disable_encode_output(&mut self, output: &OutputVideoRequirements, reason: &str) {
        match (&output.codec, output.bit_depth) {
            (VideoCodec::Hevc, 10) => {
                self.hevc_main10_vaapi_encode = CapabilitySupport::unsupported(reason)
            }
            (VideoCodec::Av1, 10) => {
                self.av1_10bit_vaapi_encode = CapabilitySupport::unsupported(reason)
            }
            _ => self.disable_encode(&output.codec, reason),
        }
    }

    pub fn decode_for(&self, codec: &VideoCodec) -> &CapabilitySupport {
        match codec {
            VideoCodec::H264 => &self.h264_vaapi_decode,
            VideoCodec::Hevc => &self.hevc_vaapi_decode,
            VideoCodec::Av1 => &self.av1_vaapi_decode,
            VideoCodec::Other(_) => &UNSUPPORTED_CODEC,
        }
    }
    pub fn disable_decode(&mut self, codec: &VideoCodec, reason: &str) {
        let fact = match codec {
            VideoCodec::H264 => &mut self.h264_vaapi_decode,
            VideoCodec::Hevc => &mut self.hevc_vaapi_decode,
            VideoCodec::Av1 => &mut self.av1_vaapi_decode,
            VideoCodec::Other(_) => return,
        };
        *fact = CapabilitySupport::unsupported(reason);
    }
}
static UNSUPPORTED_CODEC: std::sync::LazyLock<CapabilitySupport> = std::sync::LazyLock::new(|| {
    CapabilitySupport::not_probed("codec is outside the qualified hardware input set")
});
impl InteropCapabilities {
    pub fn input_for_requirements(&self, input: &InputRequirements) -> &CapabilitySupport {
        if input.bit_depth == Some(10) {
            &self.p010_input
        } else {
            self.input_for(&input.codec)
        }
    }

    pub fn output_for_requirements(&self, output: &OutputVideoRequirements) -> &CapabilitySupport {
        if output.pixel_format == PixelFormat::P010Le {
            if output.codec == VideoCodec::Av1 {
                &self.av1_p010_output
            } else {
                &self.p010_output
            }
        } else {
            self.output_for(&output.codec)
        }
    }

    pub fn set_output_for_requirements(
        &mut self,
        output: &OutputVideoRequirements,
        fact: CapabilitySupport,
    ) {
        if output.pixel_format == PixelFormat::P010Le {
            if output.codec == VideoCodec::Av1 {
                self.av1_p010_output = fact;
            } else {
                self.p010_output = fact;
            }
        } else {
            self.set_output(&output.codec, fact);
        }
    }

    pub fn input_for(&self, codec: &VideoCodec) -> &CapabilitySupport {
        match codec {
            VideoCodec::H264 => &self.input,
            VideoCodec::Hevc => &self.hevc_input,
            VideoCodec::Av1 => &self.av1_input,
            VideoCodec::Other(_) => &UNSUPPORTED_CODEC,
        }
    }
    pub fn set_input(&mut self, codec: &VideoCodec, fact: CapabilitySupport) {
        match codec {
            VideoCodec::H264 => self.input = fact,
            VideoCodec::Hevc => self.hevc_input = fact,
            VideoCodec::Av1 => self.av1_input = fact,
            VideoCodec::Other(_) => {}
        }
    }

    pub fn output_for(&self, codec: &VideoCodec) -> &CapabilitySupport {
        match codec {
            VideoCodec::H264 => &self.output,
            VideoCodec::Hevc => &self.hevc_output,
            VideoCodec::Av1 => &self.av1_output,
            VideoCodec::Other(_) => &UNSUPPORTED_CODEC,
        }
    }

    pub fn set_output(&mut self, codec: &VideoCodec, fact: CapabilitySupport) {
        match codec {
            VideoCodec::H264 => self.output = fact,
            VideoCodec::Hevc => self.hevc_output = fact,
            VideoCodec::Av1 => self.av1_output = fact,
            VideoCodec::Other(_) => {}
        }
    }
}

impl MediaCapabilities {
    pub fn encode_for(&self, codec: &VideoCodec) -> &CapabilitySupport {
        match codec {
            VideoCodec::H264 => &self.h264_vaapi_encode,
            VideoCodec::Hevc => &self.hevc_vaapi_encode,
            VideoCodec::Av1 => &self.av1_vaapi_encode,
            VideoCodec::Other(_) => &UNSUPPORTED_CODEC,
        }
    }

    pub fn disable_encode(&mut self, codec: &VideoCodec, reason: &str) {
        let fact = match codec {
            VideoCodec::H264 => &mut self.h264_vaapi_encode,
            VideoCodec::Hevc => &mut self.hevc_vaapi_encode,
            VideoCodec::Av1 => &mut self.av1_vaapi_encode,
            VideoCodec::Other(_) => return,
        };
        *fact = CapabilitySupport::unsupported(reason);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChromaSubsampling {
    Yuv420,
    Other,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InputRequirements {
    pub codec: VideoCodec,
    pub profile: Option<VideoProfile>,
    pub pixel_format: Option<String>,
    pub bit_depth: Option<u8>,
    pub chroma_subsampling: ChromaSubsampling,
    pub width: u32,
    pub height: u32,
    pub frame_rate: Rational,
    pub color_space: ColorSpace,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutputVideoRequirements {
    pub codec: VideoCodec,
    /// Explicitly required profile. H.264 retains the encoder's existing profile selection.
    pub profile: Option<VideoProfile>,
    pub bit_depth: u8,
    pub pixel_format: PixelFormat,
    pub color_space: ColorSpace,
    pub chroma_subsampling: ChromaSubsampling,
    pub width: u32,
    pub height: u32,
    pub frame_rate: Rational,
}

impl OutputVideoRequirements {
    pub fn current_nv12(codec: VideoCodec, input: &InputRequirements) -> Result<Self> {
        let profile = match codec {
            VideoCodec::H264 => None,
            VideoCodec::Hevc => Some(VideoProfile::HevcMain),
            VideoCodec::Av1 => Some(VideoProfile::Av1Main),
            VideoCodec::Other(ref name) => {
                return Err(Error::InvalidConfig(format!(
                    "output codec {name} is not implemented"
                )));
            }
        };
        Ok(Self {
            codec,
            profile,
            bit_depth: 8,
            pixel_format: PixelFormat::Nv12,
            color_space: ColorSpace::default(),
            chroma_subsampling: ChromaSubsampling::Yuv420,
            width: input.width,
            height: input.height,
            frame_rate: input.frame_rate,
        })
    }

    pub fn selected(codec: VideoCodec, bit_depth: u8, input: &InputRequirements) -> Result<Self> {
        if bit_depth == 8 {
            return Self::current_nv12(codec, input);
        }
        if !matches!(codec, VideoCodec::Hevc | VideoCodec::Av1) || bit_depth != 10 {
            return Err(Error::InvalidConfig(format!(
                "{} {}-bit output is not implemented",
                codec, bit_depth
            )));
        }
        if input.color_space.chroma_location != ChromaLocation::Left {
            return Err(Error::UnsupportedFrame(
                "10-bit output currently requires left-sited 4:2:0 chroma".into(),
            ));
        }
        let profile = match codec {
            VideoCodec::Hevc => VideoProfile::HevcMain10,
            VideoCodec::Av1 => VideoProfile::Av1Main,
            _ => unreachable!("validated 10-bit output codec"),
        };
        Ok(Self {
            codec,
            profile: Some(profile),
            bit_depth: 10,
            pixel_format: PixelFormat::P010Le,
            color_space: ColorSpace {
                range: input.color_space.range,
                ..ColorSpace::default()
            },
            chroma_subsampling: ChromaSubsampling::Yuv420,
            width: input.width,
            height: input.height,
            frame_rate: input.frame_rate,
        })
    }
}

impl InputRequirements {
    fn supports_current_hardware_path(&self) -> bool {
        matches!(
            self.codec,
            VideoCodec::H264 | VideoCodec::Hevc | VideoCodec::Av1
        ) && self.bit_depth == Some(8)
            && self.chroma_subsampling == ChromaSubsampling::Yuv420
    }

    pub fn validate_current_pipeline(&self) -> Result<()> {
        if self.bit_depth != Some(8) {
            if self.bit_depth == Some(10) && self.validate_processing_input().is_ok() {
                return Err(Error::UnsupportedFrame(
                    "10-bit P010 input is outside the 8-bit NV12 validation contract; select explicit HEVC Main10 output for production".into(),
                ));
            }
            return Err(Error::UnsupportedFrame(format!(
                "{}-bit video is not supported by the current 8-bit NV12 pipeline",
                self.bit_depth
                    .map_or_else(|| "unknown".into(), |value| value.to_string())
            )));
        }
        if self.chroma_subsampling != ChromaSubsampling::Yuv420 {
            return Err(Error::UnsupportedFrame(format!(
                "the current NV12 pipeline requires 4:2:0 chroma, got {:?}",
                self.chroma_subsampling
            )));
        }
        if (self.codec == VideoCodec::Hevc && self.profile != Some(VideoProfile::HevcMain))
            || (self.codec == VideoCodec::Av1 && self.profile != Some(VideoProfile::Av1Main))
        {
            return Err(Error::UnsupportedFrame(format!(
                "{:?} profile {:?} is outside the Main 8-bit 4:2:0 input contract",
                self.codec, self.profile
            )));
        }
        if self.width == 0
            || self.height == 0
            || self.width % 2 != 0
            || self.height % 2 != 0
            || self.width > i32::MAX as u32
            || self.height > i32::MAX as u32
        {
            return Err(Error::UnsupportedFrame(format!(
                "NV12 requires even dimensions in 2..={}, got {}x{}",
                i32::MAX,
                self.width,
                self.height
            )));
        }
        Ok(())
    }

    /// Validate the input processing contract independently of output encoding.
    /// Production subsequently checks the selected output format without conversion.
    pub fn validate_processing_input(&self) -> Result<PixelFormat> {
        match self.bit_depth {
            Some(8) => {
                self.validate_current_pipeline()?;
                Ok(PixelFormat::Nv12)
            }
            Some(10) => {
                if self.chroma_subsampling != ChromaSubsampling::Yuv420 {
                    return Err(Error::UnsupportedFrame(
                        "10-bit processing requires 4:2:0 chroma".into(),
                    ));
                }
                if !matches!(
                    (&self.codec, &self.profile),
                    (VideoCodec::Hevc, Some(VideoProfile::HevcMain10))
                        | (VideoCodec::Av1, Some(VideoProfile::Av1Main))
                ) {
                    return Err(Error::UnsupportedFrame(format!(
                        "10-bit {:?} profile {:?} is outside the HEVC Main10 / AV1 Main input contract",
                        self.codec, self.profile
                    )));
                }
                if self.color_space.primaries != ColorPrimaries::Bt709
                    || self.color_space.transfer != TransferCharacteristic::Bt709
                    || self.color_space.matrix != ColorMatrix::Bt709
                    || !matches!(
                        self.color_space.range,
                        ColorRange::Limited | ColorRange::Full
                    )
                {
                    return Err(Error::UnsupportedFrame(format!(
                        "10-bit input requires explicitly tagged BT.709 SDR primaries, transfer, matrix and range; got {:?}",
                        self.color_space
                    )));
                }
                if self.width == 0
                    || self.height == 0
                    || self.width % 2 != 0
                    || self.height % 2 != 0
                    || self.width > i32::MAX as u32
                    || self.height > i32::MAX as u32
                {
                    return Err(Error::UnsupportedFrame(format!(
                        "P010LE requires even dimensions in 2..={}, got {}x{}",
                        i32::MAX,
                        self.width,
                        self.height
                    )));
                }
                Ok(PixelFormat::P010Le)
            }
            _ => Err(Error::UnsupportedFrame(format!(
                "{}-bit video is outside the 8-bit NV12 / 10-bit P010LE processing contract",
                self.bit_depth
                    .map_or_else(|| "unknown".into(), |value| value.to_string())
            ))),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MediaRequest {
    #[default]
    Auto,
    Software,
    Hardware,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum InteropRequest {
    #[default]
    Auto,
    Off,
    On,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PipelinePolicy {
    pub backend: ProcessingBackend,
    pub decode: MediaRequest,
    pub encode: MediaRequest,
    pub input_interop: InteropRequest,
    pub output_interop: InteropRequest,
    pub output_codec: VideoCodec,
    pub output_bit_depth: u8,
}

impl Default for PipelinePolicy {
    fn default() -> Self {
        Self {
            backend: ProcessingBackend::Auto,
            decode: MediaRequest::Auto,
            encode: MediaRequest::Auto,
            input_interop: InteropRequest::Auto,
            output_interop: InteropRequest::Auto,
            output_codec: VideoCodec::H264,
            output_bit_depth: 8,
        }
    }
}

impl PipelinePolicy {
    pub fn is_fully_explicit(&self) -> bool {
        self.backend != ProcessingBackend::Auto
            && self.decode != MediaRequest::Auto
            && self.encode != MediaRequest::Auto
            && self.input_interop != InteropRequest::Auto
            && self.output_interop != InteropRequest::Auto
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MediaImplementation {
    Software,
    Hardware,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameDomain {
    HostNv12,
    HardwareNv12,
    VulkanNv12Buffer,
    HostP010,
    HardwareP010,
    VulkanP010Buffer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlanNode {
    SoftwareDecode,
    VaapiDecode,
    HardwareDownload,
    InputHardwareInterop,
    CpuAscii,
    VulkanAscii,
    HostReadback,
    HardwareUpload,
    OutputHardwareInterop,
    SoftwareEncode,
    VaapiEncode,
}

impl fmt::Display for PlanNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::SoftwareDecode => "software decode",
            Self::VaapiDecode => "VAAPI decode",
            Self::HardwareDownload => "hardware download",
            Self::InputHardwareInterop => "VAAPI/Vulkan input interop",
            Self::CpuAscii => "CPU ASCII",
            Self::VulkanAscii => "Vulkan ASCII",
            Self::HostReadback => "Host readback",
            Self::HardwareUpload => "VAAPI hardware upload",
            Self::OutputHardwareInterop => "Vulkan/VAAPI output interop",
            Self::SoftwareEncode => "software encode",
            Self::VaapiEncode => "VAAPI encode",
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlanStep {
    pub node: PlanNode,
    pub input: Option<FrameDomain>,
    pub output: Option<FrameDomain>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PixelPath {
    GpuResident,
    PartiallyStaged,
    Host,
}

impl fmt::Display for PixelPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::GpuResident => "GPU-resident",
            Self::PartiallyStaged => "partially staged through Host memory",
            Self::Host => "Host-resident",
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PipelinePlan {
    pub backend: ProcessingBackend,
    pub decode: MediaImplementation,
    pub encode: MediaImplementation,
    pub output: OutputVideoRequirements,
    pub hardware_download: bool,
    pub hardware_upload: bool,
    pub hardware_input_interop: bool,
    pub hardware_output_interop: bool,
    pub steps: Vec<PlanStep>,
    pub pixel_path: PixelPath,
    pub preference_cost: u16,
    pub reasons: Vec<String>,
    pub buffer_capacity: usize,
}

impl fmt::Display for PipelinePlan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, step) in self.steps.iter().enumerate() {
            if index != 0 {
                f.write_str("\n  -> ")?;
            }
            match step.node {
                PlanNode::SoftwareEncode => write!(f, "software {} encode", self.output.codec)?,
                PlanNode::VaapiEncode => write!(f, "VAAPI {} encode", self.output.codec)?,
                _ => write!(f, "{}", step.node)?,
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CandidateRejection {
    pub candidate: String,
    pub reasons: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlanningResult {
    pub selected: PipelinePlan,
    pub rejected: Vec<CandidateRejection>,
}

pub struct PipelinePlanner;

#[derive(Clone, Copy)]
struct Candidate {
    decode: MediaImplementation,
    backend: ProcessingBackend,
    encode: MediaImplementation,
    input_interop: bool,
    output_interop: bool,
}

impl PipelinePlanner {
    pub fn validate_policy(policy: PipelinePolicy) -> Result<()> {
        validate_policy(&policy)
    }

    pub fn select(
        capabilities: &CapabilitySnapshot,
        requirements: &InputRequirements,
        policy: PipelinePolicy,
    ) -> Result<PlanningResult> {
        validate_policy(&policy)?;
        let input_format = requirements.validate_processing_input()?;
        let output = OutputVideoRequirements::selected(
            policy.output_codec.clone(),
            policy.output_bit_depth,
            requirements,
        )?;
        if input_format != output.pixel_format {
            return Err(Error::UnsupportedFrame(format!(
                "{} input cannot be sent to {}-bit {} output without a conversion path",
                requirements
                    .bit_depth
                    .map_or_else(|| "unknown".into(), |depth| format!("{depth}-bit")),
                output.bit_depth,
                output.codec
            )));
        }
        let mut accepted = Vec::new();
        let mut rejected = Vec::new();
        let mut ordinal = 0_u16;
        for backend in [ProcessingBackend::Vulkan, ProcessingBackend::Cpu] {
            for decode in [MediaImplementation::Hardware, MediaImplementation::Software] {
                for encode in [MediaImplementation::Hardware, MediaImplementation::Software] {
                    for input_interop in [true, false] {
                        for output_interop in [true, false] {
                            let candidate = Candidate {
                                decode,
                                backend,
                                encode,
                                input_interop,
                                output_interop,
                            };
                            if !candidate.is_structurally_valid() {
                                continue;
                            }
                            if !candidate.matches_policy(&policy) {
                                continue;
                            }
                            let reasons = candidate.rejection_reasons(
                                capabilities,
                                requirements,
                                &output,
                                &policy,
                            );
                            if reasons.is_empty() {
                                accepted.push((candidate.preference_cost(), ordinal, candidate));
                            } else {
                                rejected.push(CandidateRejection {
                                    candidate: candidate.label(),
                                    reasons,
                                });
                            }
                            ordinal = ordinal.saturating_add(1);
                        }
                    }
                }
            }
        }
        accepted.sort_by_key(|(cost, ordinal, _)| (*cost, *ordinal));
        let Some((cost, _, selected)) = accepted.into_iter().next() else {
            let details = rejected
                .iter()
                .flat_map(|c| c.reasons.iter())
                .take(4)
                .cloned()
                .collect::<Vec<_>>()
                .join("; ");
            return Err(Error::InvalidConfig(if details.is_empty() {
                "no pipeline matches the requested policy".into()
            } else {
                format!("no legal pipeline matches the requested policy: {details}")
            }));
        };
        Ok(PlanningResult {
            selected: selected.into_plan(cost, capabilities, output),
            rejected,
        })
    }
}

fn validate_policy(policy: &PipelinePolicy) -> Result<()> {
    if policy.output_bit_depth != 8 && policy.output_bit_depth != 10 {
        return Err(Error::InvalidConfig(
            "--output-bit-depth must be 8 or 10".into(),
        ));
    }
    if policy.output_bit_depth == 10
        && !matches!(policy.output_codec, VideoCodec::Hevc | VideoCodec::Av1)
    {
        return Err(Error::InvalidConfig(match policy.output_codec {
            VideoCodec::H264 => "10-bit H.264 output is not implemented; use HEVC or AV1".into(),
            _ => "10-bit output requires --output-codec hevc or av1".into(),
        }));
    }
    if policy.output_codec == VideoCodec::Hevc && policy.encode == MediaRequest::Software {
        return Err(Error::InvalidConfig(
            "HEVC software encoding is not implemented".into(),
        ));
    }
    if policy.output_codec == VideoCodec::Av1 && policy.encode == MediaRequest::Software {
        return Err(Error::InvalidConfig(
            "AV1 software encoding is not implemented".into(),
        ));
    }
    if policy.input_interop == InteropRequest::On && policy.decode == MediaRequest::Software {
        return Err(Error::InvalidConfig(
            "input interop requires a hardware decoder".into(),
        ));
    }
    if policy.input_interop == InteropRequest::On && policy.backend == ProcessingBackend::Cpu {
        return Err(Error::InvalidConfig(
            "input interop requires Vulkan processing".into(),
        ));
    }
    if policy.output_interop == InteropRequest::On && policy.encode == MediaRequest::Software {
        return Err(Error::InvalidConfig(
            "output interop requires a hardware encoder".into(),
        ));
    }
    if policy.output_interop == InteropRequest::On && policy.backend == ProcessingBackend::Cpu {
        return Err(Error::InvalidConfig(
            "output interop requires Vulkan processing".into(),
        ));
    }
    Ok(())
}

impl Candidate {
    fn is_structurally_valid(self) -> bool {
        (!self.input_interop
            || (self.decode == MediaImplementation::Hardware
                && self.backend == ProcessingBackend::Vulkan))
            && (!self.output_interop
                || (self.encode == MediaImplementation::Hardware
                    && self.backend == ProcessingBackend::Vulkan))
    }

    fn matches_policy(self, policy: &PipelinePolicy) -> bool {
        matches_media(policy.decode, self.decode)
            && matches_media(policy.encode, self.encode)
            && matches_backend(policy.backend, self.backend)
            && matches_interop(policy.input_interop, self.input_interop)
            && matches_interop(policy.output_interop, self.output_interop)
    }

    fn rejection_reasons(
        self,
        caps: &CapabilitySnapshot,
        req: &InputRequirements,
        output: &OutputVideoRequirements,
        policy: &PipelinePolicy,
    ) -> Vec<String> {
        let mut reasons = Vec::new();
        if self.decode == MediaImplementation::Hardware
            || self.encode == MediaImplementation::Hardware
        {
            require(&mut reasons, "VAAPI device", &caps.media.vaapi_device);
        }
        if self.decode == MediaImplementation::Hardware {
            require(
                &mut reasons,
                &format!("VAAPI {} decode", req.codec),
                caps.media.decode_for_input(req),
            );
            if !req.supports_current_hardware_path() && req.bit_depth != Some(10) {
                reasons.push(format!(
                    "input {:?} {:?}-bit {:?} is outside the qualified 8-bit 4:2:0 hardware path",
                    req.codec, req.bit_depth, req.chroma_subsampling
                ));
            }
        } else {
            require(&mut reasons, "software decode", &caps.media.software_decode);
        }
        if self.encode == MediaImplementation::Hardware {
            require(
                &mut reasons,
                &format!("{} {}-bit VAAPI encoding", output.codec, output.bit_depth),
                caps.media.encode_for_output(output),
            );
            let (frames, upload, label) = if output.pixel_format == PixelFormat::P010Le {
                (
                    &caps.media.p010_hardware_frames,
                    &caps.media.p010_hardware_upload,
                    "P010",
                )
            } else {
                (
                    &caps.media.nv12_hardware_frames,
                    &caps.media.nv12_hardware_upload,
                    "NV12",
                )
            };
            require(&mut reasons, &format!("{label} hardware frames"), frames);
            if !self.output_interop {
                require(
                    &mut reasons,
                    &format!("Host to VAAPI {label} upload"),
                    upload,
                );
            }
        } else {
            if output.codec != VideoCodec::H264 {
                reasons.push(format!(
                    "{} software encoding is not implemented",
                    output.codec
                ));
            }
            require(
                &mut reasons,
                "software H.264 encode",
                &caps.media.software_encode,
            );
        }
        match self.backend {
            ProcessingBackend::Vulkan => {
                require(&mut reasons, "Vulkan processing", &caps.processing.vulkan);
                require(
                    &mut reasons,
                    "Vulkan compute queue",
                    &caps.processing.compute_queue,
                );
                require(
                    &mut reasons,
                    "storageBuffer8BitAccess",
                    &caps.processing.storage_buffer_8bit,
                );
                require(&mut reasons, "shaderInt64", &caps.processing.shader_int64);
                require(
                    &mut reasons,
                    "Synchronization2",
                    &caps.processing.synchronization2,
                );
                if policy.backend == ProcessingBackend::Auto
                    && !caps.processing.vulkan_auto_eligible
                {
                    reasons.push(
                        "the selected Vulkan device is not eligible for automatic processing"
                            .into(),
                    );
                }
            }
            ProcessingBackend::Cpu => require(&mut reasons, "CPU processing", &caps.processing.cpu),
            ProcessingBackend::Auto => unreachable!(),
        }
        if self.input_interop {
            if self.decode != MediaImplementation::Hardware
                || self.backend != ProcessingBackend::Vulkan
            {
                reasons.push("input interop requires VAAPI decode and Vulkan processing".into());
            }
            require(
                &mut reasons,
                "VAAPI to Vulkan input interop",
                caps.interop.input_for_requirements(req),
            );
        }
        if self.output_interop {
            if self.encode != MediaImplementation::Hardware
                || self.backend != ProcessingBackend::Vulkan
            {
                reasons.push("output interop requires Vulkan processing and VAAPI encode".into());
            }
            require(
                &mut reasons,
                "Vulkan to VAAPI output interop",
                caps.interop.output_for_requirements(output),
            );
        }
        reasons
    }

    fn preference_cost(self) -> u16 {
        // Qualitative tiers derived from Stage 1-3 measurements; these are
        // ordering penalties, not portable millisecond estimates.
        let mut cost = match self.backend {
            ProcessingBackend::Vulkan => 0,
            ProcessingBackend::Cpu => 200,
            ProcessingBackend::Auto => unreachable!(),
        };
        if self.decode == MediaImplementation::Software {
            cost += 20;
        }
        if self.decode == MediaImplementation::Hardware && !self.input_interop {
            cost += 120;
        }
        if self.backend == ProcessingBackend::Vulkan && !self.output_interop {
            cost += 20;
        }
        if self.encode == MediaImplementation::Hardware && !self.output_interop {
            cost += 10;
        }
        if self.encode == MediaImplementation::Software {
            cost += 30;
        }
        cost
    }

    fn label(self) -> String {
        self.steps(PixelFormat::Nv12)
            .iter()
            .map(|s| s.node.to_string())
            .collect::<Vec<_>>()
            .join(" -> ")
    }

    fn into_plan(
        self,
        preference_cost: u16,
        caps: &CapabilitySnapshot,
        output: OutputVideoRequirements,
    ) -> PipelinePlan {
        let hardware_download = self.decode == MediaImplementation::Hardware && !self.input_interop;
        let hardware_upload = self.encode == MediaImplementation::Hardware && !self.output_interop;
        let pixel_path = if self.decode == MediaImplementation::Hardware
            && self.backend == ProcessingBackend::Vulkan
            && self.encode == MediaImplementation::Hardware
            && self.input_interop
            && self.output_interop
        {
            PixelPath::GpuResident
        } else if self.backend == ProcessingBackend::Cpu
            && self.decode == MediaImplementation::Software
            && self.encode == MediaImplementation::Software
        {
            PixelPath::Host
        } else {
            PixelPath::PartiallyStaged
        };
        let mut reasons = Vec::new();
        if self.backend == ProcessingBackend::Vulkan {
            reasons.push(format!(
                "Vulkan processing is supported on {}",
                caps.processing
                    .vulkan_device_name
                    .as_deref()
                    .unwrap_or("the selected GPU")
            ));
        }
        if self.input_interop {
            reasons.push("input interop avoids the expensive hardware download path".into());
        } else if self.decode == MediaImplementation::Software {
            reasons.push(
                "software decode is preferred over staged VAAPI decode without input interop"
                    .into(),
            );
        }
        if self.output_interop {
            reasons.push("output interop avoids Host readback and encoder hwupload".into());
        } else if self.encode == MediaImplementation::Hardware {
            reasons.push("VAAPI encode lowers CPU cost even when hwupload is required".into());
        }
        let steps = self.steps(output.pixel_format);
        PipelinePlan {
            backend: self.backend,
            decode: self.decode,
            encode: self.encode,
            output,
            hardware_download,
            hardware_upload,
            hardware_input_interop: self.input_interop,
            hardware_output_interop: self.output_interop,
            steps,
            pixel_path,
            preference_cost,
            reasons,
            buffer_capacity: 3,
        }
    }

    fn steps(self, format: PixelFormat) -> Vec<PlanStep> {
        let (host_domain, hardware_domain, vulkan_domain) = match format {
            PixelFormat::Nv12 => (
                FrameDomain::HostNv12,
                FrameDomain::HardwareNv12,
                FrameDomain::VulkanNv12Buffer,
            ),
            PixelFormat::P010Le => (
                FrameDomain::HostP010,
                FrameDomain::HardwareP010,
                FrameDomain::VulkanP010Buffer,
            ),
        };
        let mut steps = Vec::new();
        let decode_domain = if self.decode == MediaImplementation::Hardware {
            steps.push(step(PlanNode::VaapiDecode, None, Some(hardware_domain)));
            hardware_domain
        } else {
            steps.push(step(PlanNode::SoftwareDecode, None, Some(host_domain)));
            host_domain
        };
        let processing_input = if self.input_interop {
            steps.push(step(
                PlanNode::InputHardwareInterop,
                Some(decode_domain),
                Some(vulkan_domain),
            ));
            vulkan_domain
        } else if decode_domain == hardware_domain {
            steps.push(step(
                PlanNode::HardwareDownload,
                Some(decode_domain),
                Some(host_domain),
            ));
            host_domain
        } else {
            decode_domain
        };
        let processing_output = if self.backend == ProcessingBackend::Vulkan {
            steps.push(step(
                PlanNode::VulkanAscii,
                Some(processing_input),
                Some(vulkan_domain),
            ));
            vulkan_domain
        } else {
            steps.push(step(
                PlanNode::CpuAscii,
                Some(processing_input),
                Some(host_domain),
            ));
            host_domain
        };
        let encode_input = if self.output_interop {
            steps.push(step(
                PlanNode::OutputHardwareInterop,
                Some(processing_output),
                Some(hardware_domain),
            ));
            hardware_domain
        } else {
            let host = if processing_output == vulkan_domain {
                steps.push(step(
                    PlanNode::HostReadback,
                    Some(processing_output),
                    Some(host_domain),
                ));
                host_domain
            } else {
                processing_output
            };
            if self.encode == MediaImplementation::Hardware {
                steps.push(step(
                    PlanNode::HardwareUpload,
                    Some(host),
                    Some(hardware_domain),
                ));
                hardware_domain
            } else {
                host
            }
        };
        let encode_node = if self.encode == MediaImplementation::Hardware {
            PlanNode::VaapiEncode
        } else {
            PlanNode::SoftwareEncode
        };
        steps.push(step(encode_node, Some(encode_input), None));
        steps
    }
}

fn step(node: PlanNode, input: Option<FrameDomain>, output: Option<FrameDomain>) -> PlanStep {
    PlanStep {
        node,
        input,
        output,
    }
}
fn matches_media(request: MediaRequest, value: MediaImplementation) -> bool {
    match request {
        MediaRequest::Auto => true,
        MediaRequest::Software => value == MediaImplementation::Software,
        MediaRequest::Hardware => value == MediaImplementation::Hardware,
    }
}
fn matches_backend(request: ProcessingBackend, value: ProcessingBackend) -> bool {
    request == ProcessingBackend::Auto || request == value
}
fn matches_interop(request: InteropRequest, value: bool) -> bool {
    match request {
        InteropRequest::Auto => true,
        InteropRequest::Off => !value,
        InteropRequest::On => value,
    }
}
fn require(reasons: &mut Vec<String>, name: &str, support: &CapabilitySupport) {
    if let Some(reason) = support.unavailable_reason() {
        reasons.push(format!("{name} unavailable: {reason}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_interop_support_is_independent_by_pixel_format() {
        let facts = FormatOutputInteropCapabilities {
            nv12: CapabilitySupport::Supported,
            p010: CapabilitySupport::not_probed("writable P010 descriptor not inspected"),
        };
        assert!(facts.for_format(PixelFormat::Nv12).is_supported());
        assert!(matches!(
            facts.for_format(PixelFormat::P010Le),
            CapabilitySupport::NotProbed(_)
        ));
        let unavailable = FormatOutputInteropCapabilities {
            p010: CapabilitySupport::unsupported("R16 transfer-dst unsupported"),
            ..facts
        };
        assert!(unavailable.for_format(PixelFormat::Nv12).is_supported());
        assert!(matches!(
            unavailable.for_format(PixelFormat::P010Le),
            CapabilitySupport::Unsupported(_)
        ));
    }
    fn yes() -> CapabilitySupport {
        CapabilitySupport::Supported
    }
    fn no(name: &str) -> CapabilitySupport {
        CapabilitySupport::unsupported(format!("synthetic {name} removal"))
    }
    fn full() -> CapabilitySnapshot {
        CapabilitySnapshot {
            media: MediaCapabilities {
                software_decode: yes(),
                software_encode: yes(),
                vaapi_device: yes(),
                h264_vaapi_decode: yes(),
                hevc_vaapi_decode: yes(),
                av1_vaapi_decode: yes(),
                h264_vaapi_encode: yes(),
                hevc_vaapi_encode: yes(),
                hevc_main10_vaapi_decode: yes(),
                av1_10bit_vaapi_decode: yes(),
                hevc_main10_vaapi_encode: yes(),
                av1_vaapi_encode: yes(),
                av1_10bit_vaapi_encode: yes(),
                nv12_hardware_frames: yes(),
                nv12_hardware_upload: yes(),
                p010_hardware_frames: yes(),
                p010_hardware_upload: yes(),
            },
            processing: ProcessingCapabilities {
                cpu: yes(),
                vulkan: yes(),
                vulkan_auto_eligible: true,
                vulkan_device_name: Some("synthetic integrated GPU".into()),
                vulkan_device_kind: Some(VulkanDeviceKind::IntegratedGpu),
                compute_queue: yes(),
                storage_buffer_8bit: yes(),
                shader_int64: yes(),
                synchronization2: yes(),
            },
            interop: InteropCapabilities {
                input: yes(),
                hevc_input: yes(),
                av1_input: yes(),
                output: yes(),
                hevc_output: yes(),
                av1_output: yes(),
                p010_input: yes(),
                p010_output: yes(),
                av1_p010_output: yes(),
            },
        }
    }
    fn h264() -> InputRequirements {
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
    fn new_codec_auto_capabilities_are_isolated_and_do_not_prefer_hwdownload() {
        for (codec, profile) in [
            (VideoCodec::Hevc, VideoProfile::HevcMain),
            (VideoCodec::Av1, VideoProfile::Av1Main),
        ] {
            let mut req = h264();
            req.codec = codec.clone();
            req.profile = Some(profile);
            let mut caps = full();
            let plan = PipelinePlanner::select(&caps, &req, Default::default())
                .unwrap()
                .selected;
            assert!(plan.hardware_input_interop);
            caps.interop.set_input(&codec, no("stream import"));
            let plan = PipelinePlanner::select(&caps, &req, Default::default())
                .unwrap()
                .selected;
            assert_eq!(plan.decode, MediaImplementation::Software);
            assert!(caps.interop.input.is_supported());
            caps.media.disable_decode(&codec, "decoder init");
            assert!(caps.media.h264_vaapi_decode.is_supported());
            assert!(!caps.media.decode_for(&codec).is_supported());
            assert!(
                PipelinePlanner::select(
                    &caps,
                    &req,
                    PipelinePolicy {
                        decode: MediaRequest::Hardware,
                        ..Default::default()
                    }
                )
                .is_err()
            );
        }
    }

    #[test]
    fn new_codec_depth_profile_and_chroma_are_planner_errors() {
        for (codec, profile) in [
            (VideoCodec::Hevc, VideoProfile::HevcMain),
            (VideoCodec::Av1, VideoProfile::Av1Main),
        ] {
            let mut req = h264();
            req.codec = codec;
            req.profile = Some(profile);
            req.bit_depth = Some(10);
            assert!(PipelinePlanner::select(&full(), &req, Default::default()).is_err());
            req.bit_depth = Some(8);
            req.chroma_subsampling = ChromaSubsampling::Other;
            assert!(PipelinePlanner::select(&full(), &req, Default::default()).is_err());
            req.chroma_subsampling = ChromaSubsampling::Yuv420;
            req.profile = Some(VideoProfile::Other("unsupported profile".into()));
            assert!(PipelinePlanner::select(&full(), &req, Default::default()).is_err());
        }
    }
    fn automatic(snapshot: &CapabilitySnapshot) -> PipelinePlan {
        PipelinePlanner::select(snapshot, &h264(), PipelinePolicy::default())
            .unwrap()
            .selected
    }
    fn nodes(plan: &PipelinePlan) -> Vec<PlanNode> {
        plan.steps.iter().map(|s| s.node).collect()
    }

    #[test]
    fn full_capability_selects_gpu_resident_pipeline() {
        let plan = automatic(&full());
        assert_eq!(plan.pixel_path, PixelPath::GpuResident);
        assert_eq!(
            nodes(&plan),
            vec![
                PlanNode::VaapiDecode,
                PlanNode::InputHardwareInterop,
                PlanNode::VulkanAscii,
                PlanNode::OutputHardwareInterop,
                PlanNode::VaapiEncode
            ]
        );
    }
    #[test]
    fn no_input_interop_avoids_staged_hardware_decode() {
        let mut c = full();
        c.interop.input = no("input interop");
        let p = automatic(&c);
        assert_eq!(p.decode, MediaImplementation::Software);
        assert!(p.hardware_output_interop);
        assert!(!nodes(&p).contains(&PlanNode::HardwareDownload));
    }
    #[test]
    fn no_output_interop_keeps_input_interop_and_uses_hwupload() {
        let mut c = full();
        c.interop.output = no("output interop");
        let p = automatic(&c);
        assert!(p.hardware_input_interop);
        assert!(p.hardware_upload);
        assert!(nodes(&p).contains(&PlanNode::HostReadback));
    }
    #[test]
    fn no_vaapi_encode_uses_input_interop_and_software_encode() {
        let mut c = full();
        c.media.h264_vaapi_encode = no("VAAPI encode");
        c.media.nv12_hardware_frames = no("encoder frames");
        c.media.nv12_hardware_upload = no("encoder upload");
        c.interop.output = no("output interop");
        let p = automatic(&c);
        assert!(p.hardware_input_interop);
        assert_eq!(p.encode, MediaImplementation::Software);
    }
    #[test]
    fn no_vaapi_uses_software_media_with_vulkan() {
        let mut c = full();
        c.media.vaapi_device = no("device");
        c.media.h264_vaapi_decode = no("decode");
        c.media.h264_vaapi_encode = no("encode");
        c.media.nv12_hardware_frames = no("frames");
        c.interop.input = no("input");
        c.interop.output = no("output");
        let p = automatic(&c);
        assert_eq!(
            (p.decode, p.backend, p.encode),
            (
                MediaImplementation::Software,
                ProcessingBackend::Vulkan,
                MediaImplementation::Software
            )
        );
    }
    #[test]
    fn no_auto_eligible_vulkan_uses_cpu_without_staged_decode() {
        let mut c = full();
        c.processing.vulkan_auto_eligible = false;
        let p = automatic(&c);
        assert_eq!(
            (p.decode, p.backend, p.encode),
            (
                MediaImplementation::Software,
                ProcessingBackend::Cpu,
                MediaImplementation::Hardware
            )
        );
    }
    #[test]
    fn explicit_overrides_are_respected() {
        let c = full();
        let policies = [
            PipelinePolicy {
                decode: MediaRequest::Software,
                ..Default::default()
            },
            PipelinePolicy {
                backend: ProcessingBackend::Cpu,
                ..Default::default()
            },
            PipelinePolicy {
                encode: MediaRequest::Software,
                ..Default::default()
            },
        ];
        for policy in policies {
            let p = PipelinePlanner::select(&c, &h264(), policy.clone())
                .unwrap()
                .selected;
            if policy.decode == MediaRequest::Software {
                assert_eq!(p.decode, MediaImplementation::Software)
            }
            if policy.backend == ProcessingBackend::Cpu {
                assert_eq!(p.backend, ProcessingBackend::Cpu)
            }
            if policy.encode == MediaRequest::Software {
                assert_eq!(p.encode, MediaImplementation::Software)
            }
        }
    }
    #[test]
    fn conflicts_and_unsupported_explicit_requests_are_errors() {
        let c = full();
        let e = PipelinePlanner::select(
            &c,
            &h264(),
            PipelinePolicy {
                encode: MediaRequest::Software,
                output_interop: InteropRequest::On,
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(e.to_string().contains("requires a hardware encoder"));
        let mut u = full();
        u.media.h264_vaapi_decode = no("decode");
        let e = PipelinePlanner::select(
            &u,
            &h264(),
            PipelinePolicy {
                decode: MediaRequest::Hardware,
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(e.to_string().contains("VAAPI H.264 decode unavailable"));
    }
    #[test]
    fn ten_bit_input_is_explicitly_rejected_by_current_pipeline_contract() {
        let p010 =
            crate::FrameDesc::host_p010_le(1920, 1080, crate::ColorSpace::default()).unwrap();
        assert_eq!(p010.format, crate::PixelFormat::P010Le);
        let mut r = h264();
        r.bit_depth = Some(10);
        r.pixel_format = Some("yuv420p10le".into());
        let error = r.validate_current_pipeline().unwrap_err();
        assert!(error.to_string().contains("10-bit video is not supported"));
        for (codec, profile) in [
            (VideoCodec::Hevc, VideoProfile::HevcMain10),
            (VideoCodec::Av1, VideoProfile::Av1Main),
        ] {
            let mut requirements = h264();
            requirements.codec = codec.clone();
            requirements.profile = Some(profile);
            requirements.bit_depth = Some(10);
            let error =
                PipelinePlanner::select(&full(), &requirements, Default::default()).unwrap_err();
            assert!(error.to_string().contains("without a conversion path"));
            requirements.bit_depth = Some(8);
            if codec == VideoCodec::Hevc {
                requirements.profile = Some(VideoProfile::HevcMain);
            }
            let plan = PipelinePlanner::select(
                &full(),
                &requirements,
                PipelinePolicy {
                    output_codec: codec,
                    ..Default::default()
                },
            )
            .unwrap()
            .selected;
            assert_eq!(plan.output.bit_depth, 8);
        }
    }

    #[test]
    fn invalid_nv12_chroma_and_dimensions_are_rejected_early() {
        let mut requirements = h264();
        requirements.chroma_subsampling = ChromaSubsampling::Other;
        assert!(
            requirements
                .validate_current_pipeline()
                .unwrap_err()
                .to_string()
                .contains("requires 4:2:0 chroma")
        );
        requirements.chroma_subsampling = ChromaSubsampling::Yuv420;
        requirements.width = 1919;
        assert!(
            requirements
                .validate_current_pipeline()
                .unwrap_err()
                .to_string()
                .contains("requires even dimensions")
        );
    }
    #[test]
    fn display_and_domains_are_stable() {
        let p = automatic(&full());
        assert_eq!(p.steps[0].output, Some(FrameDomain::HardwareNv12));
        assert_eq!(
            p.to_string(),
            "VAAPI decode\n  -> VAAPI/Vulkan input interop\n  -> Vulkan ASCII\n  -> Vulkan/VAAPI output interop\n  -> VAAPI H.264 encode"
        );
    }

    #[test]
    fn explicit_vulkan_can_use_a_device_excluded_from_auto() {
        let mut c = full();
        c.processing.vulkan_auto_eligible = false;
        c.processing.vulkan_device_kind = Some(VulkanDeviceKind::Cpu);
        let plan = PipelinePlanner::select(
            &c,
            &h264(),
            PipelinePolicy {
                backend: ProcessingBackend::Vulkan,
                ..Default::default()
            },
        )
        .unwrap()
        .selected;
        assert_eq!(plan.backend, ProcessingBackend::Vulkan);
    }

    #[test]
    fn missing_required_vulkan_feature_rejects_explicit_vulkan() {
        let mut c = full();
        c.processing.synchronization2 = no("Synchronization2");
        let error = PipelinePlanner::select(
            &c,
            &h264(),
            PipelinePolicy {
                backend: ProcessingBackend::Vulkan,
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("Synchronization2 unavailable"));
    }

    #[test]
    fn planning_is_deterministic() {
        let capabilities = full();
        let first = PipelinePlanner::select(&capabilities, &h264(), Default::default()).unwrap();
        assert_eq!(first.selected.output.profile, None);
        for _ in 0..20 {
            assert_eq!(
                PipelinePlanner::select(&capabilities, &h264(), Default::default()).unwrap(),
                first
            );
        }
    }

    #[test]
    fn hevc_output_selects_vaapi_and_records_codec_identity() {
        let plan = PipelinePlanner::select(
            &full(),
            &h264(),
            PipelinePolicy {
                output_codec: VideoCodec::Hevc,
                ..Default::default()
            },
        )
        .unwrap()
        .selected;
        assert_eq!(plan.output.codec, VideoCodec::Hevc);
        assert_eq!(plan.output.profile, Some(VideoProfile::HevcMain));
        assert_eq!(plan.encode, MediaImplementation::Hardware);
        assert!(plan.hardware_output_interop);
        assert!(plan.to_string().contains("VAAPI HEVC encode"));
    }

    #[test]
    fn hevc_output_without_interop_keeps_vaapi_encode_and_stages() {
        let mut capabilities = full();
        capabilities.interop.hevc_output = no("HEVC output interop");
        let plan = PipelinePlanner::select(
            &capabilities,
            &h264(),
            PipelinePolicy {
                output_codec: VideoCodec::Hevc,
                ..Default::default()
            },
        )
        .unwrap()
        .selected;
        assert_eq!(plan.output.codec, VideoCodec::Hevc);
        assert!(plan.hardware_upload);
        assert!(!plan.hardware_output_interop);
    }

    #[test]
    fn hevc_output_never_falls_back_to_h264_or_software() {
        let mut capabilities = full();
        capabilities.media.hevc_vaapi_encode = no("HEVC encoder");
        let error = PipelinePlanner::select(
            &capabilities,
            &h264(),
            PipelinePolicy {
                output_codec: VideoCodec::Hevc,
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("HEVC 8-bit VAAPI encoding unavailable")
        );

        let error = PipelinePlanner::select(
            &full(),
            &h264(),
            PipelinePolicy {
                encode: MediaRequest::Software,
                output_codec: VideoCodec::Hevc,
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("HEVC software encoding is not implemented")
        );
    }

    #[test]
    fn output_interop_capability_is_scoped_by_codec() {
        let mut capabilities = full();
        capabilities.interop.hevc_output = no("HEVC-only output import");
        let h264_plan = PipelinePlanner::select(&capabilities, &h264(), Default::default())
            .unwrap()
            .selected;
        assert!(h264_plan.hardware_output_interop);
        let hevc = PipelinePlanner::select(
            &capabilities,
            &h264(),
            PipelinePolicy {
                output_codec: VideoCodec::Hevc,
                ..Default::default()
            },
        )
        .unwrap()
        .selected;
        assert!(!hevc.hardware_output_interop);

        let error = PipelinePlanner::select(
            &capabilities,
            &h264(),
            PipelinePolicy {
                output_codec: VideoCodec::Hevc,
                output_interop: InteropRequest::On,
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("HEVC-only output import"));
    }

    #[test]
    fn av1_output_requires_its_own_encoder_and_never_changes_codec() {
        let policy = PipelinePolicy {
            output_codec: VideoCodec::Av1,
            ..Default::default()
        };
        let plan = PipelinePlanner::select(&full(), &h264(), policy.clone())
            .unwrap()
            .selected;
        assert_eq!(plan.output.codec, VideoCodec::Av1);
        assert_eq!(plan.output.profile, Some(VideoProfile::Av1Main));
        assert_eq!(plan.encode, MediaImplementation::Hardware);
        assert!(plan.hardware_output_interop);
        assert!(plan.to_string().contains("VAAPI AV1 encode"));

        let mut capabilities = full();
        capabilities.media.av1_vaapi_encode = no("AV1 encoder absent");
        let error = PipelinePlanner::select(&capabilities, &h264(), policy.clone()).unwrap_err();
        assert!(error.to_string().contains("AV1 encoder absent"));
        assert!(capabilities.media.h264_vaapi_encode.is_supported());
        assert!(capabilities.media.hevc_vaapi_encode.is_supported());

        let error = PipelinePlanner::select(
            &full(),
            &h264(),
            PipelinePolicy {
                encode: MediaRequest::Software,
                ..policy
            },
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("AV1 software encoding is not implemented")
        );
    }

    #[test]
    fn av1_output_interop_failure_stages_only_av1_when_auto() {
        let mut capabilities = full();
        capabilities.interop.av1_output = no("AV1 import rejected");
        let policy = PipelinePolicy {
            output_codec: VideoCodec::Av1,
            ..Default::default()
        };
        let plan = PipelinePlanner::select(&capabilities, &h264(), policy.clone())
            .unwrap()
            .selected;
        assert!(plan.hardware_upload);
        assert!(!plan.hardware_output_interop);
        assert_eq!(plan.output.codec, VideoCodec::Av1);
        assert!(capabilities.interop.output.is_supported());
        assert!(capabilities.interop.hevc_output.is_supported());

        let error = PipelinePlanner::select(
            &capabilities,
            &h264(),
            PipelinePolicy {
                output_interop: InteropRequest::On,
                ..policy
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("AV1 import rejected"));
    }

    fn main10_input(codec: VideoCodec) -> InputRequirements {
        let mut input = h264();
        input.codec = codec.clone();
        input.profile = Some(match codec {
            VideoCodec::Hevc => VideoProfile::HevcMain10,
            VideoCodec::Av1 => VideoProfile::Av1Main,
            _ => unreachable!(),
        });
        input.pixel_format = Some("yuv420p10le".into());
        input.bit_depth = Some(10);
        input
    }

    #[test]
    fn main10_production_plan_is_explicit_and_format_aware() {
        let policy = PipelinePolicy {
            output_codec: VideoCodec::Hevc,
            output_bit_depth: 10,
            ..Default::default()
        };
        for codec in [VideoCodec::Hevc, VideoCodec::Av1] {
            let plan = PipelinePlanner::select(&full(), &main10_input(codec), policy.clone())
                .unwrap()
                .selected;
            assert_eq!(plan.output.profile, Some(VideoProfile::HevcMain10));
            assert_eq!(plan.output.pixel_format, PixelFormat::P010Le);
            assert_eq!(plan.output.bit_depth, 10);
            assert_eq!(plan.pixel_path, PixelPath::GpuResident);
            assert_eq!(plan.steps[0].output, Some(FrameDomain::HardwareP010));
            assert!(
                plan.steps
                    .iter()
                    .any(|step| step.output == Some(FrameDomain::VulkanP010Buffer))
            );
        }
    }

    #[test]
    fn main10_replan_stays_main10_and_isolates_main8_facts() {
        let mut caps = full();
        let policy = PipelinePolicy {
            output_codec: VideoCodec::Hevc,
            output_bit_depth: 10,
            ..Default::default()
        };
        let input = main10_input(VideoCodec::Hevc);
        let output = PipelinePlanner::select(&caps, &input, policy.clone())
            .unwrap()
            .selected
            .output;
        caps.interop
            .set_output_for_requirements(&output, no("P010 import"));
        let staged = PipelinePlanner::select(&caps, &input, policy.clone())
            .unwrap()
            .selected;
        assert!(staged.hardware_upload);
        assert!(!staged.hardware_output_interop);
        assert_eq!(staged.output.profile, Some(VideoProfile::HevcMain10));
        assert!(caps.interop.hevc_output.is_supported());
        let explicit_error = PipelinePlanner::select(
            &caps,
            &input,
            PipelinePolicy {
                output_interop: InteropRequest::On,
                ..policy.clone()
            },
        )
        .unwrap_err();
        assert!(explicit_error.to_string().contains("P010 import"));
        caps.media
            .disable_encode_output(&output, "Main10 encoder failed");
        let error = PipelinePlanner::select(&caps, &input, policy).unwrap_err();
        assert!(error.to_string().contains("Main10 encoder failed"));
        assert!(caps.media.hevc_vaapi_encode.is_supported());
    }

    #[test]
    fn ten_bit_output_rejects_unsupported_codec_and_conversion() {
        let input = main10_input(VideoCodec::Hevc);
        let error = PipelinePlanner::select(
            &full(),
            &input,
            PipelinePolicy {
                output_codec: VideoCodec::H264,
                output_bit_depth: 10,
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("not implemented"));
        let error = PipelinePlanner::select(
            &full(),
            &h264(),
            PipelinePolicy {
                output_codec: VideoCodec::Hevc,
                output_bit_depth: 10,
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("without a conversion path"));
    }

    #[test]
    fn av1_10bit_output_is_independent_of_av1_8bit_and_hevc_main10() {
        let input = main10_input(VideoCodec::Av1);
        let policy = PipelinePolicy {
            output_codec: VideoCodec::Av1,
            output_bit_depth: 10,
            ..Default::default()
        };
        let mut caps = full();
        let output = PipelinePlanner::select(&caps, &input, policy.clone())
            .unwrap()
            .selected
            .output;
        assert_eq!(output.profile, Some(VideoProfile::Av1Main));
        assert_eq!(output.pixel_format, PixelFormat::P010Le);
        assert_eq!(output.bit_depth, 10);

        caps.interop
            .set_output_for_requirements(&output, no("AV1 P010 import"));
        let staged = PipelinePlanner::select(&caps, &input, policy.clone())
            .unwrap()
            .selected;
        assert!(staged.hardware_upload);
        assert!(!staged.hardware_output_interop);
        assert!(caps.interop.p010_output.is_supported());
        assert!(caps.interop.av1_output.is_supported());
        assert!(
            PipelinePlanner::select(
                &caps,
                &input,
                PipelinePolicy {
                    output_interop: InteropRequest::On,
                    ..policy.clone()
                }
            )
            .unwrap_err()
            .to_string()
            .contains("AV1 P010 import")
        );

        caps.media
            .disable_encode_output(&output, "AV1 10-bit encoder failed");
        assert!(
            PipelinePlanner::select(&caps, &input, policy)
                .unwrap_err()
                .to_string()
                .contains("AV1 10-bit encoder failed")
        );
        assert!(caps.media.av1_vaapi_encode.is_supported());
        assert!(caps.media.hevc_main10_vaapi_encode.is_supported());
    }
}
