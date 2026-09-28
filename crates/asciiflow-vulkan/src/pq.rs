//! Internal PQ qualification entry point, deliberately absent from CLI planning.
use asciiflow_core::{
    ChromaLocation, ColorMatrix, ColorPrimaries, ColorRange, Error, FrameDesc, PixelFormat, Result,
    TransferCharacteristic,
};
use bytemuck::{Pod, Zeroable};

/// std430: one vec4 and one uvec4, 32-byte stride. Linear RGB is normalized
/// to 10,000 cd/m²; the fourth float is the glyph perceptual PQ scalar.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct GpuPqCell {
    pub linear_and_perceptual: [f32; 4],
    /// glyph, input RGB clips, invalid components, output code clips.
    pub counts: [u32; 4],
}

/// One-shot simulated failures, not hardware/device-lost fault injection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PqQualificationFault {
    CellAllocation,
    DescriptorCreation,
    PipelineCreation,
    BeforeSubmit,
    AfterCompletion,
}

pub(crate) fn checkpoint(
    fault: Option<PqQualificationFault>,
    at: PqQualificationFault,
) -> Result<()> {
    if fault == Some(at) {
        let stage = match at {
            PqQualificationFault::CellAllocation
            | PqQualificationFault::DescriptorCreation
            | PqQualificationFault::PipelineCreation => {
                asciiflow_core::PipelineStage::ProcessorInitialization
            }
            _ => asciiflow_core::PipelineStage::ProcessingRuntime,
        };
        return Err(Error::pipeline(
            stage,
            "PQ qualification checkpoint",
            Error::Vulkan(format!("injected {at:?}")),
        ));
    }
    Ok(())
}

pub(crate) fn validate_pq_desc(desc: &FrameDesc) -> Result<()> {
    desc.validate_layout()?;
    let c = desc.color_space;
    if desc.format != PixelFormat::P010Le
        || c.primaries != ColorPrimaries::Bt2020
        || c.matrix != ColorMatrix::Bt2020
        || c.transfer != TransferCharacteristic::Pq
        || c.range != ColorRange::Limited
        || c.chroma_location != ChromaLocation::Left
    {
        return Err(Error::Vulkan(
            "PQ qualification requires left-sited limited BT.2020 NCL/PQ P010LE".into(),
        ));
    }
    Ok(())
}

#[cfg(feature = "hdr-pq-qualification")]
pub struct VulkanPqQualification {
    backend: crate::VulkanAsciiBackend,
}

#[cfg(feature = "hdr-pq-qualification")]
impl VulkanPqQualification {
    pub fn inject_fault(&mut self, fault: PqQualificationFault) {
        self.backend.pq_fault = Some(fault);
    }
    pub fn new() -> Result<Self> {
        Ok(Self {
            backend: crate::VulkanAsciiBackend::new_pq_qualification()?,
        })
    }
    pub fn with_atlas(
        mut self,
        atlas: asciiflow_font::GlyphAtlas,
        config: &asciiflow_core::AsciiConfig,
    ) -> Result<Self> {
        self.backend = self.backend.with_atlas(atlas, config)?;
        Ok(self)
    }
    pub fn try_fork(&self) -> Result<Self> {
        Ok(Self {
            backend: self.backend.try_fork()?,
        })
    }
    pub fn prepare(
        &mut self,
        desc: &FrameDesc,
        config: &asciiflow_core::AsciiConfig,
    ) -> Result<()> {
        self.backend.prepare(desc, config)
    }
    pub fn device_info(&self) -> &crate::DeviceInfo {
        self.backend.device_info()
    }
    pub fn validation_error_count(&self) -> usize {
        self.backend.validation_error_count()
    }
    pub fn map_cells(
        &mut self,
        input: &asciiflow_core::VideoFrame,
        config: &asciiflow_core::AsciiConfig,
    ) -> Result<Vec<GpuPqCell>> {
        self.backend.map_pq_cells(input, config)
    }
    pub fn diagnostics(&mut self) -> Result<Vec<GpuPqCell>> {
        self.backend.pq_cells()
    }
    pub fn process(
        &mut self,
        input: &asciiflow_core::VideoFrame,
        config: &asciiflow_core::AsciiConfig,
    ) -> Result<asciiflow_core::BackendOutput> {
        use asciiflow_core::AsciiBackend;
        self.backend.process(input.clone(), config)
    }
    pub fn process_external_p010(
        &mut self,
        desc: &FrameDesc,
        pts: Option<i64>,
        config: &asciiflow_core::AsciiConfig,
        planes: [crate::ExternalPlaneImage; 2],
    ) -> Result<asciiflow_core::BackendOutput> {
        self.backend
            .process_external_p010(desc, pts, config, planes)
    }
    pub fn read_external_p010(
        &mut self,
        desc: &FrameDesc,
        pts: Option<i64>,
        config: &asciiflow_core::AsciiConfig,
        planes: [crate::ExternalPlaneImage; 2],
    ) -> Result<asciiflow_core::VideoFrame> {
        self.backend.read_external_p010(desc, pts, config, planes)
    }
    pub fn process_external_to_external(
        &mut self,
        desc: &FrameDesc,
        config: &asciiflow_core::AsciiConfig,
        input: [crate::ExternalPlaneImage; 2],
        output: [crate::ExternalPlaneImage; 2],
    ) -> Result<asciiflow_core::BackendTimings> {
        self.backend
            .process_external_to_external(desc, config, input, output)
    }
    /// Feature-gated harness bridge into the existing bounded interop workers.
    /// No production factory constructs this processing mode.
    pub fn into_qualification_backend(self) -> crate::VulkanAsciiBackend {
        self.backend
    }
    pub fn into_pipelined(
        self,
        desc: FrameDesc,
        config: asciiflow_core::AsciiConfig,
    ) -> Result<crate::PipelinedVulkanAsciiBackend> {
        validate_pq_desc(&desc)?;
        crate::PipelinedVulkanAsciiBackend::new(self.backend, desc, config)
    }
}
