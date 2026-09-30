//! Sealed Vulkan f32 HDR -> SDR color pipeline and optional qualification captures.
//! Production completion reads only hard-domain diagnostics, never RGB pixels.
use crate::buffer::Buffer;
use crate::context::{VulkanContext, vk_error};
use crate::{DeviceInfo, ExternalPlaneImage, GpuPqCell, VulkanAsciiBackend};
use asciiflow_core::{AsciiConfig, ColorSpace, Error, FrameDesc, Result, VideoFrame};
use asciiflow_font::GlyphAtlas;
use ash::vk;
use bytemuck::{Pod, Zeroable};
use gpu_allocator::{
    MemoryLocation,
    vulkan::{Allocator, AllocatorCreateDesc},
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

const SHADERS: [&[u8]; 3] = [
    include_bytes!(concat!(env!("OUT_DIR"), "/ascii_render_hdr_linear.spv")),
    include_bytes!(concat!(env!("OUT_DIR"), "/tone_map_bt2446.spv")),
    include_bytes!(concat!(env!("OUT_DIR"), "/bt2020_to_bt709_limit.spv")),
];

// Qualification arithmetic selected by the C3B review: shared inverse-NCL
// cancellation in B and compensated difference-form matrix evaluation in C.
// Explicit experiments retain their own selectors; production uses this sealed mode.
const QUALIFICATION_MODE: u32 = 256 | 6;

pub(crate) struct MapBindings {
    pub context: Arc<VulkanContext>,
    pub cells: vk::Buffer,
    pub atlas: vk::Buffer,
    pub atlas_width: u32,
    pub atlas_height: u32,
    pub glyph_count: u32,
    pub grid_width: u32,
    pub grid_height: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum C3Fault {
    LinearRenderPipeline,
    FloatAAllocation,
    FloatBAllocation,
    ToneMapDescriptor,
    BeforeSubmit,
    BeforeC2bDispatch,
    DiagnosticReadback,
    AfterFence,
}
fn checkpoint(fault: Option<C3Fault>, at: C3Fault) -> Result<()> {
    if fault == Some(at) {
        Err(Error::Vulkan(format!("C-3 injected {at:?}")))
    } else {
        Ok(())
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Push {
    width: u32,
    height: u32,
    grid_width: u32,
    grid_height: u32,
    atlas_width: u32,
    atlas_height: u32,
    color: u32,
    glyph_count: u32,
    capture: u32,
    mode: u32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct C3Timings {
    pub hdr_map: Duration,
    pub linear_render: Duration,
    pub method_a: Duration,
    pub target_limit: Duration,
    pub readback: Duration,
    pub backend_wall: Duration,
}

#[derive(Debug)]
pub struct C3Output {
    pub width: u32,
    pub height: u32,
    pub pts: Option<i64>,
    pub cells: Vec<GpuPqCell>,
    pub linear_hdr: Option<Vec<[f32; 3]>>,
    pub nonlinear_2020: Option<Vec<[f32; 3]>>,
    pub display_linear_2020: Option<Vec<[f32; 3]>>,
    /// Actual three high summands per row: full row products, or g/a*dr/b*db.
    /// Compensated-mode residuals are reconstructed separately by its f32 oracle.
    /// Explicit FP64 experiments capture casts of their actual double summands;
    /// this f32 observation buffer cannot establish double intermediate bits.
    pub matrix_terms: Option<Vec<[[f32; 3]; 3]>>,
    pub pre_limit_709: Option<Vec<[f32; 3]>>,
    pub bounded_709: Option<Vec<[f32; 3]>>,
    pub clip_masks: Option<Vec<u32>>,
    /// Original atlas R8 values, not rounded normalized coverage fractions.
    pub coverage: Option<Vec<u8>>,
    pub nonlinear_709: Vec<[f32; 3]>,
    /// negative/>1000/NaN/Inf/invalid-arithmetic, clip0/1/2/3 pixel counts.
    pub diagnostics: [u32; 9],
    pub timings: C3Timings,
}

/// Qualification-only operation-order experiments; never production policy.
#[derive(Clone, Copy, Debug)]
#[repr(u32)]
pub enum C3MatrixExperiment {
    NeutralPrecise = 0,
    RowLeft = 2,
    RowRight = 3,
    RowFma = 4,
    NeutralCompensated = 5,
    NeutralTwoProduct = 6,
}

/// One slot, one private ping-pong pair. `submit_*`/`complete` permit two real
/// in-flight slots via `try_fork`, sharing only the device/serialized queue.
pub struct VulkanHdrToSdrPipeline {
    // Consumer descriptors must disappear before their borrowed producer buffers
    // and backend context, including automatic field destruction.
    sdr_packers: Vec<crate::sdr_pack::SdrPackResources>,
    resources: Option<Resources>,
    backend: VulkanAsciiBackend,
    key: Option<(FrameDesc, AsciiConfig, bool)>,
    fault: Option<C3Fault>,
    last_diagnostics: [u32; 9],
    sdr_ready: bool,
}
impl VulkanHdrToSdrPipeline {
    /// Explicit experimental device, never used by normal qualification/production.
    #[cfg(feature = "hdr-to-sdr-fp64-experiment")]
    pub fn new_fp64_experiment() -> Result<Self> {
        let context = Arc::new(VulkanContext::new_fp64_experiment()?);
        let mut backend = VulkanAsciiBackend::from_context(context)?;
        backend = backend.with_pq_qualification();
        Ok(Self {
            resources: None,
            backend,
            key: None,
            fault: None,
            last_diagnostics: [0; 9],
            sdr_packers: Vec::new(),
            sdr_ready: false,
        })
    }
    pub fn new() -> Result<Self> {
        Ok(Self::from_backend(
            VulkanAsciiBackend::new_pq_qualification()?,
        ))
    }
    /// Retain the already-selected device and atlas; no independent GPU selection.
    pub fn from_backend(backend: VulkanAsciiBackend) -> Self {
        Self {
            resources: None,
            backend,
            key: None,
            fault: None,
            last_diagnostics: [0; 9],
            sdr_packers: Vec::new(),
            sdr_ready: false,
        }
    }
    pub fn with_atlas(mut self, atlas: GlyphAtlas, config: &AsciiConfig) -> Result<Self> {
        if self.resources.is_some() {
            return Err(Error::Vulkan(
                "C-3 atlas must be selected before preparation".into(),
            ));
        }
        self.backend = self.backend.with_atlas(atlas, config)?;
        Ok(self)
    }
    pub fn try_fork(&self) -> Result<Self> {
        if self.backend.shared_context().is_abandoned() {
            return Err(Error::Vulkan("C-3 device abandoned".into()));
        }
        Ok(Self {
            resources: None,
            backend: self.backend.try_fork()?,
            key: None,
            fault: None,
            last_diagnostics: [0; 9],
            sdr_packers: Vec::new(),
            sdr_ready: false,
        })
    }
    pub fn device_info(&self) -> &DeviceInfo {
        self.backend.device_info()
    }
    /// Unknown GPU completion forbids releasing/recycling borrowed surfaces.
    pub fn is_device_abandoned(&self) -> bool {
        self.backend.shared_context().is_abandoned()
    }
    pub fn validation_error_count(&self) -> usize {
        self.backend.validation_error_count()
    }
    /// Observe validation through complete device/context teardown without
    /// retaining the device, its resources or its FDs.
    pub fn validation_observer(&self) -> impl Fn() -> usize + use<> {
        let counter = self.backend.shared_context().validation_counter();
        move || {
            counter
                .as_ref()
                .map_or(0, |value| value.load(std::sync::atomic::Ordering::Relaxed))
        }
    }
    pub fn inject_fault(&mut self, fault: C3Fault) {
        self.fault = Some(fault);
    }
    pub fn last_diagnostics(&self) -> [u32; 9] {
        self.last_diagnostics
    }
    pub fn float_bytes_per_slot(&self) -> u64 {
        self.resources.as_ref().map_or(0, |r| r.bytes * 2)
    }
    pub fn qualification_bytes_per_slot(&self) -> u64 {
        self.resources
            .as_ref()
            .map_or(0, |r| r.buffers.iter().map(|b| b.size).sum())
    }
    /// All explicit buffers, including the reused B-2 map/input resources.
    /// Excludes allocator block padding, driver objects and external surfaces.
    pub fn total_buffer_bytes_per_slot(&self) -> u64 {
        self.qualification_bytes_per_slot() + self.backend.c3_map_buffer_bytes()
    }

    pub fn prepare(&mut self, desc: &FrameDesc, config: &AsciiConfig, capture: bool) -> Result<()> {
        if self.backend.shared_context().is_abandoned() {
            return Err(Error::Vulkan("C-3 device abandoned".into()));
        }
        if self
            .resources
            .as_ref()
            .is_some_and(|r| r.pending || r.staged)
        {
            return Err(Error::Vulkan(
                "C-3 slot is staged or still in flight".into(),
            ));
        }
        crate::pq::validate_pq_desc(desc)?;
        let key = (desc.clone(), config.clone(), capture);
        if self.key.as_ref() == Some(&key) {
            return Ok(());
        }
        // Borrowed map/atlas buffer handles must never outlive their owner.
        // Destroy C-3 descriptors before prepare can replace B-2 resources.
        self.sdr_ready = false;
        self.sdr_packers.clear();
        self.resources.take();
        self.key = None;
        let bindings = self.backend.c3_bindings(desc, config)?;
        let init_fault = self.fault.filter(|f| {
            matches!(
                f,
                C3Fault::LinearRenderPipeline
                    | C3Fault::FloatAAllocation
                    | C3Fault::FloatBAllocation
                    | C3Fault::ToneMapDescriptor
            )
        });
        if init_fault.is_some() {
            self.fault.take();
        }
        self.resources = Some(Resources::new(bindings, desc, config, capture, init_fault)?);
        self.key = Some(key);
        Ok(())
    }

    pub fn submit_host(
        &mut self,
        input: &VideoFrame,
        config: &AsciiConfig,
        capture: bool,
    ) -> Result<()> {
        self.stage_host(input, config, capture)?;
        self.submit_staged()
    }
    /// Host equivalent of stage_external; input copy/map finishes before return.
    pub fn stage_host(
        &mut self,
        input: &VideoFrame,
        config: &AsciiConfig,
        capture: bool,
    ) -> Result<()> {
        let start = Instant::now();
        self.sdr_ready = false;
        self.prepare(input.desc(), config, capture)?;
        let map = self.backend.c3_map_host(input, config)?;
        let r = self.resources.as_mut().expect("prepared");
        r.start = start;
        r.map_time = map;
        r.pts = input.pts();
        r.staged = true;
        Ok(())
    }
    pub fn submit_external(
        &mut self,
        desc: &FrameDesc,
        pts: Option<i64>,
        config: &AsciiConfig,
        planes: [ExternalPlaneImage; 2],
        capture: bool,
    ) -> Result<()> {
        self.stage_external(desc, pts, config, planes, capture)?;
        self.submit_staged()
    }
    /// Finish input copy/map before any slot submits A/B/C. This allows a pair
    /// of independently prepared slots to be submitted without intervening
    /// shared-queue map waits. A staged slot cannot be prepared or overwritten.
    pub fn stage_external(
        &mut self,
        desc: &FrameDesc,
        pts: Option<i64>,
        config: &AsciiConfig,
        planes: [ExternalPlaneImage; 2],
        capture: bool,
    ) -> Result<()> {
        let start = Instant::now();
        self.sdr_ready = false;
        self.prepare(desc, config, capture)?;
        let map = self.backend.c3_map_external(desc, config, planes)?;
        let r = self.resources.as_mut().expect("prepared");
        r.start = start;
        r.map_time = map;
        r.pts = pts;
        r.staged = true;
        Ok(())
    }
    pub fn submit_staged(&mut self) -> Result<()> {
        self.sdr_ready = false;
        let r = self
            .resources
            .as_mut()
            .filter(|r| r.staged)
            .ok_or_else(|| Error::Vulkan("C-3 slot has no staged input".into()))?;
        r.staged = false;
        // A failed record/submit consumes the staged state; retries must restage.
        r.record_submit(0, QUALIFICATION_MODE, self.fault.take())
    }
    pub fn process_host(
        &mut self,
        input: &VideoFrame,
        config: &AsciiConfig,
        capture: bool,
    ) -> Result<C3Output> {
        self.submit_host(input, config, capture)?;
        self.complete()
    }
    /// Direct real dispatch of Pass B/C vectors. Input is not validated by CPU;
    /// invalid values are counted by the shader and cause complete() to fail.
    pub fn process_linear(
        &mut self,
        width: u32,
        height: u32,
        pixels: &[[f32; 3]],
    ) -> Result<C3Output> {
        self.process_vectors(width, height, pixels, 1, QUALIFICATION_MODE)
    }
    pub fn process_c1(&mut self, width: u32, height: u32, pixels: &[[f32; 3]]) -> Result<C3Output> {
        self.process_vectors(width, height, pixels, 2, QUALIFICATION_MODE)
    }
    pub fn process_limiter(
        &mut self,
        width: u32,
        height: u32,
        pixels: &[[f32; 3]],
    ) -> Result<C3Output> {
        self.process_vectors(width, height, pixels, 2, 1)
    }
    pub fn process_arithmetic_experiment(
        &mut self,
        width: u32,
        height: u32,
        pixels: &[[f32; 3]],
        includes_method_a: bool,
        matrix: C3MatrixExperiment,
        cancel_inverse_ncl: bool,
    ) -> Result<C3Output> {
        self.process_vectors(
            width,
            height,
            pixels,
            if includes_method_a { 1 } else { 2 },
            matrix as u32 | if cancel_inverse_ncl { 256 } else { 0 },
        )
    }
    fn process_vectors(
        &mut self,
        width: u32,
        height: u32,
        pixels: &[[f32; 3]],
        first: usize,
        mode: u32,
    ) -> Result<C3Output> {
        self.sdr_ready = false;
        let desc = FrameDesc::host_p010_le(width, height, ColorSpace::pq_bt2020())?;
        if pixels.len() != width as usize * height as usize {
            return Err(Error::Vulkan("C-3 vector dimensions mismatch".into()));
        }
        self.prepare(&desc, &AsciiConfig::default(), true)?;
        let r = self.resources.as_mut().expect("prepared");
        r.start = Instant::now();
        r.map_time = Duration::ZERO;
        r.pts = None;
        r.buffers[if first == 1 { 0 } else { 1 }]
            .write(&r.context.device, bytemuck::cast_slice(pixels))?;
        r.record_submit(first, mode, self.fault.take())?;
        self.complete()
    }
    /// M1: matrix, M2: matrix + target power, M3: entire Pass C in f64.
    #[cfg(feature = "hdr-to-sdr-fp64-experiment")]
    pub fn process_precision_experiment(
        &mut self,
        width: u32,
        height: u32,
        pixels: &[[f32; 3]],
        includes_method_a: bool,
        variant: u32,
    ) -> Result<C3Output> {
        if !(1..=3).contains(&variant) || !self.backend.shared_context().shader_float64_enabled {
            return Err(Error::Vulkan(
                "FP64 experiment requires explicit device and M1/M2/M3".into(),
            ));
        }
        self.process_vectors(
            width,
            height,
            pixels,
            if includes_method_a { 1 } else { 2 },
            256 | (16 + variant),
        )
    }
    /// Consume the completed C-3 signal on-device, never uploading its host
    /// readback. Synchronous packing/copy retains the producer and surface
    /// imports through fence completion. No production backend calls this.
    /// Callers passing external planes must retain their owning surface until
    /// return; on device abandonment they must quarantine it, never recycle it.
    pub fn pack_completed_sdr(
        &mut self,
        format: crate::SdrPackFormat,
        planes: Option<[ExternalPlaneImage; 2]>,
        fault: Option<crate::SdrPackFault>,
    ) -> Result<crate::SdrPackOutput> {
        self.pack_completed_sdr_impl(format, planes, fault, true)
    }

    /// Production consumes resident RGB and copies resident packed codes into
    /// imported output surfaces. Only the diagnostic counters reach the host.
    pub fn pack_completed_sdr_resident(
        &mut self,
        format: crate::SdrPackFormat,
        planes: [ExternalPlaneImage; 2],
    ) -> Result<crate::SdrPackOutput> {
        self.pack_completed_sdr_impl(format, Some(planes), None, false)
    }

    pub fn prepare_sdr_output(&mut self, format: crate::SdrPackFormat) -> Result<()> {
        self.prepare_sdr_output_impl(format, None)
    }

    #[cfg(feature = "hdr-to-sdr-qualification")]
    pub fn prepare_sdr_output_with_fault(
        &mut self,
        format: crate::SdrPackFormat,
        fault: crate::SdrPackFault,
    ) -> Result<()> {
        self.prepare_sdr_output_impl(format, Some(fault))
    }

    fn prepare_sdr_output_impl(
        &mut self,
        format: crate::SdrPackFormat,
        fault: Option<crate::SdrPackFault>,
    ) -> Result<()> {
        let r = self.resources.as_ref().ok_or_else(|| {
            Error::Vulkan("HDR→SDR color pipeline must be prepared before output pack".into())
        })?;
        if !self
            .sdr_packers
            .iter()
            .any(|p| p.matches(r.push.width, r.push.height, format))
        {
            self.sdr_packers
                .push(crate::sdr_pack::SdrPackResources::new(
                    &r.context,
                    r.push.width,
                    r.push.height,
                    format,
                    fault,
                )?);
        }
        Ok(())
    }

    fn pack_completed_sdr_impl(
        &mut self,
        format: crate::SdrPackFormat,
        planes: Option<[ExternalPlaneImage; 2]>,
        fault: Option<crate::SdrPackFault>,
        readback: bool,
    ) -> Result<crate::SdrPackOutput> {
        if !self.sdr_ready {
            return Err(Error::Vulkan(
                "C-4A requires a successfully completed C-3 frame".into(),
            ));
        }
        let r = self.resources.as_ref().expect("completed resources");
        if matches!(
            fault,
            Some(
                crate::SdrPackFault::Pipeline
                    | crate::SdrPackFault::Buffer
                    | crate::SdrPackFault::Descriptor
            )
        ) {
            self.sdr_packers
                .retain(|p| !p.matches(r.push.width, r.push.height, format));
        }
        let index = match self
            .sdr_packers
            .iter()
            .position(|p| p.matches(r.push.width, r.push.height, format))
        {
            Some(index) => index,
            None => {
                let packer = crate::sdr_pack::SdrPackResources::new(
                    &r.context,
                    r.push.width,
                    r.push.height,
                    format,
                    fault,
                )?;
                self.sdr_packers.push(packer);
                self.sdr_packers.len() - 1
            }
        };
        self.sdr_packers[index].process_completed_with_readback(
            r.buffers[0].handle,
            fault,
            planes,
            readback,
        )
    }

    pub fn complete(&mut self) -> Result<C3Output> {
        self.complete_impl(true)
    }

    pub fn complete_resident(&mut self) -> Result<C3Output> {
        self.complete_impl(false)
    }

    fn complete_impl(&mut self, readback: bool) -> Result<C3Output> {
        self.sdr_ready = false;
        let r = self
            .resources
            .as_mut()
            .ok_or_else(|| Error::Vulkan("C-3 slot not prepared".into()))?;
        r.wait()?;
        let read_start = Instant::now();
        self.last_diagnostics = read_pod::<u32>(&r.buffers[3], &r.context.device, 9)?
            .try_into()
            .expect("nine counters");
        checkpoint(r.completion_fault.take(), C3Fault::AfterFence)?;
        if self.last_diagnostics[..5].iter().any(|v| *v != 0) {
            return Err(Error::Vulkan(format!(
                "C-3 invalid GPU input/arithmetic diagnostics: {:?}; HDR→SDR conversion supports the qualified 0–1000 cd/m² PQ domain. No implicit highlight clipping is performed",
                self.last_diagnostics
            )));
        }
        checkpoint(r.readback_fault.take(), C3Fault::DiagnosticReadback)?;
        let final_rgb = if readback {
            read_pod::<[f32; 3]>(&r.buffers[0], &r.context.device, r.pixels)?
        } else {
            Vec::new()
        };
        let mut output = C3Output {
            width: r.push.width,
            height: r.push.height,
            pts: r.pts,
            cells: Vec::new(),
            linear_hdr: None,
            nonlinear_2020: None,
            display_linear_2020: None,
            matrix_terms: None,
            pre_limit_709: None,
            bounded_709: None,
            clip_masks: None,
            coverage: None,
            nonlinear_709: final_rgb,
            diagnostics: self.last_diagnostics,
            timings: C3Timings {
                hdr_map: r.map_time,
                linear_render: r.duration(0)?,
                method_a: r.duration(2)?,
                target_limit: r.duration(4)?,
                ..Default::default()
            },
        };
        if readback && r.push.capture != 0 {
            if r.first <= 1 {
                output.linear_hdr = Some(read_pod::<[f32; 3]>(
                    &r.buffers[4],
                    &r.context.device,
                    r.pixels,
                )?);
            }
            output.nonlinear_2020 = Some(read_pod::<[f32; 3]>(
                &r.buffers[1],
                &r.context.device,
                r.pixels,
            )?);
            let observations = read_pod::<[f32; 20]>(&r.buffers[2], &r.context.device, r.pixels)?;
            output.pre_limit_709 = Some(observations.iter().map(|v| [v[0], v[1], v[2]]).collect());
            output.bounded_709 = Some(observations.iter().map(|v| [v[3], v[4], v[5]]).collect());
            output.clip_masks = Some(observations.iter().map(|v| v[6].to_bits()).collect());
            output.display_linear_2020 =
                Some(observations.iter().map(|v| [v[8], v[9], v[10]]).collect());
            output.matrix_terms = Some(
                observations
                    .iter()
                    .map(|v| {
                        std::array::from_fn(|row| std::array::from_fn(|c| v[11 + row * 3 + c]))
                    })
                    .collect(),
            );
            if r.mapped {
                output.coverage = Some(observations.iter().map(|v| v[7] as u8).collect());
            }
        }
        if readback && r.mapped {
            output.cells = self.backend.pq_cells()?;
        }
        output.timings.readback = read_start.elapsed();
        output.timings.backend_wall = r.start.elapsed();
        self.sdr_ready = true;
        Ok(output)
    }
}

fn read_pod<T: Pod>(buffer: &Buffer, device: &ash::Device, count: usize) -> Result<Vec<T>> {
    let bytes = buffer.read(device, count * std::mem::size_of::<T>())?;
    Ok(bytes
        .chunks_exact(std::mem::size_of::<T>())
        .map(bytemuck::pod_read_unaligned)
        .collect())
}

struct Resources {
    context: Arc<VulkanContext>,
    allocator: Option<Allocator>,
    buffers: Vec<Buffer>,
    bytes: u64,
    pixels: usize,
    push: Push,
    descriptor_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    set: vk::DescriptorSet,
    layout: vk::PipelineLayout,
    pipelines: Vec<vk::Pipeline>,
    pool: vk::CommandPool,
    command: vk::CommandBuffer,
    fence: vk::Fence,
    queries: vk::QueryPool,
    pending: bool,
    staged: bool,
    mapped: bool,
    first: usize,
    completion_fault: Option<C3Fault>,
    readback_fault: Option<C3Fault>,
    start: Instant,
    map_time: Duration,
    pts: Option<i64>,
}
impl Resources {
    fn new(
        b: MapBindings,
        desc: &FrameDesc,
        config: &AsciiConfig,
        capture: bool,
        fault: Option<C3Fault>,
    ) -> Result<Self> {
        let pixels = desc.width as usize * desc.height as usize;
        if pixels > u32::MAX as usize {
            return Err(Error::Vulkan(
                "C-3 pixel count exceeds shader uint indexing".into(),
            ));
        }
        let limits = unsafe {
            b.context
                .instance
                .get_physical_device_properties(b.context.physical_device)
        }
        .limits;
        if desc.width.div_ceil(32) > limits.max_compute_work_group_count[0]
            || desc.height.div_ceil(4) > limits.max_compute_work_group_count[1]
            || (pixels as u32).div_ceil(256) > limits.max_compute_work_group_count[0]
        {
            return Err(Error::Vulkan(
                "C-3 dispatch exceeds workgroup count limits".into(),
            ));
        }
        let bytes = pixels as u64 * 12;
        let observation_bytes = if capture { pixels as u64 * 80 } else { 80 };
        if b.context.info.max_compute_work_group_invocations < 256
            || b.context.info.max_compute_work_group_size[0] < 256
            || b.context.info.max_compute_work_group_size[1] < 4
        {
            return Err(Error::Vulkan("C-3 workgroup limits insufficient".into()));
        }
        if bytes.max(observation_bytes) > b.context.info.max_storage_buffer_range {
            return Err(Error::Vulkan(
                "C-3 buffers exceed maxStorageBufferRange".into(),
            ));
        }
        let allocator = Allocator::new(&AllocatorCreateDesc {
            instance: b.context.instance.clone(),
            device: b.context.device.clone(),
            physical_device: b.context.physical_device,
            debug_settings: Default::default(),
            buffer_device_address: false,
            allocation_sizes: Default::default(),
        })
        .map_err(|e| Error::Vulkan(format!("C-3 allocator: {e}")))?;
        let mut r = Self {
            context: b.context,
            allocator: Some(allocator),
            buffers: Vec::new(),
            bytes,
            pixels,
            push: Push {
                width: desc.width,
                height: desc.height,
                grid_width: b.grid_width,
                grid_height: b.grid_height,
                atlas_width: b.atlas_width,
                atlas_height: b.atlas_height,
                color: u32::from(config.color),
                glyph_count: b.glyph_count,
                capture: u32::from(capture),
                mode: 0,
            },
            descriptor_layout: vk::DescriptorSetLayout::null(),
            descriptor_pool: vk::DescriptorPool::null(),
            set: vk::DescriptorSet::null(),
            layout: vk::PipelineLayout::null(),
            pipelines: Vec::new(),
            pool: vk::CommandPool::null(),
            command: vk::CommandBuffer::null(),
            fence: vk::Fence::null(),
            queries: vk::QueryPool::null(),
            pending: false,
            staged: false,
            mapped: false,
            first: 0,
            completion_fault: None,
            readback_fault: None,
            start: Instant::now(),
            map_time: Duration::ZERO,
            pts: None,
        };
        // The partially initialized Resources is the rollback guard for every
        // allocation/object below. Drop releases only successfully owned handles.
        for (size, name, at) in [
            (bytes, "C3 float A", Some(C3Fault::FloatAAllocation)),
            (bytes, "C3 float B", Some(C3Fault::FloatBAllocation)),
            (observation_bytes, "C3 observations", None),
            (36, "C3 counters", None),
            (if capture { bytes } else { 4 }, "C3 stage-A readback", None),
        ] {
            if let Some(at) = at {
                checkpoint(fault, at)?;
            }
            r.buffers.push(Buffer::new(
                &r.context.device,
                r.allocator.as_mut().expect("allocator"),
                size,
                vk::BufferUsageFlags::STORAGE_BUFFER
                    | vk::BufferUsageFlags::TRANSFER_SRC
                    | vk::BufferUsageFlags::TRANSFER_DST,
                MemoryLocation::GpuToCpu,
                name,
                &r.context.info,
            )?);
        }
        let device = &r.context.device;
        let bindings: Vec<_> = (0..6)
            .map(|binding| {
                vk::DescriptorSetLayoutBinding::default()
                    .binding(binding)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE)
            })
            .collect();
        r.descriptor_layout = unsafe {
            device.create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                None,
            )
        }
        .map_err(vk_error("C-3 descriptor layout"))?;
        r.descriptor_pool = unsafe {
            device.create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::default()
                    .max_sets(1)
                    .pool_sizes(&[vk::DescriptorPoolSize {
                        ty: vk::DescriptorType::STORAGE_BUFFER,
                        descriptor_count: 6,
                    }]),
                None,
            )
        }
        .map_err(vk_error("C-3 descriptor pool"))?;
        checkpoint(fault, C3Fault::ToneMapDescriptor)?;
        r.set = unsafe {
            device.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(r.descriptor_pool)
                    .set_layouts(&[r.descriptor_layout]),
            )
        }
        .map_err(vk_error("C-3 descriptor allocate"))?[0];
        let handles = [
            b.cells,
            b.atlas,
            r.buffers[0].handle,
            r.buffers[1].handle,
            r.buffers[2].handle,
            r.buffers[3].handle,
        ];
        let infos: Vec<_> = handles
            .into_iter()
            .map(|buffer| {
                [vk::DescriptorBufferInfo::default()
                    .buffer(buffer)
                    .range(vk::WHOLE_SIZE)]
            })
            .collect();
        let writes: Vec<_> = infos
            .iter()
            .enumerate()
            .map(|(i, info)| {
                vk::WriteDescriptorSet::default()
                    .dst_set(r.set)
                    .dst_binding(i as u32)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .buffer_info(info)
            })
            .collect();
        unsafe { device.update_descriptor_sets(&writes, &[]) };
        r.layout = unsafe {
            device.create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .set_layouts(&[r.descriptor_layout])
                    .push_constant_ranges(&[vk::PushConstantRange::default()
                        .stage_flags(vk::ShaderStageFlags::COMPUTE)
                        .size(std::mem::size_of::<Push>() as u32)]),
                None,
            )
        }
        .map_err(vk_error("C-3 pipeline layout"))?;
        for (i, shader) in SHADERS.into_iter().enumerate() {
            #[cfg(feature = "hdr-to-sdr-fp64-experiment")]
            let shader: &[u8] = if i == 2 && r.context.shader_float64_enabled {
                include_bytes!(concat!(
                    env!("OUT_DIR"),
                    "/bt2020_to_bt709_fp64_experiment.spv"
                ))
            } else {
                shader
            };
            if i == 0 {
                checkpoint(fault, C3Fault::LinearRenderPipeline)?;
            }
            r.pipelines
                .push(crate::backend::create_pipeline(device, r.layout, shader)?);
        }
        r.pool = unsafe {
            device.create_command_pool(
                &vk::CommandPoolCreateInfo::default()
                    .queue_family_index(r.context.info.queue_family),
                None,
            )
        }
        .map_err(vk_error("C-3 command pool"))?;
        r.command = unsafe {
            device.allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::default()
                    .command_pool(r.pool)
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .command_buffer_count(1),
            )
        }
        .map_err(vk_error("C-3 command buffer"))?[0];
        r.fence = unsafe { device.create_fence(&vk::FenceCreateInfo::default(), None) }
            .map_err(vk_error("C-3 fence"))?;
        r.queries = unsafe {
            device.create_query_pool(
                &vk::QueryPoolCreateInfo::default()
                    .query_type(vk::QueryType::TIMESTAMP)
                    .query_count(6),
                None,
            )
        }
        .map_err(vk_error("C-3 timestamps"))?;
        Ok(r)
    }

    fn barrier(
        &self,
        src: vk::PipelineStageFlags2,
        access: vk::AccessFlags2,
        dst: vk::PipelineStageFlags2,
        dst_access: vk::AccessFlags2,
    ) {
        let barriers = [vk::MemoryBarrier2::default()
            .src_stage_mask(src)
            .src_access_mask(access)
            .dst_stage_mask(dst)
            .dst_access_mask(dst_access)];
        unsafe {
            self.context.device.cmd_pipeline_barrier2(
                self.command,
                &vk::DependencyInfo::default().memory_barriers(&barriers),
            )
        };
    }
    fn record_submit(&mut self, first: usize, mode: u32, fault: Option<C3Fault>) -> Result<()> {
        if self.pending || self.context.is_abandoned() {
            return Err(Error::Vulkan("C-3 slot busy/device abandoned".into()));
        }
        self.push.mode = mode;
        self.mapped = first == 0;
        self.first = first;
        self.completion_fault = fault.filter(|f| *f == C3Fault::AfterFence);
        self.readback_fault = fault.filter(|f| *f == C3Fault::DiagnosticReadback);
        let device = &self.context.device;
        unsafe {
            device
                .reset_command_pool(self.pool, vk::CommandPoolResetFlags::empty())
                .map_err(vk_error("C-3 reset pool"))?;
            device
                .reset_fences(&[self.fence])
                .map_err(vk_error("C-3 reset fence"))?;
            device
                .begin_command_buffer(
                    self.command,
                    &vk::CommandBufferBeginInfo::default()
                        .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
                )
                .map_err(vk_error("C-3 begin"))?;
            device.cmd_reset_query_pool(self.command, self.queries, 0, 6);
            device.cmd_fill_buffer(self.command, self.buffers[3].handle, 0, 36, 0);
            if self.push.capture != 0 {
                device.cmd_fill_buffer(self.command, self.buffers[2].handle, 0, vk::WHOLE_SIZE, 0);
            }
        }
        self.barrier(
            vk::PipelineStageFlags2::ALL_COMMANDS | vk::PipelineStageFlags2::HOST,
            vk::AccessFlags2::MEMORY_WRITE | vk::AccessFlags2::HOST_WRITE,
            vk::PipelineStageFlags2::COMPUTE_SHADER,
            vk::AccessFlags2::SHADER_READ | vk::AccessFlags2::SHADER_WRITE,
        );
        unsafe {
            device.cmd_bind_descriptor_sets(
                self.command,
                vk::PipelineBindPoint::COMPUTE,
                self.layout,
                0,
                &[self.set],
                &[],
            );
            device.cmd_push_constants(
                self.command,
                self.layout,
                vk::ShaderStageFlags::COMPUTE,
                0,
                bytemuck::bytes_of(&self.push),
            );
        }
        for pass in 0..3 {
            unsafe {
                device.cmd_write_timestamp2(
                    self.command,
                    vk::PipelineStageFlags2::ALL_COMMANDS,
                    self.queries,
                    (pass * 2) as u32,
                );
            }
            if pass >= first {
                if pass == 2 {
                    checkpoint(fault, C3Fault::BeforeC2bDispatch)?;
                }
                unsafe {
                    device.cmd_bind_pipeline(
                        self.command,
                        vk::PipelineBindPoint::COMPUTE,
                        self.pipelines[pass],
                    );
                    if pass == 0 {
                        device.cmd_dispatch(
                            self.command,
                            self.push.width.div_ceil(32),
                            self.push.height.div_ceil(4),
                            1,
                        );
                    } else {
                        device.cmd_dispatch(self.command, (self.pixels as u32).div_ceil(256), 1, 1);
                    }
                }
            }
            unsafe {
                device.cmd_write_timestamp2(
                    self.command,
                    vk::PipelineStageFlags2::ALL_COMMANDS,
                    self.queries,
                    (pass * 2 + 1) as u32,
                );
            }
            if pass == 0 && first <= 1 && self.push.capture != 0 {
                self.barrier(
                    vk::PipelineStageFlags2::COMPUTE_SHADER | vk::PipelineStageFlags2::HOST,
                    vk::AccessFlags2::SHADER_WRITE | vk::AccessFlags2::HOST_WRITE,
                    vk::PipelineStageFlags2::COPY,
                    vk::AccessFlags2::TRANSFER_READ,
                );
                unsafe {
                    device.cmd_copy_buffer(
                        self.command,
                        self.buffers[0].handle,
                        self.buffers[4].handle,
                        &[vk::BufferCopy {
                            src_offset: 0,
                            dst_offset: 0,
                            size: self.bytes,
                        }],
                    );
                }
            }
            self.barrier(
                vk::PipelineStageFlags2::ALL_COMMANDS,
                vk::AccessFlags2::MEMORY_READ | vk::AccessFlags2::MEMORY_WRITE,
                vk::PipelineStageFlags2::COMPUTE_SHADER,
                vk::AccessFlags2::SHADER_READ | vk::AccessFlags2::SHADER_WRITE,
            );
        }
        self.barrier(
            vk::PipelineStageFlags2::ALL_COMMANDS,
            vk::AccessFlags2::MEMORY_WRITE,
            vk::PipelineStageFlags2::HOST,
            vk::AccessFlags2::HOST_READ,
        );
        unsafe {
            device
                .end_command_buffer(self.command)
                .map_err(vk_error("C-3 end"))?;
        }
        checkpoint(fault, C3Fault::BeforeSubmit)?;
        let _lock = self
            .context
            .queue_submit_lock
            .lock()
            .map_err(|_| Error::Vulkan("C-3 queue lock poisoned".into()))?;
        let commands = [vk::CommandBufferSubmitInfo::default().command_buffer(self.command)];
        if let Err(e) = unsafe {
            device.queue_submit2(
                self.context.queue,
                &[vk::SubmitInfo2::default().command_buffer_infos(&commands)],
                self.fence,
            )
        } {
            if e == vk::Result::ERROR_DEVICE_LOST {
                self.context.abandon();
            }
            return Err(vk_error("C-3 submit")(e));
        }
        self.pending = true;
        Ok(())
    }
    fn wait(&mut self) -> Result<()> {
        if !self.pending {
            return Err(Error::Vulkan("C-3 has no submitted result".into()));
        }
        if let Err(e) = unsafe {
            self.context
                .device
                .wait_for_fences(&[self.fence], true, 5_000_000_000)
        } {
            // Unknown completion cannot authorize destruction/reuse.
            self.context.abandon();
            return Err(vk_error("C-3 fence completion (device abandoned)")(e));
        }
        self.pending = false;
        Ok(())
    }
    fn duration(&self, first: u32) -> Result<Duration> {
        let mut values = [0u64; 2];
        unsafe {
            self.context.device.get_query_pool_results(
                self.queries,
                first,
                &mut values,
                vk::QueryResultFlags::TYPE_64 | vk::QueryResultFlags::WAIT,
            )
        }
        .map_err(vk_error("C-3 timestamps"))?;
        let bits = self.context.info.timestamp_valid_bits;
        let mask = if bits == 64 {
            u64::MAX
        } else {
            (1u64 << bits) - 1
        };
        Ok(Duration::from_secs_f64(
            (values[1].wrapping_sub(values[0]) & mask) as f64
                * self.context.info.timestamp_period_ns as f64
                / 1e9,
        ))
    }
}
impl Drop for Resources {
    fn drop(&mut self) {
        if self.pending {
            let _ = self.wait();
        }
        if self.context.is_abandoned() {
            // Mirror the existing device-abandonment policy: no unsafe free of
            // potentially in-flight objects, including allocator blocks.
            std::mem::forget(std::mem::take(&mut self.buffers));
            if let Some(a) = self.allocator.take() {
                std::mem::forget(a);
            }
            return;
        }
        let device = &self.context.device;
        unsafe {
            device.destroy_query_pool(self.queries, None);
            device.destroy_fence(self.fence, None);
            device.destroy_command_pool(self.pool, None);
            for pipeline in self.pipelines.drain(..) {
                device.destroy_pipeline(pipeline, None);
            }
            device.destroy_pipeline_layout(self.layout, None);
            device.destroy_descriptor_pool(self.descriptor_pool, None);
            device.destroy_descriptor_set_layout(self.descriptor_layout, None);
        }
        if let Some(allocator) = self.allocator.as_mut() {
            for buffer in &mut self.buffers {
                buffer.destroy(device, allocator);
            }
        }
    }
}
