//! Decode-and-process planning for internal 10-bit qualification.
//! This deliberately has no encode node and cannot create a production output.

use crate::{
    CapabilitySupport, Error, InputRequirements, InteropRequest, MediaImplementation, MediaRequest,
    PixelFormat, PixelPath, ProcessingBackend, Result, VideoCodec, VideoProfile,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InputProcessingCapabilities {
    pub hevc_main10_vaapi_decode: CapabilitySupport,
    pub av1_main10_vaapi_decode: CapabilitySupport,
    pub hevc_main10_input_interop: CapabilitySupport,
    pub av1_main10_input_interop: CapabilitySupport,
    pub vulkan_p010: CapabilitySupport,
}

impl InputProcessingCapabilities {
    pub fn decode_for(&self, input: &InputRequirements) -> &CapabilitySupport {
        match (&input.codec, &input.profile, input.bit_depth) {
            (VideoCodec::Hevc, Some(VideoProfile::HevcMain10), Some(10)) => {
                &self.hevc_main10_vaapi_decode
            }
            (VideoCodec::Av1, Some(VideoProfile::Av1Main), Some(10)) => {
                &self.av1_main10_vaapi_decode
            }
            _ => &UNQUALIFIED,
        }
    }

    pub fn interop_for(&self, input: &InputRequirements) -> &CapabilitySupport {
        match (&input.codec, &input.profile, input.bit_depth) {
            (VideoCodec::Hevc, Some(VideoProfile::HevcMain10), Some(10)) => {
                &self.hevc_main10_input_interop
            }
            (VideoCodec::Av1, Some(VideoProfile::Av1Main), Some(10)) => {
                &self.av1_main10_input_interop
            }
            _ => &UNQUALIFIED,
        }
    }

    /// Initialization failures change only the capability that actually failed.
    pub fn disable_decode(&mut self, input: &InputRequirements, reason: impl Into<String>) {
        let fact = match (&input.codec, &input.profile, input.bit_depth) {
            (VideoCodec::Hevc, Some(VideoProfile::HevcMain10), Some(10)) => {
                &mut self.hevc_main10_vaapi_decode
            }
            (VideoCodec::Av1, Some(VideoProfile::Av1Main), Some(10)) => {
                &mut self.av1_main10_vaapi_decode
            }
            _ => return,
        };
        *fact = CapabilitySupport::unsupported(reason);
    }

    pub fn disable_interop(&mut self, input: &InputRequirements, reason: impl Into<String>) {
        let fact = match (&input.codec, &input.profile, input.bit_depth) {
            (VideoCodec::Hevc, Some(VideoProfile::HevcMain10), Some(10)) => {
                &mut self.hevc_main10_input_interop
            }
            (VideoCodec::Av1, Some(VideoProfile::Av1Main), Some(10)) => {
                &mut self.av1_main10_input_interop
            }
            _ => return,
        };
        *fact = CapabilitySupport::unsupported(reason);
    }
}

static UNQUALIFIED: std::sync::LazyLock<CapabilitySupport> = std::sync::LazyLock::new(|| {
    CapabilitySupport::not_probed("input is outside HEVC Main10 / AV1 Main 10-bit qualification")
});

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct InputProcessingPolicy {
    pub decode: MediaRequest,
    pub backend: ProcessingBackend,
    pub input_interop: InteropRequest,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputProcessingDomain {
    HostP010,
    HardwareP010,
    VulkanP010Buffer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InputProcessingPlan {
    pub format: PixelFormat,
    pub decode: MediaImplementation,
    pub decoded_domain: InputProcessingDomain,
    pub backend: ProcessingBackend,
    pub processing_domain: InputProcessingDomain,
    pub output_domain: InputProcessingDomain,
    pub input_interop: bool,
    pub hardware_download: bool,
    pub pixel_path: PixelPath,
}

pub struct InputProcessingPlanner;

impl InputProcessingPlanner {
    pub fn select(
        capabilities: &InputProcessingCapabilities,
        input: &InputRequirements,
        policy: InputProcessingPolicy,
    ) -> Result<InputProcessingPlan> {
        if input.validate_processing_input()? != PixelFormat::P010Le {
            return Err(Error::InvalidConfig(
                "input processing qualification requires P010LE".into(),
            ));
        }
        if policy.input_interop == InteropRequest::On
            && (policy.backend == ProcessingBackend::Cpu || policy.decode == MediaRequest::Software)
        {
            return Err(Error::InvalidConfig(
                "explicit input interop requires VAAPI decode and Vulkan processing".into(),
            ));
        }
        let vulkan = capabilities.vulkan_p010.is_supported();
        let decode = capabilities.decode_for(input).is_supported();
        let interop = capabilities.interop_for(input).is_supported();
        if policy.backend == ProcessingBackend::Vulkan && !vulkan {
            return Err(Error::InvalidConfig(format!(
                "P010 Vulkan processing unavailable: {:?}",
                capabilities.vulkan_p010
            )));
        }
        if policy.decode == MediaRequest::Hardware && !decode {
            return Err(Error::InvalidConfig(format!(
                "10-bit VAAPI decode unavailable: {:?}",
                capabilities.decode_for(input)
            )));
        }
        if policy.input_interop == InteropRequest::On && (!decode || !interop || !vulkan) {
            return Err(Error::InvalidConfig(format!(
                "explicit P010 input interop unavailable: {:?}",
                capabilities.interop_for(input)
            )));
        }
        let backend = match policy.backend {
            ProcessingBackend::Auto if vulkan => ProcessingBackend::Vulkan,
            ProcessingBackend::Auto => ProcessingBackend::Cpu,
            explicit => explicit,
        };
        let full_interop = backend == ProcessingBackend::Vulkan
            && policy.decode != MediaRequest::Software
            && policy.input_interop != InteropRequest::Off
            && decode
            && interop;
        let use_hardware = full_interop || policy.decode == MediaRequest::Hardware;
        let decode = if use_hardware {
            MediaImplementation::Hardware
        } else {
            MediaImplementation::Software
        };
        let hardware_download = use_hardware && !full_interop;
        let pixel_path = if full_interop {
            PixelPath::GpuResident
        } else if backend == ProcessingBackend::Vulkan {
            PixelPath::PartiallyStaged
        } else {
            PixelPath::Host
        };
        Ok(InputProcessingPlan {
            format: PixelFormat::P010Le,
            decode,
            decoded_domain: if use_hardware {
                InputProcessingDomain::HardwareP010
            } else {
                InputProcessingDomain::HostP010
            },
            backend,
            processing_domain: if backend == ProcessingBackend::Vulkan {
                InputProcessingDomain::VulkanP010Buffer
            } else {
                InputProcessingDomain::HostP010
            },
            output_domain: InputProcessingDomain::HostP010,
            input_interop: full_interop,
            hardware_download,
            pixel_path,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ChromaSubsampling, ColorSpace, Rational};

    fn input(codec: VideoCodec, profile: VideoProfile) -> InputRequirements {
        InputRequirements {
            codec,
            profile: Some(profile),
            pixel_format: Some("yuv420p10le".into()),
            bit_depth: Some(10),
            chroma_subsampling: ChromaSubsampling::Yuv420,
            width: 1920,
            height: 1080,
            frame_rate: Rational::new(30, 1).unwrap(),
            color_space: ColorSpace::default(),
        }
    }

    fn full() -> InputProcessingCapabilities {
        let supported = CapabilitySupport::supported;
        InputProcessingCapabilities {
            hevc_main10_vaapi_decode: supported(),
            av1_main10_vaapi_decode: supported(),
            hevc_main10_input_interop: supported(),
            av1_main10_input_interop: supported(),
            vulkan_p010: supported(),
        }
    }

    #[test]
    fn auto_selects_gpu_resident_then_software_when_only_p010_interop_fails() {
        let hevc = input(VideoCodec::Hevc, VideoProfile::HevcMain10);
        let av1 = input(VideoCodec::Av1, VideoProfile::Av1Main);
        let mut capabilities = full();
        let selected =
            InputProcessingPlanner::select(&capabilities, &hevc, Default::default()).unwrap();
        assert_eq!(selected.pixel_path, PixelPath::GpuResident);
        assert_eq!(selected.decode, MediaImplementation::Hardware);
        assert_eq!(selected.decoded_domain, InputProcessingDomain::HardwareP010);
        assert_eq!(
            selected.processing_domain,
            InputProcessingDomain::VulkanP010Buffer
        );
        assert_eq!(selected.output_domain, InputProcessingDomain::HostP010);
        capabilities.disable_interop(&hevc, "R16 modifier cannot be imported");
        let replanned =
            InputProcessingPlanner::select(&capabilities, &hevc, Default::default()).unwrap();
        assert_eq!(replanned.decode, MediaImplementation::Software);
        assert_eq!(replanned.decoded_domain, InputProcessingDomain::HostP010);
        assert!(!replanned.hardware_download);
        assert_eq!(replanned.backend, ProcessingBackend::Vulkan);
        assert_eq!(
            InputProcessingPlanner::select(&capabilities, &av1, Default::default())
                .unwrap()
                .pixel_path,
            PixelPath::GpuResident
        );
        assert!(capabilities.hevc_main10_vaapi_decode.is_supported());
    }

    #[test]
    fn explicit_decode_can_stage_but_explicit_interop_never_falls_back() {
        let hevc = input(VideoCodec::Hevc, VideoProfile::HevcMain10);
        let mut capabilities = full();
        capabilities.disable_interop(&hevc, "import failed");
        let staged = InputProcessingPlanner::select(
            &capabilities,
            &hevc,
            InputProcessingPolicy {
                decode: MediaRequest::Hardware,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(staged.hardware_download);
        assert_eq!(staged.pixel_path, PixelPath::PartiallyStaged);
        let strict = InputProcessingPlanner::select(
            &capabilities,
            &hevc,
            InputProcessingPolicy {
                input_interop: InteropRequest::On,
                ..Default::default()
            },
        );
        assert!(
            strict
                .unwrap_err()
                .to_string()
                .contains("explicit P010 input interop")
        );
    }

    #[test]
    fn vulkan_unavailable_uses_cpu_unless_explicit() {
        let av1 = input(VideoCodec::Av1, VideoProfile::Av1Main);
        let mut capabilities = full();
        capabilities.vulkan_p010 = CapabilitySupport::unsupported("16-bit storage unavailable");
        let auto = InputProcessingPlanner::select(&capabilities, &av1, Default::default()).unwrap();
        assert_eq!(auto.backend, ProcessingBackend::Cpu);
        assert_eq!(auto.decode, MediaImplementation::Software);
        assert!(
            InputProcessingPlanner::select(
                &capabilities,
                &av1,
                InputProcessingPolicy {
                    backend: ProcessingBackend::Vulkan,
                    ..Default::default()
                }
            )
            .is_err()
        );
    }

    #[test]
    fn ten_bit_input_requires_known_sdr_color_profile_and_420() {
        let mut hevc = input(VideoCodec::Hevc, VideoProfile::HevcMain10);
        assert_eq!(
            hevc.validate_processing_input().unwrap(),
            PixelFormat::P010Le
        );
        hevc.color_space.transfer = crate::TransferCharacteristic::Pq;
        assert!(
            hevc.validate_processing_input()
                .unwrap_err()
                .to_string()
                .contains("BT.709 SDR")
        );
        hevc.color_space.transfer = crate::TransferCharacteristic::Unspecified;
        assert!(hevc.validate_processing_input().is_err());
        hevc.color_space.transfer = crate::TransferCharacteristic::Bt709;
        hevc.color_space.range = crate::ColorRange::Unspecified;
        assert!(hevc.validate_processing_input().is_err());
        hevc.color_space.range = crate::ColorRange::Limited;
        hevc.chroma_subsampling = ChromaSubsampling::Other;
        assert!(hevc.validate_processing_input().is_err());
        hevc.chroma_subsampling = ChromaSubsampling::Yuv420;
        hevc.bit_depth = Some(12);
        assert!(hevc.validate_processing_input().is_err());
    }
}
