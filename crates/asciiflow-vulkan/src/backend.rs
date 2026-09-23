use crate::{
    buffer::Buffer,
    context::{DeviceInfo, VulkanContext, vk_error},
    external::{
        ExternalImageTimings, ExternalPlaneImage, ExternalPlaneKind, ImportedExternalPlane,
    },
};
use asciiflow_core::{
    AsciiBackend, AsciiConfig, BackendOutput, BackendTimings, Error, FrameDesc, HostFrame,
    PipelineStage, PixelFormat, Result, VideoFrame, glyph_lookup_table,
};
use asciiflow_font::GlyphAtlas;
use ash::{util::read_spv, vk};
use bytemuck::{Pod, Zeroable};
use gpu_allocator::{
    MemoryLocation,
    vulkan::{Allocator, AllocatorCreateDesc},
};
use std::{
    io::Cursor,
    mem::size_of,
    sync::Arc,
    time::{Duration, Instant},
};

const GPU_COMPLETION_TIMEOUT: Duration = Duration::from_secs(5);

fn at_stage<T>(result: Result<T>, stage: PipelineStage, operation: &'static str) -> Result<T> {
    result.map_err(|error| Error::pipeline(stage, operation, error))
}

const MAP_U64_64: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/ascii_map_u64_64.spv"));
const MAP_U32_32: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/ascii_map_u32_32.spv"));
const MAP_U32_64: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/ascii_map_u32_64.spv"));
const MAP_U32_128: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/ascii_map_u32_128.spv"));
const MAP_U32_256: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/ascii_map_u32_256.spv"));
const MAP_P010_U32_32: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/ascii_map_p010_u32_32.spv"));
const MAP_P010_U64_64: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/ascii_map_p010_u64_64.spv"));
const RENDER_INT64_8X8: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/ascii_render_int64_8x8.spv"));
const RENDER_U32_8X8: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/ascii_render_u32_8x8.spv"));
const RENDER_U32_16X8: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/ascii_render_u32_16x8.spv"));
const RENDER_U32_16X16: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/ascii_render_u32_16x16.spv"));
const RENDER_U32_32X4: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/ascii_render_u32_32x4.spv"));
const RENDER_LUT_8X8: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/ascii_render_lut_8x8.spv"));
const RENDER_LUT_16X8: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/ascii_render_lut_16x8.spv"));
const RENDER_LUT_16X16: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/ascii_render_lut_16x16.spv"));
const RENDER_LUT_32X4: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/ascii_render_lut_32x4.spv"));
const RENDER_P010_LUT_32X4: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/ascii_render_p010_lut_32x4.spv"));

const QUERY_UPLOAD_BEGIN: u32 = 0;
const QUERY_MAPPING_BEGIN: u32 = 2;
const QUERY_RENDER_BEGIN: u32 = 4;
const QUERY_DOWNLOAD_BEGIN: u32 = 6;
const QUERY_COUNT: u32 = 8;

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Pod, Zeroable)]
pub struct GpuAsciiCell {
    pub glyph: u32,
    pub y: u32,
    pub u: u32,
    pub v: u32,
}

#[derive(Clone, Debug)]
pub struct MemoryAllocationInfo {
    pub name: &'static str,
    pub memory_type_index: u32,
    pub heap_index: u32,
    pub property_flags: vk::MemoryPropertyFlags,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct PushConstants {
    width: u32,
    height: u32,
    grid_width: u32,
    grid_height: u32,
    atlas_width: u32,
    atlas_height: u32,
    color: u32,
    glyph_count: u32,
    neutral_chroma: u32,
}

#[derive(Clone, Copy, Debug, Default)]
struct SubmitTimings {
    queue_submit: Duration,
    gpu_wait: Duration,
}

#[derive(Clone, Copy, Debug, Default)]
struct UploadTimings {
    host_upload: Duration,
    gpu_upload: Duration,
    submit: SubmitTimings,
}

#[derive(Clone, Copy, Debug, Default)]
struct ComputeTimings {
    gpu_mapping: Duration,
    gpu_render: Duration,
    submit: SubmitTimings,
}

#[derive(Clone, Copy, Debug, Default)]
struct ExternalCopyTimings {
    gpu_copy: Duration,
    ownership_acquire_record: Duration,
    ownership_release_record: Duration,
    submit: SubmitTimings,
}

struct DownloadResult {
    bytes: Vec<u8>,
    gpu_download: Duration,
    gpu_busy: Duration,
    host_readback: Duration,
    host_invalidate: Duration,
    submit: SubmitTimings,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ResourceKey {
    width: u32,
    height: u32,
    format: PixelFormat,
    grid_width: u32,
    grid_height: u32,
    font: String,
    charset: String,
}

fn mapping_u32_is_safe(
    width: u32,
    height: u32,
    grid_width: u32,
    grid_height: u32,
    frame_bytes: u64,
    max_sample: u64,
) -> bool {
    let max_cell_width = (width as u64).div_ceil(grid_width as u64);
    let max_cell_height = (height as u64).div_ceil(grid_height as u64);
    let max_cell_pixels = max_cell_width * max_cell_height;
    max_cell_pixels
        .checked_mul(max_sample)
        .is_some_and(|sum| sum <= u32::MAX as u64)
        && grid_width as u64 * width as u64 <= u32::MAX as u64
        && grid_height as u64 * height as u64 <= u32::MAX as u64
        && frame_bytes <= u32::MAX as u64
        && grid_width as u64 * grid_height as u64 <= u32::MAX as u64
}

fn max_sample_for(format: PixelFormat) -> u32 {
    match format {
        PixelFormat::Nv12 => 255,
        PixelFormat::P010Le => 1023,
    }
}

pub struct VulkanAsciiBackend {
    supplied_atlas: Option<(GlyphAtlas, String, String)>,
    context: Arc<VulkanContext>,
    allocator: Option<Allocator>,
    resources: Option<Resources>,
}

impl VulkanAsciiBackend {
    pub fn new() -> Result<Self> {
        Self::from_context(Arc::new(VulkanContext::new()?))
    }

    pub(crate) fn from_context(context: Arc<VulkanContext>) -> Result<Self> {
        tracing::info!(gpu=%context.info.name,vendor_id=format_args!("{:#06x}",context.info.vendor_id),device_id=format_args!("{:#06x}",context.info.device_id),api=%context.info.api_version_string(),driver=context.info.driver_version,"selected Vulkan compute device");
        let allocator = Allocator::new(&AllocatorCreateDesc {
            instance: context.instance.clone(),
            device: context.device.clone(),
            physical_device: context.physical_device,
            debug_settings: Default::default(),
            buffer_device_address: false,
            allocation_sizes: Default::default(),
        })
        .map_err(|e| Error::Vulkan(format!("failed to create Vulkan memory allocator: {e}")))?;
        Ok(Self {
            supplied_atlas: None,
            context,
            allocator: Some(allocator),
            resources: None,
        })
    }
    pub(crate) fn shared_context(&self) -> Arc<VulkanContext> {
        self.context.clone()
    }
    pub fn try_fork(&self) -> Result<Self> {
        let mut fork = Self::from_context(self.context.clone())?;
        fork.supplied_atlas = self.supplied_atlas.clone();
        Ok(fork)
    }
    /// Supply initialization-owned pixels before allocating frame resources.
    pub fn with_atlas(mut self, atlas: GlyphAtlas, config: &AsciiConfig) -> Result<Self> {
        if self.resources.is_some() {
            return Err(Error::Vulkan(
                "atlas must be supplied before prepare".into(),
            ));
        }
        if atlas.glyph_count() != config.charset.chars().count() {
            return Err(Error::Vulkan(
                "atlas glyph count does not match the ramp".into(),
            ));
        }
        self.supplied_atlas = Some((atlas, config.font.clone(), config.charset.clone()));
        Ok(self)
    }
    pub fn device_info(&self) -> &DeviceInfo {
        &self.context.info
    }
    pub fn validation_error_count(&self) -> usize {
        self.context.validation_error_count()
    }
    pub fn memory_allocations(&self) -> Vec<MemoryAllocationInfo> {
        self.resources.as_ref().map_or_else(Vec::new, |resources| {
            [
                Some(&resources.upload),
                resources.readback.as_ref(),
                Some(&resources.input),
                Some(&resources.output),
            ]
            .into_iter()
            .flatten()
            .map(|buffer| MemoryAllocationInfo {
                name: buffer.memory_info.name,
                memory_type_index: buffer.memory_info.memory_type_index,
                heap_index: buffer.memory_info.heap_index,
                property_flags: buffer.memory_info.property_flags,
            })
            .collect()
        })
    }
    pub fn readback_mode(&self) -> Option<&'static str> {
        self.resources.as_ref().map(|resources| {
            if resources.direct_output {
                "direct"
            } else if resources.readback.as_ref().is_some_and(|buffer| {
                buffer
                    .memory_info
                    .property_flags
                    .contains(vk::MemoryPropertyFlags::HOST_CACHED)
            }) {
                "cached"
            } else {
                "allocator"
            }
        })
    }
    pub fn mapping_variant(&self) -> Option<(&'static str, u32)> {
        self.resources
            .as_ref()
            .map(|resources| (resources.map_variant, resources.map_workgroup_size))
    }
    pub fn allocated_buffer_bytes(&self) -> Option<u64> {
        self.resources
            .as_ref()
            .map(Resources::allocated_buffer_bytes)
    }

    pub fn prepare(&mut self, desc: &FrameDesc, config: &AsciiConfig) -> Result<()> {
        self.ensure_resources(desc, config)
    }

    fn ensure_resources(&mut self, desc: &FrameDesc, config: &AsciiConfig) -> Result<()> {
        desc.validate_layout()?;
        if desc.format == PixelFormat::P010Le && !self.context.info.p010_storage_supported {
            return Err(Error::Vulkan(format!(
                "P010LE Vulkan processing requires storageBuffer16BitAccess on {}",
                self.context.info.name
            )));
        }
        if let Some((_, font, charset)) = &self.supplied_atlas
            && (font != &config.font || charset != &config.charset)
        {
            return Err(Error::Vulkan(
                "supplied atlas font/ramp identity changed".into(),
            ));
        }
        let (grid_width, grid_height) = config.resolved_grid(desc.width, desc.height)?;
        let key = ResourceKey {
            width: desc.width,
            height: desc.height,
            format: desc.format,
            grid_width,
            grid_height,
            font: config.font.clone(),
            charset: config.charset.clone(),
        };
        if self
            .resources
            .as_ref()
            .is_some_and(|resources| resources.key == key)
        {
            return Ok(());
        }
        if let Some(mut resources) = self.resources.take() {
            let _queue_guard = self
                .context
                .queue_submit_lock
                .lock()
                .map_err(|_| Error::Vulkan("Vulkan queue lock was poisoned".into()))?;
            resources.destroy(
                &self.context.device,
                self.allocator.as_mut().expect("allocator missing"),
            );
        }
        let atlas = match &self.supplied_atlas {
            Some((atlas, _, _)) => atlas.clone(),
            None => GlyphAtlas::builtin(&config.font, &config.charset)
                .map_err(|e| Error::Vulkan(e.to_string()))?,
        };
        if atlas.glyph_count() != config.charset.chars().count() {
            return Err(Error::Vulkan(
                "atlas glyph count does not match the ramp".into(),
            ));
        }
        self.resources = Some(Resources::new(
            &self.context,
            self.allocator.as_mut().expect("allocator missing"),
            key,
            atlas,
        )?);
        Ok(())
    }

    pub fn map_cells(
        &mut self,
        input: &VideoFrame,
        config: &AsciiConfig,
    ) -> Result<Vec<GpuAsciiCell>> {
        self.ensure_resources(input.desc(), config)?;
        let resources = self.resources.as_mut().expect("resources initialized");
        resources.upload_input(&self.context, input.host().as_slice())?;
        resources.run_compute(&self.context, config.color, true, false, false)?;
        resources.download_cells(&self.context)
    }

    pub fn render_cells(
        &mut self,
        desc: &FrameDesc,
        pts: Option<i64>,
        config: &AsciiConfig,
        cells: &[GpuAsciiCell],
    ) -> Result<BackendOutput> {
        let total_started = Instant::now();
        self.ensure_resources(desc, config)?;
        let resources = self.resources.as_mut().expect("resources initialized");
        let expected = resources.key.grid_width as usize * resources.key.grid_height as usize;
        if cells.len() != expected {
            return Err(Error::Vulkan(format!(
                "mapped cell count is {}; expected {expected}",
                cells.len()
            )));
        }
        if cells.iter().any(|cell| {
            cell.glyph >= resources.atlas.glyph_count() as u32
                || cell.y > max_sample_for(desc.format)
                || cell.u > max_sample_for(desc.format)
                || cell.v > max_sample_for(desc.format)
        }) {
            return Err(Error::Vulkan("mapped cell value is out of range".into()));
        }
        let cell_bytes = bytemuck::cast_slice(cells);
        let upload = resources.upload_buffer(&self.context, cell_bytes, resources.cells.handle)?;
        let compute = resources.run_compute(&self.context, config.color, false, true, true)?;
        let download = resources.download_output(&self.context, desc.byte_len())?;
        let frame = VideoFrame::new_host(
            desc.clone(),
            pts,
            HostFrame::from_bytes(desc, download.bytes)?,
        )?;
        Ok(BackendOutput {
            frame,
            timings: BackendTimings {
                host_upload: upload.host_upload,
                queue_submit: upload.submit.queue_submit
                    + compute.submit.queue_submit
                    + download.submit.queue_submit,
                gpu_upload: upload.gpu_upload,
                gpu_render: compute.gpu_render,
                gpu_download: download.gpu_download,
                gpu_busy: download.gpu_busy,
                gpu_wait: upload.submit.gpu_wait
                    + compute.submit.gpu_wait
                    + download.submit.gpu_wait,
                host_invalidate: download.host_invalidate,
                host_readback: download.host_readback,
                backend_wall: total_started.elapsed(),
                ..BackendTimings::default()
            },
        })
    }

    pub fn process_external_nv12(
        &mut self,
        desc: &FrameDesc,
        pts: Option<i64>,
        config: &AsciiConfig,
        planes: [ExternalPlaneImage; 2],
    ) -> Result<BackendOutput> {
        self.process_external_input(desc, pts, config, planes, PixelFormat::Nv12)
    }

    pub fn process_external_p010(
        &mut self,
        desc: &FrameDesc,
        pts: Option<i64>,
        config: &AsciiConfig,
        planes: [ExternalPlaneImage; 2],
    ) -> Result<BackendOutput> {
        self.process_external_input(desc, pts, config, planes, PixelFormat::P010Le)
    }

    fn process_external_input(
        &mut self,
        desc: &FrameDesc,
        pts: Option<i64>,
        config: &AsciiConfig,
        planes: [ExternalPlaneImage; 2],
        expected_format: PixelFormat,
    ) -> Result<BackendOutput> {
        validate_external_planes(
            desc,
            &planes,
            crate::ExternalImageAccess::Read,
            "input",
            expected_format,
        )?;
        let total_started = Instant::now();
        self.ensure_resources(desc, config)?;
        let mut import_timings = ExternalImageTimings::default();
        let [y, uv] = planes;
        let (y, y_timings) = at_stage(
            ImportedExternalPlane::import(&self.context, y),
            PipelineStage::InputInteropRuntime,
            "import input Y DMA-BUF",
        )?;
        add_external_timings(&mut import_timings, y_timings);
        let (uv, uv_timings) = at_stage(
            ImportedExternalPlane::import(&self.context, uv),
            PipelineStage::InputInteropRuntime,
            "import input UV DMA-BUF",
        )?;
        add_external_timings(&mut import_timings, uv_timings);
        let resources = self.resources.as_mut().expect("resources initialized");
        let copied = at_stage(
            resources.copy_external_input(&self.context, [&y, &uv]),
            PipelineStage::InputInteropRuntime,
            "copy imported input image",
        )?;
        let compute = at_stage(
            resources.run_compute(&self.context, config.color, true, true, true),
            PipelineStage::ProcessingRuntime,
            "execute Vulkan ASCII compute",
        )?;
        let download = at_stage(
            resources.download_output(&self.context, desc.byte_len()),
            PipelineStage::ProcessingRuntime,
            "read Vulkan output",
        )?;
        import_timings.destroy += uv.destroy();
        import_timings.destroy += y.destroy();
        let frame = VideoFrame::new_host(
            desc.clone(),
            pts,
            HostFrame::from_bytes(desc, download.bytes)?,
        )?;
        Ok(BackendOutput {
            frame,
            timings: BackendTimings {
                queue_submit: copied.submit.queue_submit
                    + compute.submit.queue_submit
                    + download.submit.queue_submit,
                drm_prime_map: Duration::ZERO,
                external_capability_query: import_timings.capability_query,
                external_image_create: import_timings.image_create,
                external_memory_import: import_timings.dma_buf_import,
                external_memory_bind: import_timings.memory_bind,
                external_ownership: copied.ownership_acquire_record
                    + copied.ownership_release_record,
                external_image_destroy: import_timings.destroy,
                gpu_external_copy: copied.gpu_copy,
                gpu_mapping: compute.gpu_mapping,
                gpu_render: compute.gpu_render,
                gpu_download: download.gpu_download,
                gpu_busy: download.gpu_busy,
                gpu_wait: copied.submit.gpu_wait
                    + compute.submit.gpu_wait
                    + download.submit.gpu_wait,
                host_invalidate: download.host_invalidate,
                host_readback: download.host_readback,
                backend_wall: total_started.elapsed(),
                ..BackendTimings::default()
            },
        })
    }

    pub fn read_external_nv12(
        &mut self,
        desc: &FrameDesc,
        pts: Option<i64>,
        config: &AsciiConfig,
        planes: [ExternalPlaneImage; 2],
    ) -> Result<VideoFrame> {
        self.read_external_input(desc, pts, config, planes, PixelFormat::Nv12)
    }

    pub fn read_external_p010(
        &mut self,
        desc: &FrameDesc,
        pts: Option<i64>,
        config: &AsciiConfig,
        planes: [ExternalPlaneImage; 2],
    ) -> Result<VideoFrame> {
        self.read_external_input(desc, pts, config, planes, PixelFormat::P010Le)
    }

    fn read_external_input(
        &mut self,
        desc: &FrameDesc,
        pts: Option<i64>,
        config: &AsciiConfig,
        planes: [ExternalPlaneImage; 2],
        expected_format: PixelFormat,
    ) -> Result<VideoFrame> {
        validate_external_planes(
            desc,
            &planes,
            crate::ExternalImageAccess::Read,
            "input",
            expected_format,
        )?;
        self.ensure_resources(desc, config)?;
        let [y, uv] = planes;
        let (y, _) = ImportedExternalPlane::import(&self.context, y)?;
        let (uv, _) = ImportedExternalPlane::import(&self.context, uv)?;
        let resources = self.resources.as_mut().expect("resources initialized");
        resources.copy_external_input(&self.context, [&y, &uv])?;
        let bytes = resources.download_input(&self.context, desc.byte_len())?;
        uv.destroy();
        y.destroy();
        VideoFrame::new_host(desc.clone(), pts, HostFrame::from_bytes(desc, bytes)?)
    }

    pub fn probe_external_nv12(
        &mut self,
        desc: &FrameDesc,
        planes: [ExternalPlaneImage; 2],
        access: crate::ExternalImageAccess,
    ) -> Result<ExternalImageTimings> {
        validate_external_planes(desc, &planes, access, "probe", PixelFormat::Nv12)?;
        let mut total = ExternalImageTimings::default();
        let [y, uv] = planes;
        let (y, timings) = ImportedExternalPlane::import(&self.context, y)?;
        add_external_timings(&mut total, timings);
        let (uv, timings) = ImportedExternalPlane::import(&self.context, uv)?;
        add_external_timings(&mut total, timings);
        total.destroy += uv.destroy();
        total.destroy += y.destroy();
        Ok(total)
    }

    pub fn process_external_nv12_to_external(
        &mut self,
        desc: &FrameDesc,
        config: &AsciiConfig,
        input_planes: [ExternalPlaneImage; 2],
        output_planes: [ExternalPlaneImage; 2],
    ) -> Result<BackendTimings> {
        let total_started = Instant::now();
        validate_external_planes(
            desc,
            &input_planes,
            crate::ExternalImageAccess::Read,
            "input",
            PixelFormat::Nv12,
        )?;
        validate_external_planes(
            desc,
            &output_planes,
            crate::ExternalImageAccess::Write,
            "output",
            PixelFormat::Nv12,
        )?;
        self.ensure_resources(desc, config)?;

        let mut input_import = ExternalImageTimings::default();
        let [input_y, input_uv] = input_planes;
        let (input_y, timings) = at_stage(
            ImportedExternalPlane::import(&self.context, input_y),
            PipelineStage::InputInteropRuntime,
            "import input Y DMA-BUF",
        )?;
        add_external_timings(&mut input_import, timings);
        let (input_uv, timings) = at_stage(
            ImportedExternalPlane::import(&self.context, input_uv),
            PipelineStage::InputInteropRuntime,
            "import input UV DMA-BUF",
        )?;
        add_external_timings(&mut input_import, timings);

        let mut output_import = ExternalImageTimings::default();
        let [output_y, output_uv] = output_planes;
        let (output_y, timings) = at_stage(
            ImportedExternalPlane::import(&self.context, output_y),
            PipelineStage::OutputInteropRuntime,
            "import output Y DMA-BUF as TRANSFER_DST",
        )?;
        add_external_timings(&mut output_import, timings);
        let (output_uv, timings) = at_stage(
            ImportedExternalPlane::import(&self.context, output_uv),
            PipelineStage::OutputInteropRuntime,
            "import output UV DMA-BUF as TRANSFER_DST",
        )?;
        add_external_timings(&mut output_import, timings);

        let resources = self.resources.as_mut().expect("resources initialized");
        let input = at_stage(
            resources.copy_external_input(&self.context, [&input_y, &input_uv]),
            PipelineStage::InputInteropRuntime,
            "copy imported input image",
        )?;
        let compute = at_stage(
            resources.run_compute(&self.context, config.color, true, true, false),
            PipelineStage::ProcessingRuntime,
            "execute Vulkan ASCII compute",
        )?;
        let output = at_stage(
            resources.copy_output_to_external(&self.context, [&output_y, &output_uv]),
            PipelineStage::OutputInteropRuntime,
            "copy Vulkan output to encoder DMA-BUF",
        )?;

        output_import.destroy += output_uv.destroy();
        output_import.destroy += output_y.destroy();
        input_import.destroy += input_uv.destroy();
        input_import.destroy += input_y.destroy();

        Ok(BackendTimings {
            queue_submit: input.submit.queue_submit
                + compute.submit.queue_submit
                + output.submit.queue_submit,
            external_capability_query: input_import.capability_query,
            external_image_create: input_import.image_create,
            external_memory_import: input_import.dma_buf_import,
            external_memory_bind: input_import.memory_bind,
            external_ownership: input.ownership_acquire_record + input.ownership_release_record,
            external_image_destroy: input_import.destroy,
            gpu_external_copy: input.gpu_copy,
            gpu_mapping: compute.gpu_mapping,
            gpu_render: compute.gpu_render,
            output_external_capability_query: output_import.capability_query,
            output_external_image_create: output_import.image_create,
            output_external_memory_import: output_import.dma_buf_import,
            output_external_memory_bind: output_import.memory_bind,
            output_ownership_acquire: output.ownership_acquire_record,
            output_ownership_release: output.ownership_release_record,
            output_external_image_destroy: output_import.destroy,
            gpu_external_output_copy: output.gpu_copy,
            output_queue_submit: output.submit.queue_submit,
            output_gpu_wait: output.submit.gpu_wait,
            gpu_busy: resources.timestamp_span(
                &self.context,
                QUERY_UPLOAD_BEGIN,
                QUERY_DOWNLOAD_BEGIN + 1,
            )?,
            gpu_wait: input.submit.gpu_wait + compute.submit.gpu_wait + output.submit.gpu_wait,
            backend_wall: total_started.elapsed(),
            ..BackendTimings::default()
        })
    }

    pub fn process_nv12_to_external(
        &mut self,
        input: VideoFrame,
        config: &AsciiConfig,
        output_planes: [ExternalPlaneImage; 2],
    ) -> Result<BackendTimings> {
        let total_started = Instant::now();
        let desc = input.desc().clone();
        validate_external_planes(
            &desc,
            &output_planes,
            crate::ExternalImageAccess::Write,
            "output",
            PixelFormat::Nv12,
        )?;
        self.ensure_resources(&desc, config)?;
        let mut output_import = ExternalImageTimings::default();
        let [output_y, output_uv] = output_planes;
        let (output_y, timings) = at_stage(
            ImportedExternalPlane::import(&self.context, output_y),
            PipelineStage::OutputInteropRuntime,
            "import output Y DMA-BUF as TRANSFER_DST",
        )?;
        add_external_timings(&mut output_import, timings);
        let (output_uv, timings) = at_stage(
            ImportedExternalPlane::import(&self.context, output_uv),
            PipelineStage::OutputInteropRuntime,
            "import output UV DMA-BUF as TRANSFER_DST",
        )?;
        add_external_timings(&mut output_import, timings);

        let resources = self.resources.as_mut().expect("resources initialized");
        let upload = at_stage(
            resources.upload_input(&self.context, input.host().as_slice()),
            PipelineStage::ProcessingRuntime,
            "upload Host NV12 to Vulkan",
        )?;
        let compute = at_stage(
            resources.run_compute(&self.context, config.color, true, true, false),
            PipelineStage::ProcessingRuntime,
            "execute Vulkan ASCII compute",
        )?;
        let output = at_stage(
            resources.copy_output_to_external(&self.context, [&output_y, &output_uv]),
            PipelineStage::OutputInteropRuntime,
            "copy Vulkan output to encoder DMA-BUF",
        )?;
        output_import.destroy += output_uv.destroy();
        output_import.destroy += output_y.destroy();
        Ok(BackendTimings {
            host_upload: upload.host_upload,
            queue_submit: upload.submit.queue_submit
                + compute.submit.queue_submit
                + output.submit.queue_submit,
            gpu_upload: upload.gpu_upload,
            gpu_mapping: compute.gpu_mapping,
            gpu_render: compute.gpu_render,
            output_external_capability_query: output_import.capability_query,
            output_external_image_create: output_import.image_create,
            output_external_memory_import: output_import.dma_buf_import,
            output_external_memory_bind: output_import.memory_bind,
            output_ownership_acquire: output.ownership_acquire_record,
            output_ownership_release: output.ownership_release_record,
            output_external_image_destroy: output_import.destroy,
            gpu_external_output_copy: output.gpu_copy,
            output_queue_submit: output.submit.queue_submit,
            output_gpu_wait: output.submit.gpu_wait,
            gpu_busy: resources.timestamp_span(
                &self.context,
                QUERY_UPLOAD_BEGIN,
                QUERY_DOWNLOAD_BEGIN + 1,
            )?,
            gpu_wait: upload.submit.gpu_wait + compute.submit.gpu_wait + output.submit.gpu_wait,
            backend_wall: total_started.elapsed(),
            ..BackendTimings::default()
        })
    }
}

fn validate_external_planes(
    desc: &FrameDesc,
    planes: &[ExternalPlaneImage; 2],
    access: crate::ExternalImageAccess,
    label: &str,
    expected_format: PixelFormat,
) -> Result<()> {
    if desc.format != expected_format {
        return Err(Error::Vulkan(format!(
            "external-image interop expected {expected_format:?}, got {:?}",
            desc.format
        )));
    }
    let (y_kind, uv_kind) = match expected_format {
        PixelFormat::Nv12 => (ExternalPlaneKind::Y, ExternalPlaneKind::Uv),
        PixelFormat::P010Le => (ExternalPlaneKind::P010Y, ExternalPlaneKind::P010Uv),
    };
    if planes[0].kind != y_kind
        || planes[1].kind != uv_kind
        || planes.iter().any(|plane| plane.access != access)
    {
        return Err(Error::Vulkan(format!(
            "external {expected_format:?} {label} must contain Y then UV with {access:?} access"
        )));
    }
    let row_bytes = u64::from(desc.width) * expected_format.bytes_per_sample() as u64;
    let expected = [
        (desc.width, desc.height, row_bytes),
        (desc.width / 2, desc.height / 2, row_bytes),
    ];
    for (plane, (width, height, row_bytes)) in planes.iter().zip(expected) {
        if plane.width != width || plane.height != height {
            return Err(Error::Vulkan(format!(
                "external {expected_format:?} {label} {:?} plane is {}x{}; expected {width}x{height}",
                plane.kind, plane.width, plane.height
            )));
        }
        if plane.row_pitch < row_bytes {
            return Err(Error::Vulkan(format!(
                "external {expected_format:?} {label} {:?} plane row pitch {} is smaller than {row_bytes} bytes",
                plane.kind, plane.row_pitch
            )));
        }
        let last_row = u64::from(height - 1)
            .checked_mul(plane.row_pitch)
            .and_then(|bytes| plane.offset.checked_add(bytes))
            .and_then(|offset| offset.checked_add(row_bytes))
            .ok_or_else(|| {
                Error::Vulkan(format!(
                    "external {expected_format:?} {label} {:?} plane range overflows u64",
                    plane.kind
                ))
            })?;
        if last_row > plane.object_size {
            return Err(Error::Vulkan(format!(
                "external {expected_format:?} {label} {:?} plane ends at byte {last_row}, beyond object size {}",
                plane.kind, plane.object_size
            )));
        }
    }
    if planes[0].modifier != planes[1].modifier {
        return Err(Error::Vulkan(format!(
            "external {expected_format:?} {label} Y and UV planes must use the same DRM modifier"
        )));
    }
    Ok(())
}

fn add_external_timings(total: &mut ExternalImageTimings, value: ExternalImageTimings) {
    total.capability_query += value.capability_query;
    total.image_create += value.image_create;
    total.dma_buf_import += value.dma_buf_import;
    total.memory_bind += value.memory_bind;
    total.destroy += value.destroy;
}

impl AsciiBackend for VulkanAsciiBackend {
    fn process(&mut self, input: VideoFrame, config: &AsciiConfig) -> Result<BackendOutput> {
        let total_started = Instant::now();
        let desc = input.desc().clone();
        let pts = input.pts();
        self.ensure_resources(&desc, config)?;
        let resources = self.resources.as_mut().expect("resources initialized");
        let upload = resources.upload_input(&self.context, input.host().as_slice())?;
        let compute = resources.run_compute(&self.context, config.color, true, true, true)?;
        let download = resources.download_output(&self.context, desc.byte_len())?;
        let frame = VideoFrame::new_host(
            desc.clone(),
            pts,
            HostFrame::from_bytes(&desc, download.bytes)?,
        )?;
        Ok(BackendOutput {
            frame,
            timings: BackendTimings {
                host_upload: upload.host_upload,
                queue_submit: upload.submit.queue_submit
                    + compute.submit.queue_submit
                    + download.submit.queue_submit,
                gpu_upload: upload.gpu_upload,
                gpu_mapping: compute.gpu_mapping,
                gpu_render: compute.gpu_render,
                gpu_download: download.gpu_download,
                gpu_busy: download.gpu_busy,
                gpu_wait: upload.submit.gpu_wait
                    + compute.submit.gpu_wait
                    + download.submit.gpu_wait,
                host_invalidate: download.host_invalidate,
                host_readback: download.host_readback,
                backend_wall: total_started.elapsed(),
                ..BackendTimings::default()
            },
        })
    }
}

impl Drop for VulkanAsciiBackend {
    fn drop(&mut self) {
        if self.context.is_abandoned() {
            if let Some(resources) = self.resources.take() {
                std::mem::forget(resources);
            }
            if let Some(allocator) = self.allocator.take() {
                std::mem::forget(allocator);
            }
            return;
        }
        if let Some(mut resources) = self.resources.take() {
            let _queue_guard = self
                .context
                .queue_submit_lock
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            resources.destroy(
                &self.context.device,
                self.allocator.as_mut().expect("allocator missing"),
            );
        }
        drop(self.allocator.take());
    }
}

struct Resources {
    key: ResourceKey,
    atlas: GlyphAtlas,
    upload: Buffer,
    readback: Option<Buffer>,
    cell_readback: Buffer,
    input: Buffer,
    cells: Buffer,
    lut: Buffer,
    atlas_buffer: Buffer,
    output: Buffer,
    coordinate_lut: Buffer,
    direct_output: bool,
    descriptor_pool: vk::DescriptorPool,
    descriptor_layout: vk::DescriptorSetLayout,
    descriptor_set: vk::DescriptorSet,
    pipeline_layout: vk::PipelineLayout,
    map_pipeline: vk::Pipeline,
    map_variant: &'static str,
    map_workgroup_size: u32,
    render_pipeline: vk::Pipeline,
    render_local_size: (u32, u32),
    command_pool: vk::CommandPool,
    command: vk::CommandBuffer,
    fence: vk::Fence,
    query_pool: vk::QueryPool,
}

struct PendingBuffers<'a> {
    device: &'a ash::Device,
    device_info: &'a DeviceInfo,
    allocator: &'a mut Allocator,
    buffers: Vec<Buffer>,
}

impl<'a> PendingBuffers<'a> {
    fn new(
        device: &'a ash::Device,
        device_info: &'a DeviceInfo,
        allocator: &'a mut Allocator,
    ) -> Self {
        Self {
            device,
            device_info,
            allocator,
            buffers: Vec::with_capacity(8),
        }
    }

    fn create(
        &mut self,
        size: vk::DeviceSize,
        usage: vk::BufferUsageFlags,
        location: MemoryLocation,
        name: &'static str,
        _atom_size: vk::DeviceSize,
    ) -> Result<vk::Buffer> {
        let buffer = Buffer::new(
            self.device,
            self.allocator,
            size,
            usage,
            location,
            name,
            self.device_info,
        )?;
        let handle = buffer.handle;
        self.buffers.push(buffer);
        Ok(handle)
    }

    fn create_cached(
        &mut self,
        size: vk::DeviceSize,
        usage: vk::BufferUsageFlags,
        name: &'static str,
        _atom_size: vk::DeviceSize,
    ) -> Result<Option<vk::Buffer>> {
        let Some(buffer) = Buffer::new_cached(self.device, size, usage, name, self.device_info)?
        else {
            return Ok(None);
        };
        let handle = buffer.handle;
        self.buffers.push(buffer);
        Ok(Some(handle))
    }

    fn finish(mut self) -> Vec<Buffer> {
        std::mem::take(&mut self.buffers)
    }
}

impl Drop for PendingBuffers<'_> {
    fn drop(&mut self) {
        for buffer in self.buffers.iter_mut().rev() {
            buffer.destroy(self.device, self.allocator);
        }
    }
}

struct PendingObjects<'a> {
    device: &'a ash::Device,
    descriptor_pool: vk::DescriptorPool,
    descriptor_layout: vk::DescriptorSetLayout,
    pipeline_layout: vk::PipelineLayout,
    map_pipeline: vk::Pipeline,
    render_pipeline: vk::Pipeline,
    command_pool: vk::CommandPool,
    fence: vk::Fence,
    query_pool: vk::QueryPool,
}

impl<'a> PendingObjects<'a> {
    fn new(device: &'a ash::Device) -> Self {
        Self {
            device,
            descriptor_pool: vk::DescriptorPool::null(),
            descriptor_layout: vk::DescriptorSetLayout::null(),
            pipeline_layout: vk::PipelineLayout::null(),
            map_pipeline: vk::Pipeline::null(),
            render_pipeline: vk::Pipeline::null(),
            command_pool: vk::CommandPool::null(),
            fence: vk::Fence::null(),
            query_pool: vk::QueryPool::null(),
        }
    }

    fn disarm(&mut self) {
        self.descriptor_pool = vk::DescriptorPool::null();
        self.descriptor_layout = vk::DescriptorSetLayout::null();
        self.pipeline_layout = vk::PipelineLayout::null();
        self.map_pipeline = vk::Pipeline::null();
        self.render_pipeline = vk::Pipeline::null();
        self.command_pool = vk::CommandPool::null();
        self.fence = vk::Fence::null();
        self.query_pool = vk::QueryPool::null();
    }
}

impl Drop for PendingObjects<'_> {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_query_pool(self.query_pool, None);
            self.device.destroy_fence(self.fence, None);
            self.device.destroy_command_pool(self.command_pool, None);
            self.device.destroy_pipeline(self.render_pipeline, None);
            self.device.destroy_pipeline(self.map_pipeline, None);
            self.device
                .destroy_pipeline_layout(self.pipeline_layout, None);
            self.device
                .destroy_descriptor_pool(self.descriptor_pool, None);
            self.device
                .destroy_descriptor_set_layout(self.descriptor_layout, None);
        }
    }
}

impl Resources {
    fn allocated_buffer_bytes(&self) -> u64 {
        [
            Some(&self.upload),
            self.readback.as_ref(),
            Some(&self.cell_readback),
            Some(&self.input),
            Some(&self.cells),
            Some(&self.lut),
            Some(&self.atlas_buffer),
            Some(&self.output),
            Some(&self.coordinate_lut),
        ]
        .into_iter()
        .flatten()
        .map(|buffer| buffer.size)
        .sum()
    }

    fn new(
        context: &VulkanContext,
        allocator: &mut Allocator,
        key: ResourceKey,
        atlas: GlyphAtlas,
    ) -> Result<Self> {
        let device = &context.device;
        let frame_bytes = u64::try_from(key.format.frame_byte_len(key.width, key.height)?)
            .map_err(|_| Error::Vulkan("frame resource size exceeds u64".into()))?;
        let cell_bytes =
            key.grid_width as u64 * key.grid_height as u64 * size_of::<GpuAsciiCell>() as u64;
        let atlas_bytes = atlas.as_r8_slice().len() as u64;
        let pixel_count = key.width as u64 * key.height as u64;
        let coordinate_lut_bytes = (key.width as u64 + key.height as u64) * 8;
        let map_u32_safe = mapping_u32_is_safe(
            key.width,
            key.height,
            key.grid_width,
            key.grid_height,
            frame_bytes,
            u64::from(max_sample_for(key.format)),
        );
        let x_coordinate_max = (key.width - 1)
            .checked_mul(key.grid_width)
            .map(u64::from)
            .and_then(|scaled| scaled.checked_mul(atlas.width() as u64))
            .ok_or_else(|| Error::Vulkan("horizontal coordinate range overflow".into()))?;
        let y_coordinate_max = (key.height - 1)
            .checked_mul(key.grid_height)
            .map(u64::from)
            .and_then(|scaled| scaled.checked_mul(atlas.height() as u64))
            .ok_or_else(|| Error::Vulkan("vertical coordinate range overflow".into()))?;
        if x_coordinate_max > u32::MAX as u64
            || y_coordinate_max > u32::MAX as u64
            || pixel_count > u32::MAX as u64
            || frame_bytes > u32::MAX as u64
            || key.grid_width as u64 * key.grid_height as u64 > u32::MAX as u64
        {
            return Err(Error::Vulkan(
                "frame/grid/atlas dimensions exceed the 32-bit render path".into(),
            ));
        }
        let largest_storage = frame_bytes
            .max(cell_bytes)
            .max(atlas_bytes)
            .max(coordinate_lut_bytes)
            .max(1024);
        if largest_storage > context.info.max_storage_buffer_range {
            return Err(Error::Vulkan(format!(
                "required storage buffer is {largest_storage} bytes; device limit is {} bytes",
                context.info.max_storage_buffer_range
            )));
        }
        let upload_size = frame_bytes.max(cell_bytes).max(atlas_bytes).max(1024);
        let mut pending_buffers = PendingBuffers::new(device, &context.info, allocator);
        let _upload_handle = pending_buffers.create(
            upload_size,
            vk::BufferUsageFlags::TRANSFER_SRC,
            MemoryLocation::CpuToGpu,
            "upload staging",
            context.info.non_coherent_atom_size,
        )?;
        let readback_mode =
            std::env::var("ASCIIFLOW_VULKAN_READBACK").unwrap_or_else(|_| "cached".into());
        if !matches!(readback_mode.as_str(), "allocator" | "cached" | "direct") {
            return Err(Error::Vulkan(format!(
                "unknown ASCIIFLOW_VULKAN_READBACK={readback_mode}"
            )));
        }
        let direct_output = readback_mode == "direct";
        if !direct_output && readback_mode == "cached" {
            if pending_buffers
                .create_cached(
                    frame_bytes,
                    vk::BufferUsageFlags::TRANSFER_DST,
                    "NV12 readback",
                    context.info.non_coherent_atom_size,
                )?
                .is_none()
            {
                pending_buffers.create(
                    frame_bytes,
                    vk::BufferUsageFlags::TRANSFER_DST,
                    MemoryLocation::GpuToCpu,
                    "NV12 readback",
                    context.info.non_coherent_atom_size,
                )?;
            }
        } else if !direct_output {
            pending_buffers.create(
                frame_bytes,
                vk::BufferUsageFlags::TRANSFER_DST,
                MemoryLocation::GpuToCpu,
                "NV12 readback",
                context.info.non_coherent_atom_size,
            )?;
        }
        let _cell_readback_handle = pending_buffers.create(
            cell_bytes,
            vk::BufferUsageFlags::TRANSFER_DST,
            MemoryLocation::GpuToCpu,
            "cell readback",
            context.info.non_coherent_atom_size,
        )?;
        let input_handle = pending_buffers.create(
            frame_bytes,
            vk::BufferUsageFlags::TRANSFER_DST
                | vk::BufferUsageFlags::TRANSFER_SRC
                | vk::BufferUsageFlags::STORAGE_BUFFER,
            MemoryLocation::GpuOnly,
            "NV12 input",
            context.info.non_coherent_atom_size,
        )?;
        let cells_handle = pending_buffers.create(
            cell_bytes,
            vk::BufferUsageFlags::STORAGE_BUFFER
                | vk::BufferUsageFlags::TRANSFER_SRC
                | vk::BufferUsageFlags::TRANSFER_DST,
            MemoryLocation::GpuOnly,
            "ASCII cells",
            context.info.non_coherent_atom_size,
        )?;
        let lut_handle = pending_buffers.create(
            1024,
            vk::BufferUsageFlags::TRANSFER_DST | vk::BufferUsageFlags::STORAGE_BUFFER,
            MemoryLocation::GpuOnly,
            "glyph LUT",
            context.info.non_coherent_atom_size,
        )?;
        let atlas_handle = pending_buffers.create(
            atlas_bytes,
            vk::BufferUsageFlags::TRANSFER_DST | vk::BufferUsageFlags::STORAGE_BUFFER,
            MemoryLocation::GpuOnly,
            "glyph atlas",
            context.info.non_coherent_atom_size,
        )?;
        let output_handle = if direct_output {
            pending_buffers
                .create_cached(
                    frame_bytes,
                    vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_SRC,
                    "NV12 output",
                    context.info.non_coherent_atom_size,
                )?
                .ok_or_else(|| {
                    Error::Vulkan("direct output requires HOST_VISIBLE | HOST_CACHED memory".into())
                })?
        } else {
            pending_buffers.create(
                frame_bytes,
                vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_SRC,
                MemoryLocation::GpuOnly,
                "NV12 output",
                context.info.non_coherent_atom_size,
            )?
        };
        let coordinate_lut_handle = pending_buffers.create(
            coordinate_lut_bytes,
            vk::BufferUsageFlags::TRANSFER_DST | vk::BufferUsageFlags::STORAGE_BUFFER,
            MemoryLocation::GpuOnly,
            "coordinate LUT",
            context.info.non_coherent_atom_size,
        )?;
        let bindings: [vk::DescriptorSetLayoutBinding; 6] = std::array::from_fn(|binding| {
            vk::DescriptorSetLayoutBinding::default()
                .binding(binding as u32)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE)
        });
        let mut pending_objects = PendingObjects::new(device);
        pending_objects.descriptor_layout = unsafe {
            device.create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                None,
            )
        }
        .map_err(vk_error("failed to create descriptor layout"))?;
        let descriptor_layout = pending_objects.descriptor_layout;
        pending_objects.descriptor_pool = unsafe {
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
        .map_err(vk_error("failed to create descriptor pool"))?;
        let descriptor_pool = pending_objects.descriptor_pool;
        let descriptor_set = unsafe {
            device.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(descriptor_pool)
                    .set_layouts(&[descriptor_layout]),
            )
        }
        .map_err(vk_error("failed to allocate descriptor set"))?
        .into_iter()
        .next()
        .ok_or_else(|| Error::Vulkan("Vulkan returned no descriptor set".into()))?;
        let infos = [
            input_handle,
            cells_handle,
            lut_handle,
            atlas_handle,
            output_handle,
            coordinate_lut_handle,
        ]
        .map(|buffer| vk::DescriptorBufferInfo {
            buffer,
            offset: 0,
            range: vk::WHOLE_SIZE,
        });
        let writes: Vec<_> = infos
            .iter()
            .enumerate()
            .map(|(binding, info)| {
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_set)
                    .dst_binding(binding as u32)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .buffer_info(std::slice::from_ref(info))
            })
            .collect();
        unsafe { device.update_descriptor_sets(&writes, &[]) };
        let push_range = [vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::COMPUTE)
            .offset(0)
            .size(size_of::<PushConstants>() as u32)];
        pending_objects.pipeline_layout = unsafe {
            device.create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .set_layouts(&[descriptor_layout])
                    .push_constant_ranges(&push_range),
                None,
            )
        }
        .map_err(vk_error("failed to create compute pipeline layout"))?;
        let pipeline_layout = pending_objects.pipeline_layout;
        let (map_shader, map_variant, map_workgroup_size) = if key.format == PixelFormat::P010Le {
            if map_u32_safe {
                (MAP_P010_U32_32, "p010-u32-32", 32)
            } else {
                (MAP_P010_U64_64, "p010-u64-64", 64)
            }
        } else {
            match std::env::var("ASCIIFLOW_VULKAN_MAP_VARIANT").as_deref() {
                Ok("u64-64") => (MAP_U64_64, "u64-64", 64),
                Ok(value @ ("u32-32" | "u32-64" | "u32-128" | "u32-256")) if !map_u32_safe => {
                    return Err(Error::Vulkan(format!(
                        "{value} Pass 1 was requested but this workload exceeds its proven bounds"
                    )));
                }
                Ok("u32-32") => (MAP_U32_32, "u32-32", 32),
                Ok("u32-64") => (MAP_U32_64, "u32-64", 64),
                Ok("u32-128") => (MAP_U32_128, "u32-128", 128),
                Ok("u32-256") => (MAP_U32_256, "u32-256", 256),
                Ok(value) => {
                    return Err(Error::Vulkan(format!(
                        "unknown ASCIIFLOW_VULKAN_MAP_VARIANT={value}"
                    )));
                }
                Err(_) if map_u32_safe => (MAP_U32_32, "u32-32", 32),
                Err(_) => (MAP_U64_64, "u64-64", 64),
            }
        };
        if map_workgroup_size > context.info.max_compute_work_group_invocations
            || map_workgroup_size > context.info.max_compute_work_group_size[0]
        {
            return Err(Error::Vulkan(format!(
                "mapping workgroup {map_workgroup_size} exceeds device limits"
            )));
        }
        pending_objects.map_pipeline = create_pipeline(device, pipeline_layout, map_shader)?;
        let map_pipeline = pending_objects.map_pipeline;
        let (render_shader, render_local_size) = if key.format == PixelFormat::P010Le {
            (RENDER_P010_LUT_32X4, (32, 4))
        } else {
            match std::env::var("ASCIIFLOW_VULKAN_RENDER_VARIANT").as_deref() {
                Ok("int64-8x8") => (RENDER_INT64_8X8, (8, 8)),
                Ok("u32-16x8") => (RENDER_U32_16X8, (16, 8)),
                Ok("u32-16x16") => (RENDER_U32_16X16, (16, 16)),
                Ok("u32-32x4") => (RENDER_U32_32X4, (32, 4)),
                Ok("lut-8x8") => (RENDER_LUT_8X8, (8, 8)),
                Ok("lut-16x8") => (RENDER_LUT_16X8, (16, 8)),
                Ok("lut-16x16") => (RENDER_LUT_16X16, (16, 16)),
                Ok("lut-32x4") => (RENDER_LUT_32X4, (32, 4)),
                Ok("u32-8x8") => (RENDER_U32_8X8, (8, 8)),
                Err(_) => (RENDER_LUT_32X4, (32, 4)),
                Ok(value) => {
                    return Err(Error::Vulkan(format!(
                        "unknown ASCIIFLOW_VULKAN_RENDER_VARIANT={value}"
                    )));
                }
            }
        };
        if render_local_size.0 * render_local_size.1
            > context.info.max_compute_work_group_invocations
            || render_local_size.0 > context.info.max_compute_work_group_size[0]
            || render_local_size.1 > context.info.max_compute_work_group_size[1]
        {
            return Err(Error::Vulkan(format!(
                "render workgroup {}x{} exceeds device limits",
                render_local_size.0, render_local_size.1
            )));
        }
        pending_objects.render_pipeline = create_pipeline(device, pipeline_layout, render_shader)?;
        let render_pipeline = pending_objects.render_pipeline;
        pending_objects.command_pool = unsafe {
            device.create_command_pool(
                &vk::CommandPoolCreateInfo::default()
                    .queue_family_index(context.info.queue_family)
                    .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
                None,
            )
        }
        .map_err(vk_error("failed to create command pool"))?;
        let command_pool = pending_objects.command_pool;
        let command = unsafe {
            device.allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::default()
                    .command_pool(command_pool)
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .command_buffer_count(1),
            )
        }
        .map_err(vk_error("failed to allocate command buffer"))?
        .into_iter()
        .next()
        .ok_or_else(|| Error::Vulkan("Vulkan returned no command buffer".into()))?;
        pending_objects.fence =
            unsafe { device.create_fence(&vk::FenceCreateInfo::default(), None) }
                .map_err(vk_error("failed to create compute fence"))?;
        let fence = pending_objects.fence;
        pending_objects.query_pool = unsafe {
            device.create_query_pool(
                &vk::QueryPoolCreateInfo::default()
                    .query_type(vk::QueryType::TIMESTAMP)
                    .query_count(QUERY_COUNT),
                None,
            )
        }
        .map_err(vk_error("failed to create timestamp query pool"))?;
        let query_pool = pending_objects.query_pool;
        let mut buffers = pending_buffers.finish().into_iter();
        let upload = buffers.next().expect("upload buffer missing");
        let readback = if direct_output {
            None
        } else {
            Some(buffers.next().expect("readback buffer missing"))
        };
        let cell_readback = buffers.next().expect("cell readback buffer missing");
        let input = buffers.next().expect("input buffer missing");
        let cells = buffers.next().expect("cell buffer missing");
        let lut = buffers.next().expect("LUT buffer missing");
        let atlas_buffer = buffers.next().expect("atlas buffer missing");
        let output = buffers.next().expect("output buffer missing");
        let coordinate_lut = buffers.next().expect("coordinate LUT buffer missing");
        debug_assert!(buffers.next().is_none());
        pending_objects.disarm();
        let mut result = Self {
            key,
            atlas,
            upload,
            readback,
            cell_readback,
            input,
            cells,
            lut,
            atlas_buffer,
            output,
            coordinate_lut,
            direct_output,
            descriptor_pool,
            descriptor_layout,
            descriptor_set,
            pipeline_layout,
            map_pipeline,
            map_variant,
            map_workgroup_size,
            render_pipeline,
            render_local_size,
            command_pool,
            command,
            fence,
            query_pool,
        };
        let lut_data = glyph_lookup_table(result.atlas.glyph_count());
        if let Err(error) =
            result.upload_static(context, bytemuck::cast_slice(&lut_data), result.lut.handle)
        {
            result.destroy(device, allocator);
            return Err(error);
        }
        let atlas_data = result.atlas.as_r8_slice().to_vec();
        let atlas_upload_started = Instant::now();
        if let Err(error) = result.upload_static(context, &atlas_data, result.atlas_buffer.handle) {
            result.destroy(device, allocator);
            return Err(error);
        }
        tracing::info!(
            target: "asciiflow",
            atlas_bytes = atlas_data.len(),
            atlas_upload_wall_ms = atlas_upload_started.elapsed().as_secs_f64() * 1000.0,
            "glyph atlas initialization upload"
        );
        let mut coordinate_lut =
            Vec::<[u32; 2]>::with_capacity((result.key.width + result.key.height) as usize);
        for x in 0..result.key.width {
            let scaled = x as u64 * result.key.grid_width as u64;
            coordinate_lut.push([
                (scaled / result.key.width as u64) as u32,
                (scaled * result.atlas.width() as u64 / result.key.width as u64
                    % result.atlas.width() as u64) as u32,
            ]);
        }
        for y in 0..result.key.height {
            let scaled = y as u64 * result.key.grid_height as u64;
            coordinate_lut.push([
                (scaled / result.key.height as u64) as u32,
                (scaled * result.atlas.height() as u64 / result.key.height as u64
                    % result.atlas.height() as u64) as u32,
            ]);
        }
        let coordinate_lut_data = bytemuck::cast_slice(&coordinate_lut).to_vec();
        if let Err(error) =
            result.upload_static(context, &coordinate_lut_data, result.coordinate_lut.handle)
        {
            result.destroy(device, allocator);
            return Err(error);
        }
        Ok(result)
    }

    fn params(&self, color: bool) -> PushConstants {
        PushConstants {
            width: self.key.width,
            height: self.key.height,
            grid_width: self.key.grid_width,
            grid_height: self.key.grid_height,
            atlas_width: self.atlas.width(),
            atlas_height: self.atlas.height(),
            color: color as u32,
            glyph_count: self.atlas.glyph_count() as u32,
            neutral_chroma: u32::from(self.key.format.neutral_chroma()),
        }
    }
    fn begin(&self, context: &VulkanContext) -> Result<()> {
        unsafe {
            context
                .device
                .reset_fences(&[self.fence])
                .map_err(vk_error("failed to reset Vulkan fence"))?;
            context
                .device
                .reset_command_buffer(self.command, vk::CommandBufferResetFlags::empty())
                .map_err(vk_error("failed to reset command buffer"))?;
            context
                .device
                .begin_command_buffer(
                    self.command,
                    &vk::CommandBufferBeginInfo::default()
                        .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
                )
                .map_err(vk_error("failed to begin command buffer"))?;
        }
        Ok(())
    }
    fn submit_wait(&self, context: &VulkanContext) -> Result<SubmitTimings> {
        unsafe {
            context
                .device
                .end_command_buffer(self.command)
                .map_err(vk_error("failed to end command buffer"))?;
            let command_info =
                [vk::CommandBufferSubmitInfo::default().command_buffer(self.command)];
            let queue_guard = context
                .queue_submit_lock
                .lock()
                .map_err(|_| Error::Vulkan("Vulkan queue lock was poisoned".into()))?;
            let submit_started = Instant::now();
            if let Err(error) = context.device.queue_submit2(
                context.queue,
                &[vk::SubmitInfo2::default().command_buffer_infos(&command_info)],
                self.fence,
            ) {
                if error == vk::Result::ERROR_DEVICE_LOST {
                    context.abandon();
                }
                return Err(vk_error("failed to submit Vulkan work")(error));
            }
            drop(queue_guard);
            let queue_submit = submit_started.elapsed();
            let wait_started = Instant::now();
            match context.device.wait_for_fences(
                &[self.fence],
                true,
                GPU_COMPLETION_TIMEOUT.as_nanos() as u64,
            ) {
                Ok(()) => {}
                Err(vk::Result::TIMEOUT) => {
                    context.abandon();
                    return Err(Error::TeardownTimeout(format!(
                        "Vulkan fence did not signal within {:.0} seconds; the device was abandoned because in-flight resources cannot be destroyed safely",
                        GPU_COMPLETION_TIMEOUT.as_secs_f64()
                    )));
                }
                Err(error) => {
                    if error == vk::Result::ERROR_DEVICE_LOST {
                        context.abandon();
                    }
                    return Err(vk_error("Vulkan work failed")(error));
                }
            }
            Ok(SubmitTimings {
                queue_submit,
                gpu_wait: wait_started.elapsed(),
            })
        }
    }
    fn upload_static(
        &mut self,
        context: &VulkanContext,
        data: &[u8],
        destination: vk::Buffer,
    ) -> Result<()> {
        self.upload.write(&context.device, data)?;
        self.begin(context)?;
        unsafe {
            context.device.cmd_copy_buffer(
                self.command,
                self.upload.handle,
                destination,
                &[vk::BufferCopy {
                    src_offset: 0,
                    dst_offset: 0,
                    size: data.len() as u64,
                }],
            );
        }
        self.submit_wait(context).map(|_| ())
    }
    fn upload_input(&mut self, context: &VulkanContext, data: &[u8]) -> Result<UploadTimings> {
        self.upload_buffer(context, data, self.input.handle)
    }
    fn copy_external_input(
        &mut self,
        context: &VulkanContext,
        planes: [&ImportedExternalPlane; 2],
    ) -> Result<ExternalCopyTimings> {
        self.begin(context)?;
        let acquire = planes.map(|plane| {
            vk::ImageMemoryBarrier2::default()
                .src_stage_mask(vk::PipelineStageFlags2::NONE)
                .src_access_mask(vk::AccessFlags2::NONE)
                .dst_stage_mask(vk::PipelineStageFlags2::COPY)
                .dst_access_mask(vk::AccessFlags2::TRANSFER_READ)
                .old_layout(vk::ImageLayout::GENERAL)
                .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_FOREIGN_EXT)
                .dst_queue_family_index(context.info.queue_family)
                .image(plane.image)
                .subresource_range(
                    vk::ImageSubresourceRange::default()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .level_count(1)
                        .layer_count(1),
                )
        });
        let ownership_started = Instant::now();
        unsafe {
            context
                .device
                .cmd_reset_query_pool(self.command, self.query_pool, 0, QUERY_COUNT);
            context.device.cmd_pipeline_barrier2(
                self.command,
                &vk::DependencyInfo::default().image_memory_barriers(&acquire),
            );
        }
        let ownership_acquire_record = ownership_started.elapsed();
        unsafe {
            context.device.cmd_write_timestamp2(
                self.command,
                vk::PipelineStageFlags2::COPY,
                self.query_pool,
                QUERY_UPLOAD_BEGIN,
            );
            for plane in planes {
                let buffer_offset = match plane.kind {
                    ExternalPlaneKind::Y | ExternalPlaneKind::P010Y => 0,
                    ExternalPlaneKind::Uv | ExternalPlaneKind::P010Uv => {
                        self.key.width as u64
                            * self.key.height as u64
                            * self.key.format.bytes_per_sample() as u64
                    }
                };
                context.device.cmd_copy_image_to_buffer(
                    self.command,
                    plane.image,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    self.input.handle,
                    &[vk::BufferImageCopy::default()
                        .buffer_offset(buffer_offset)
                        .image_subresource(
                            vk::ImageSubresourceLayers::default()
                                .aspect_mask(vk::ImageAspectFlags::COLOR)
                                .layer_count(1),
                        )
                        .image_extent(vk::Extent3D {
                            width: plane.width,
                            height: plane.height,
                            depth: 1,
                        })],
                );
            }
            context.device.cmd_write_timestamp2(
                self.command,
                vk::PipelineStageFlags2::COPY,
                self.query_pool,
                QUERY_UPLOAD_BEGIN + 1,
            );
        }
        let release = planes.map(|plane| {
            vk::ImageMemoryBarrier2::default()
                .src_stage_mask(vk::PipelineStageFlags2::COPY)
                .src_access_mask(vk::AccessFlags2::TRANSFER_READ)
                .dst_stage_mask(vk::PipelineStageFlags2::NONE)
                .dst_access_mask(vk::AccessFlags2::NONE)
                .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                .new_layout(vk::ImageLayout::GENERAL)
                .src_queue_family_index(context.info.queue_family)
                .dst_queue_family_index(vk::QUEUE_FAMILY_FOREIGN_EXT)
                .image(plane.image)
                .subresource_range(
                    vk::ImageSubresourceRange::default()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .level_count(1)
                        .layer_count(1),
                )
        });
        let release_started = Instant::now();
        unsafe {
            context.device.cmd_pipeline_barrier2(
                self.command,
                &vk::DependencyInfo::default().image_memory_barriers(&release),
            );
        }
        let ownership_release_record = release_started.elapsed();
        let submit = self.submit_wait(context)?;
        Ok(ExternalCopyTimings {
            gpu_copy: self.timestamp_duration(context, QUERY_UPLOAD_BEGIN)?,
            ownership_acquire_record,
            ownership_release_record,
            submit,
        })
    }

    fn copy_output_to_external(
        &mut self,
        context: &VulkanContext,
        planes: [&ImportedExternalPlane; 2],
    ) -> Result<ExternalCopyTimings> {
        self.begin(context)?;
        let output_barrier = [vk::BufferMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
            .src_access_mask(vk::AccessFlags2::SHADER_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::COPY)
            .dst_access_mask(vk::AccessFlags2::TRANSFER_READ)
            .buffer(self.output.handle)
            .offset(0)
            .size(vk::WHOLE_SIZE)];
        let acquire = planes.map(|plane| {
            vk::ImageMemoryBarrier2::default()
                .src_stage_mask(vk::PipelineStageFlags2::NONE)
                .src_access_mask(vk::AccessFlags2::NONE)
                .dst_stage_mask(vk::PipelineStageFlags2::COPY)
                .dst_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
                .old_layout(vk::ImageLayout::GENERAL)
                .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_FOREIGN_EXT)
                .dst_queue_family_index(context.info.queue_family)
                .image(plane.image)
                .subresource_range(
                    vk::ImageSubresourceRange::default()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .level_count(1)
                        .layer_count(1),
                )
        });
        let acquire_started = Instant::now();
        unsafe {
            context.device.cmd_pipeline_barrier2(
                self.command,
                &vk::DependencyInfo::default()
                    .buffer_memory_barriers(&output_barrier)
                    .image_memory_barriers(&acquire),
            );
        }
        let ownership_acquire_record = acquire_started.elapsed();
        unsafe {
            context.device.cmd_write_timestamp2(
                self.command,
                vk::PipelineStageFlags2::COPY,
                self.query_pool,
                QUERY_DOWNLOAD_BEGIN,
            );
            for plane in planes {
                let buffer_offset = match plane.kind {
                    ExternalPlaneKind::Y | ExternalPlaneKind::P010Y => 0,
                    ExternalPlaneKind::Uv | ExternalPlaneKind::P010Uv => {
                        self.key.width as u64
                            * self.key.height as u64
                            * self.key.format.bytes_per_sample() as u64
                    }
                };
                context.device.cmd_copy_buffer_to_image(
                    self.command,
                    self.output.handle,
                    plane.image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[vk::BufferImageCopy::default()
                        .buffer_offset(buffer_offset)
                        .image_subresource(
                            vk::ImageSubresourceLayers::default()
                                .aspect_mask(vk::ImageAspectFlags::COLOR)
                                .layer_count(1),
                        )
                        .image_extent(vk::Extent3D {
                            width: plane.width,
                            height: plane.height,
                            depth: 1,
                        })],
                );
            }
            context.device.cmd_write_timestamp2(
                self.command,
                vk::PipelineStageFlags2::COPY,
                self.query_pool,
                QUERY_DOWNLOAD_BEGIN + 1,
            );
        }
        let release = planes.map(|plane| {
            vk::ImageMemoryBarrier2::default()
                .src_stage_mask(vk::PipelineStageFlags2::COPY)
                .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
                .dst_stage_mask(vk::PipelineStageFlags2::NONE)
                .dst_access_mask(vk::AccessFlags2::NONE)
                .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .new_layout(vk::ImageLayout::GENERAL)
                .src_queue_family_index(context.info.queue_family)
                .dst_queue_family_index(vk::QUEUE_FAMILY_FOREIGN_EXT)
                .image(plane.image)
                .subresource_range(
                    vk::ImageSubresourceRange::default()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .level_count(1)
                        .layer_count(1),
                )
        });
        let release_started = Instant::now();
        unsafe {
            context.device.cmd_pipeline_barrier2(
                self.command,
                &vk::DependencyInfo::default().image_memory_barriers(&release),
            );
        }
        let ownership_release_record = release_started.elapsed();
        let submit = self.submit_wait(context)?;
        Ok(ExternalCopyTimings {
            gpu_copy: self.timestamp_duration(context, QUERY_DOWNLOAD_BEGIN)?,
            ownership_acquire_record,
            ownership_release_record,
            submit,
        })
    }
    fn upload_buffer(
        &mut self,
        context: &VulkanContext,
        data: &[u8],
        destination: vk::Buffer,
    ) -> Result<UploadTimings> {
        let host_started = Instant::now();
        self.upload.write(&context.device, data)?;
        let host_upload = host_started.elapsed();
        self.begin(context)?;
        unsafe {
            context
                .device
                .cmd_reset_query_pool(self.command, self.query_pool, 0, QUERY_COUNT);
            context.device.cmd_write_timestamp2(
                self.command,
                vk::PipelineStageFlags2::COPY,
                self.query_pool,
                QUERY_UPLOAD_BEGIN,
            );
            context.device.cmd_copy_buffer(
                self.command,
                self.upload.handle,
                destination,
                &[vk::BufferCopy {
                    src_offset: 0,
                    dst_offset: 0,
                    size: data.len() as u64,
                }],
            );
            context.device.cmd_write_timestamp2(
                self.command,
                vk::PipelineStageFlags2::COPY,
                self.query_pool,
                QUERY_UPLOAD_BEGIN + 1,
            );
        }
        let submit = self.submit_wait(context)?;
        Ok(UploadTimings {
            host_upload,
            gpu_upload: self.timestamp_duration(context, QUERY_UPLOAD_BEGIN)?,
            submit,
        })
    }
    fn run_compute(
        &mut self,
        context: &VulkanContext,
        color: bool,
        mapping: bool,
        render: bool,
        prepare_host_readback: bool,
    ) -> Result<ComputeTimings> {
        self.begin(context)?;
        let device = &context.device;
        let params = self.params(color);
        unsafe {
            let readable_buffers = [
                self.input.handle,
                self.lut.handle,
                self.atlas_buffer.handle,
                self.coordinate_lut.handle,
            ]
            .map(|buffer| {
                vk::BufferMemoryBarrier2::default()
                    .src_stage_mask(vk::PipelineStageFlags2::TRANSFER)
                    .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
                    .dst_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
                    .dst_access_mask(vk::AccessFlags2::SHADER_READ)
                    .buffer(buffer)
                    .offset(0)
                    .size(vk::WHOLE_SIZE)
            });
            device.cmd_pipeline_barrier2(
                self.command,
                &vk::DependencyInfo::default().buffer_memory_barriers(&readable_buffers),
            );
            device.cmd_bind_descriptor_sets(
                self.command,
                vk::PipelineBindPoint::COMPUTE,
                self.pipeline_layout,
                0,
                &[self.descriptor_set],
                &[],
            );
            device.cmd_push_constants(
                self.command,
                self.pipeline_layout,
                vk::ShaderStageFlags::COMPUTE,
                0,
                bytemuck::bytes_of(&params),
            );
            if mapping {
                device.cmd_bind_pipeline(
                    self.command,
                    vk::PipelineBindPoint::COMPUTE,
                    self.map_pipeline,
                );
                device.cmd_write_timestamp2(
                    self.command,
                    vk::PipelineStageFlags2::COMPUTE_SHADER,
                    self.query_pool,
                    QUERY_MAPPING_BEGIN,
                );
                device.cmd_dispatch(self.command, self.key.grid_width, self.key.grid_height, 1);
                device.cmd_write_timestamp2(
                    self.command,
                    vk::PipelineStageFlags2::COMPUTE_SHADER,
                    self.query_pool,
                    QUERY_MAPPING_BEGIN + 1,
                );
            }
            if render {
                let barrier = [vk::BufferMemoryBarrier2::default()
                    .src_stage_mask(if mapping {
                        vk::PipelineStageFlags2::COMPUTE_SHADER
                    } else {
                        vk::PipelineStageFlags2::TRANSFER
                    })
                    .src_access_mask(if mapping {
                        vk::AccessFlags2::SHADER_WRITE
                    } else {
                        vk::AccessFlags2::TRANSFER_WRITE
                    })
                    .dst_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
                    .dst_access_mask(vk::AccessFlags2::SHADER_READ)
                    .buffer(self.cells.handle)
                    .offset(0)
                    .size(vk::WHOLE_SIZE)];
                device.cmd_pipeline_barrier2(
                    self.command,
                    &vk::DependencyInfo::default().buffer_memory_barriers(&barrier),
                );
                device.cmd_bind_pipeline(
                    self.command,
                    vk::PipelineBindPoint::COMPUTE,
                    self.render_pipeline,
                );
                device.cmd_write_timestamp2(
                    self.command,
                    vk::PipelineStageFlags2::COMPUTE_SHADER,
                    self.query_pool,
                    QUERY_RENDER_BEGIN,
                );
                device.cmd_dispatch(
                    self.command,
                    (self.key.width / 2).div_ceil(self.render_local_size.0),
                    (self.key.height / 2).div_ceil(self.render_local_size.1),
                    1,
                );
                device.cmd_write_timestamp2(
                    self.command,
                    vk::PipelineStageFlags2::COMPUTE_SHADER,
                    self.query_pool,
                    QUERY_RENDER_BEGIN + 1,
                );
                if self.direct_output && prepare_host_readback {
                    let host_barrier = [vk::BufferMemoryBarrier2::default()
                        .src_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
                        .src_access_mask(vk::AccessFlags2::SHADER_WRITE)
                        .dst_stage_mask(vk::PipelineStageFlags2::HOST)
                        .dst_access_mask(vk::AccessFlags2::HOST_READ)
                        .buffer(self.output.handle)
                        .offset(0)
                        .size(vk::WHOLE_SIZE)];
                    device.cmd_pipeline_barrier2(
                        self.command,
                        &vk::DependencyInfo::default().buffer_memory_barriers(&host_barrier),
                    );
                }
            }
        }
        let submit = self.submit_wait(context)?;
        Ok(ComputeTimings {
            gpu_mapping: if mapping {
                self.timestamp_duration(context, QUERY_MAPPING_BEGIN)?
            } else {
                Duration::ZERO
            },
            gpu_render: if render {
                self.timestamp_duration(context, QUERY_RENDER_BEGIN)?
            } else {
                Duration::ZERO
            },
            submit,
        })
    }
    fn download_output(
        &mut self,
        context: &VulkanContext,
        length: usize,
    ) -> Result<DownloadResult> {
        if self.direct_output {
            let mut bytes = vec![0; length];
            let host = self.output.read_into(&context.device, &mut bytes)?;
            return Ok(DownloadResult {
                bytes,
                gpu_download: Duration::ZERO,
                gpu_busy: self.timestamp_span(
                    context,
                    QUERY_UPLOAD_BEGIN,
                    QUERY_RENDER_BEGIN + 1,
                )?,
                host_invalidate: host.invalidate,
                host_readback: host.copy,
                submit: SubmitTimings::default(),
            });
        }
        self.begin(context)?;
        unsafe {
            let barrier = [vk::BufferMemoryBarrier2::default()
                .src_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
                .src_access_mask(vk::AccessFlags2::SHADER_WRITE)
                .dst_stage_mask(vk::PipelineStageFlags2::TRANSFER)
                .dst_access_mask(vk::AccessFlags2::TRANSFER_READ)
                .buffer(self.output.handle)
                .offset(0)
                .size(vk::WHOLE_SIZE)];
            context.device.cmd_pipeline_barrier2(
                self.command,
                &vk::DependencyInfo::default().buffer_memory_barriers(&barrier),
            );
            context.device.cmd_write_timestamp2(
                self.command,
                vk::PipelineStageFlags2::COPY,
                self.query_pool,
                QUERY_DOWNLOAD_BEGIN,
            );
            context.device.cmd_copy_buffer(
                self.command,
                self.output.handle,
                self.readback
                    .as_ref()
                    .expect("readback buffer missing")
                    .handle,
                &[vk::BufferCopy {
                    src_offset: 0,
                    dst_offset: 0,
                    size: length as u64,
                }],
            );
            context.device.cmd_write_timestamp2(
                self.command,
                vk::PipelineStageFlags2::COPY,
                self.query_pool,
                QUERY_DOWNLOAD_BEGIN + 1,
            );
            let host_barrier = [vk::BufferMemoryBarrier2::default()
                .src_stage_mask(vk::PipelineStageFlags2::COPY)
                .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
                .dst_stage_mask(vk::PipelineStageFlags2::HOST)
                .dst_access_mask(vk::AccessFlags2::HOST_READ)
                .buffer(
                    self.readback
                        .as_ref()
                        .expect("readback buffer missing")
                        .handle,
                )
                .offset(0)
                .size(vk::WHOLE_SIZE)];
            context.device.cmd_pipeline_barrier2(
                self.command,
                &vk::DependencyInfo::default().buffer_memory_barriers(&host_barrier),
            );
        }
        let submit = self.submit_wait(context)?;
        let gpu_download = self.timestamp_duration(context, QUERY_DOWNLOAD_BEGIN)?;
        let mut bytes = vec![0; length];
        let host = self
            .readback
            .as_ref()
            .expect("readback buffer missing")
            .read_into(&context.device, &mut bytes)?;
        Ok(DownloadResult {
            bytes,
            gpu_download,
            gpu_busy: self.timestamp_span(context, QUERY_UPLOAD_BEGIN, QUERY_DOWNLOAD_BEGIN + 1)?,
            host_invalidate: host.invalidate,
            host_readback: host.copy,
            submit,
        })
    }
    fn download_input(&mut self, context: &VulkanContext, length: usize) -> Result<Vec<u8>> {
        let readback = self.readback.as_ref().ok_or_else(|| {
            Error::Vulkan(
                "diagnostic external-input readback is unavailable in direct-output mode".into(),
            )
        })?;
        self.begin(context)?;
        unsafe {
            let barrier = [vk::BufferMemoryBarrier2::default()
                .src_stage_mask(vk::PipelineStageFlags2::COPY)
                .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
                .dst_stage_mask(vk::PipelineStageFlags2::COPY)
                .dst_access_mask(vk::AccessFlags2::TRANSFER_READ)
                .buffer(self.input.handle)
                .offset(0)
                .size(vk::WHOLE_SIZE)];
            context.device.cmd_pipeline_barrier2(
                self.command,
                &vk::DependencyInfo::default().buffer_memory_barriers(&barrier),
            );
            context.device.cmd_copy_buffer(
                self.command,
                self.input.handle,
                readback.handle,
                &[vk::BufferCopy {
                    src_offset: 0,
                    dst_offset: 0,
                    size: length as u64,
                }],
            );
            let host_barrier = [vk::BufferMemoryBarrier2::default()
                .src_stage_mask(vk::PipelineStageFlags2::COPY)
                .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
                .dst_stage_mask(vk::PipelineStageFlags2::HOST)
                .dst_access_mask(vk::AccessFlags2::HOST_READ)
                .buffer(readback.handle)
                .offset(0)
                .size(vk::WHOLE_SIZE)];
            context.device.cmd_pipeline_barrier2(
                self.command,
                &vk::DependencyInfo::default().buffer_memory_barriers(&host_barrier),
            );
        }
        self.submit_wait(context)?;
        readback.read(&context.device, length)
    }
    fn download_cells(&mut self, context: &VulkanContext) -> Result<Vec<GpuAsciiCell>> {
        let bytes = self.key.grid_width as usize
            * self.key.grid_height as usize
            * size_of::<GpuAsciiCell>();
        self.begin(context)?;
        unsafe {
            let barrier = [vk::BufferMemoryBarrier2::default()
                .src_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
                .src_access_mask(vk::AccessFlags2::SHADER_WRITE)
                .dst_stage_mask(vk::PipelineStageFlags2::TRANSFER)
                .dst_access_mask(vk::AccessFlags2::TRANSFER_READ)
                .buffer(self.cells.handle)
                .offset(0)
                .size(vk::WHOLE_SIZE)];
            context.device.cmd_pipeline_barrier2(
                self.command,
                &vk::DependencyInfo::default().buffer_memory_barriers(&barrier),
            );
            context.device.cmd_copy_buffer(
                self.command,
                self.cells.handle,
                self.cell_readback.handle,
                &[vk::BufferCopy {
                    src_offset: 0,
                    dst_offset: 0,
                    size: bytes as u64,
                }],
            );
        }
        self.submit_wait(context)?;
        let data = self.cell_readback.read(&context.device, bytes)?;
        Ok(bytemuck::cast_slice(&data).to_vec())
    }
    fn timestamp_duration(&self, context: &VulkanContext, first: u32) -> Result<Duration> {
        self.timestamp_span(context, first, first + 1)
    }
    fn timestamp_span(&self, context: &VulkanContext, first: u32, last: u32) -> Result<Duration> {
        let mut timestamps = [0_u64; 2];
        unsafe {
            context
                .device
                .get_query_pool_results(
                    self.query_pool,
                    first,
                    std::slice::from_mut(&mut timestamps[0]),
                    vk::QueryResultFlags::TYPE_64 | vk::QueryResultFlags::WAIT,
                )
                .map_err(vk_error("failed to read Vulkan timestamp query"))?;
            context
                .device
                .get_query_pool_results(
                    self.query_pool,
                    last,
                    std::slice::from_mut(&mut timestamps[1]),
                    vk::QueryResultFlags::TYPE_64 | vk::QueryResultFlags::WAIT,
                )
                .map_err(vk_error("failed to read Vulkan timestamp query"))?;
        }
        let bits = context.info.timestamp_valid_bits;
        let mask = if bits == 64 {
            u64::MAX
        } else {
            (1_u64 << bits) - 1
        };
        let ticks = timestamps[1].wrapping_sub(timestamps[0]) & mask;
        Ok(Duration::from_secs_f64(
            ticks as f64 * context.info.timestamp_period_ns as f64 / 1e9,
        ))
    }
    fn destroy(&mut self, device: &ash::Device, allocator: &mut Allocator) {
        unsafe {
            device.destroy_query_pool(self.query_pool, None);
            device.destroy_fence(self.fence, None);
            device.destroy_command_pool(self.command_pool, None);
            device.destroy_pipeline(self.render_pipeline, None);
            device.destroy_pipeline(self.map_pipeline, None);
            device.destroy_pipeline_layout(self.pipeline_layout, None);
            device.destroy_descriptor_pool(self.descriptor_pool, None);
            device.destroy_descriptor_set_layout(self.descriptor_layout, None);
        }
        for buffer in [
            &mut self.output,
            &mut self.coordinate_lut,
            &mut self.atlas_buffer,
            &mut self.lut,
            &mut self.cells,
            &mut self.input,
            &mut self.cell_readback,
            &mut self.upload,
        ] {
            buffer.destroy(device, allocator);
        }
        if let Some(readback) = &mut self.readback {
            readback.destroy(device, allocator);
        }
    }
}

fn create_pipeline(
    device: &ash::Device,
    layout: vk::PipelineLayout,
    bytes: &[u8],
) -> Result<vk::Pipeline> {
    let code = read_spv(&mut Cursor::new(bytes))
        .map_err(|e| Error::Vulkan(format!("invalid embedded SPIR-V: {e}")))?;
    let module = unsafe {
        device.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&code), None)
    }
    .map_err(vk_error("failed to create shader module"))?;
    let stage = vk::PipelineShaderStageCreateInfo::default()
        .stage(vk::ShaderStageFlags::COMPUTE)
        .module(module)
        .name(c"main");
    let result = unsafe {
        device.create_compute_pipelines(
            vk::PipelineCache::null(),
            &[vk::ComputePipelineCreateInfo::default()
                .stage(stage)
                .layout(layout)],
            None,
        )
    };
    unsafe { device.destroy_shader_module(module, None) };
    match result {
        Ok(mut pipelines) => pipelines
            .pop()
            .ok_or_else(|| Error::Vulkan("Vulkan returned no compute pipeline".into())),
        Err((pipelines, error)) => {
            for pipeline in pipelines {
                unsafe { device.destroy_pipeline(pipeline, None) };
            }
            Err(Error::Vulkan(format!(
                "failed to create compute pipeline: {error:?}"
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{mapping_u32_is_safe, validate_external_planes};
    use crate::{ExternalImageAccess, ExternalPlaneImage, ExternalPlaneKind};
    use asciiflow_core::{ColorSpace, FrameDesc};

    fn plane(
        kind: ExternalPlaneKind,
        width: u32,
        height: u32,
        offset: u64,
        object_size: u64,
    ) -> ExternalPlaneImage {
        ExternalPlaneImage {
            fd: std::fs::File::open("/dev/null").unwrap().into(),
            object_size,
            modifier: 9,
            offset,
            row_pitch: 1_920,
            width,
            height,
            kind,
            access: ExternalImageAccess::Write,
        }
    }

    #[test]
    fn mapping_width_selects_common_path_and_large_cell_fallback() {
        assert!(mapping_u32_is_safe(1920, 1080, 80, 45, 3_110_400, 255));
        assert!(mapping_u32_is_safe(1920, 1080, 80, 45, 6_220_800, 1023));
        assert!(!mapping_u32_is_safe(8192, 4096, 1, 1, 50_331_648, 255));
        assert!(!mapping_u32_is_safe(4096, 4096, 1, 1, 50_331_648, 1023));
    }

    #[test]
    fn external_nv12_planes_require_exact_geometry_and_bounded_ranges() {
        let desc = FrameDesc::host_nv12(1_920, 1_080, ColorSpace::default()).unwrap();
        let object_size = 3_194_880;
        let valid = [
            plane(ExternalPlaneKind::Y, 1_920, 1_080, 0, object_size),
            plane(ExternalPlaneKind::Uv, 960, 540, 2_088_960, object_size),
        ];
        validate_external_planes(
            &desc,
            &valid,
            ExternalImageAccess::Write,
            "output",
            asciiflow_core::PixelFormat::Nv12,
        )
        .unwrap();

        let wrong_geometry = [
            plane(ExternalPlaneKind::Y, 1_919, 1_080, 0, object_size),
            plane(ExternalPlaneKind::Uv, 960, 540, 2_088_960, object_size),
        ];
        assert!(
            validate_external_planes(
                &desc,
                &wrong_geometry,
                ExternalImageAccess::Write,
                "output",
                asciiflow_core::PixelFormat::Nv12
            )
            .unwrap_err()
            .to_string()
            .contains("expected 1920x1080")
        );

        let out_of_bounds = [
            plane(ExternalPlaneKind::Y, 1_920, 1_080, 0, object_size),
            plane(ExternalPlaneKind::Uv, 960, 540, 2_088_960, 3_125_759),
        ];
        assert!(
            validate_external_planes(
                &desc,
                &out_of_bounds,
                ExternalImageAccess::Write,
                "output",
                asciiflow_core::PixelFormat::Nv12
            )
            .unwrap_err()
            .to_string()
            .contains("beyond object size")
        );
    }

    #[test]
    fn external_p010_planes_require_16_bit_geometry_and_bounded_ranges() {
        use asciiflow_core::PixelFormat;

        let desc = FrameDesc::host_p010_le(64, 64, ColorSpace::default()).unwrap();
        let mut valid = [
            plane(ExternalPlaneKind::P010Y, 64, 64, 0, 16_384),
            plane(ExternalPlaneKind::P010Uv, 32, 32, 8_192, 16_384),
        ];
        for image in &mut valid {
            image.row_pitch = 128;
            image.access = ExternalImageAccess::Read;
        }
        validate_external_planes(
            &desc,
            &valid,
            ExternalImageAccess::Read,
            "input",
            PixelFormat::P010Le,
        )
        .unwrap();

        valid[1].row_pitch = 127;
        assert!(
            validate_external_planes(
                &desc,
                &valid,
                ExternalImageAccess::Read,
                "input",
                PixelFormat::P010Le
            )
            .unwrap_err()
            .to_string()
            .contains("row pitch")
        );
        valid[1].row_pitch = 128;
        valid[1].object_size = 12_159;
        assert!(
            validate_external_planes(
                &desc,
                &valid,
                ExternalImageAccess::Read,
                "input",
                PixelFormat::P010Le
            )
            .unwrap_err()
            .to_string()
            .contains("beyond object size")
        );
    }
}
