//! Qualification-only BT.709 NCL packing. No production dispatch uses this module.
use crate::external::{
    ExternalImageAccess, ExternalPlaneImage, ExternalPlaneKind, ImportedExternalPlane,
};
use crate::{
    buffer::Buffer,
    context::{VulkanContext, vk_error},
};
use asciiflow_core::{Error, Result};
use ash::vk;
use gpu_allocator::{
    MemoryLocation,
    vulkan::{Allocator, AllocatorCreateDesc},
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SdrPackFormat {
    Nv12,
    P010,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SdrPackFault {
    Pipeline,
    Buffer,
    Descriptor,
    Dispatch,
    AfterFence,
    Readback,
    ExternalImport,
    ExternalCopy,
    ExternalAfterFence,
}
#[derive(Debug)]
pub struct SdrPackOutput {
    pub bytes: Vec<u8>,
    /// Pixels with nonfinite, negative, and greater-than-one RGB, respectively.
    pub diagnostics: [u32; 3],
    /// Wall duration of pack recording, dispatch, fence completion and readback.
    pub pack_duration: Duration,
    /// Wall duration of resident buffer copy, including imports and completion.
    pub copy_duration: Duration,
    pub gpu_pack: Duration,
    pub gpu_copy: Duration,
    pub buffer_bytes: u64,
    pub validation_errors: usize,
}
fn checkpoint(fault: Option<SdrPackFault>, point: SdrPackFault) -> Result<()> {
    if fault == Some(point) {
        return Err(Error::Vulkan(format!("SDR pack injected fault: {point:?}")));
    }
    Ok(())
}

pub struct VulkanSdrPackQualification {
    context: Arc<VulkanContext>,
    resources: Option<SdrPackResources>,
}
impl VulkanSdrPackQualification {
    pub fn new() -> Result<Self> {
        Ok(Self {
            context: Arc::new(VulkanContext::new()?),
            resources: None,
        })
    }
    pub fn validation_error_count(&self) -> usize {
        self.context.validation_error_count()
    }
    pub fn process(
        &mut self,
        width: u32,
        height: u32,
        rgb: &[[f32; 3]],
        format: SdrPackFormat,
        fault: Option<SdrPackFault>,
    ) -> Result<SdrPackOutput> {
        if self
            .resources
            .as_ref()
            .is_none_or(|r| !r.matches(width, height, format))
            || matches!(
                fault,
                Some(SdrPackFault::Pipeline | SdrPackFault::Buffer | SdrPackFault::Descriptor)
            )
        {
            self.resources = Some(SdrPackResources::new(
                &self.context,
                width,
                height,
                format,
                fault,
            )?);
        }
        let resources = self.resources.as_mut().expect("prepared");
        if rgb.len() != resources.pixels as usize {
            return Err(Error::Vulkan("SDR pack RGB dimensions mismatch".into()));
        }
        if resources.buffers.len() == 2 {
            resources.buffers.push(Buffer::new(
                &self.context.device,
                resources.allocator.as_mut().expect("allocator"),
                u64::from(resources.pixels) * 12,
                vk::BufferUsageFlags::STORAGE_BUFFER,
                MemoryLocation::CpuToGpu,
                "SDR pack host RGB",
                &self.context.info,
            )?);
        }
        resources.buffers[2].write(&self.context.device, bytemuck::cast_slice(rgb))?;
        resources.process_completed(resources.buffers[2].handle, fault, None)
    }
}

/// The caller keeps the C-3 buffer alive and immutable until this synchronous call
/// returns, and must have completed its producer fence before invoking it.
pub(crate) struct SdrPackResources {
    context: Arc<VulkanContext>,
    allocator: Option<Allocator>,
    buffers: Vec<Buffer>,
    pixels: u32,
    width: u32,
    height: u32,
    bytes: usize,
    words: u32,
    format: SdrPackFormat,
    descriptors: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    pool: vk::CommandPool,
    command: vk::CommandBuffer,
    fence: vk::Fence,
    queries: vk::QueryPool,
    pending: bool,
}
impl SdrPackResources {
    pub(crate) fn matches(&self, width: u32, height: u32, format: SdrPackFormat) -> bool {
        (self.width, self.height, self.format) == (width, height, format)
    }
    pub(crate) fn new(
        context: &Arc<VulkanContext>,
        width: u32,
        height: u32,
        format: SdrPackFormat,
        fault: Option<SdrPackFault>,
    ) -> Result<Self> {
        if context.is_abandoned()
            || width == 0
            || height == 0
            || !width.is_multiple_of(2)
            || !height.is_multiple_of(2)
        {
            return Err(Error::Vulkan(
                "SDR pack requires an active device and nonzero even dimensions".into(),
            ));
        }
        let pixels = width
            .checked_mul(height)
            .filter(|p| *p <= u32::MAX / 3)
            .ok_or_else(|| Error::Vulkan("SDR pack dimensions exceed shader indexing".into()))?;
        let bytes_u64 =
            u64::from(pixels) * 3 / 2 * if format == SdrPackFormat::P010 { 2 } else { 1 };
        let bytes = usize::try_from(bytes_u64)
            .map_err(|_| Error::Vulkan("SDR pack host size overflow".into()))?;
        let words = u32::try_from(bytes_u64.div_ceil(4)).expect("bounded pixels");
        let limits = unsafe {
            context
                .instance
                .get_physical_device_properties(context.physical_device)
        }
        .limits;
        if u64::from(pixels) * 12 > context.info.max_storage_buffer_range
            || u64::from(words) * 4 > context.info.max_storage_buffer_range
            || limits.max_compute_work_group_invocations < 64
            || limits.max_compute_work_group_size[0] < 64
            || words.div_ceil(64) > limits.max_compute_work_group_count[0]
        {
            return Err(Error::Vulkan(
                "SDR pack exceeds Vulkan storage or dispatch limits".into(),
            ));
        }
        let allocator = Allocator::new(&AllocatorCreateDesc {
            instance: context.instance.clone(),
            device: context.device.clone(),
            physical_device: context.physical_device,
            debug_settings: Default::default(),
            buffer_device_address: false,
            allocation_sizes: Default::default(),
        })
        .map_err(|e| Error::Vulkan(format!("SDR pack allocator: {e}")))?;
        let mut r = Self {
            context: context.clone(),
            allocator: Some(allocator),
            buffers: Vec::new(),
            pixels,
            width,
            height,
            bytes,
            words,
            format,
            descriptors: vk::DescriptorSetLayout::null(),
            descriptor_pool: vk::DescriptorPool::null(),
            layout: vk::PipelineLayout::null(),
            pipeline: vk::Pipeline::null(),
            pool: vk::CommandPool::null(),
            command: vk::CommandBuffer::null(),
            fence: vk::Fence::null(),
            queries: vk::QueryPool::null(),
            pending: false,
        };
        checkpoint(fault, SdrPackFault::Buffer)?;
        for (size, name) in [
            (u64::from(words) * 4, "SDR packed output"),
            (12, "SDR pack diagnostics"),
        ] {
            r.buffers.push(Buffer::new(
                &context.device,
                r.allocator.as_mut().expect("allocator"),
                size,
                vk::BufferUsageFlags::STORAGE_BUFFER
                    | vk::BufferUsageFlags::TRANSFER_DST
                    | vk::BufferUsageFlags::TRANSFER_SRC,
                MemoryLocation::GpuToCpu,
                name,
                &context.info,
            )?);
        }
        let device = &context.device;
        let bindings: Vec<_> = (0..3)
            .map(|binding| {
                vk::DescriptorSetLayoutBinding::default()
                    .binding(binding)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE)
            })
            .collect();
        r.descriptors = unsafe {
            device.create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                None,
            )
        }
        .map_err(vk_error("SDR pack descriptor layout"))?;
        checkpoint(fault, SdrPackFault::Descriptor)?;
        r.descriptor_pool = unsafe {
            device.create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::default()
                    .max_sets(1)
                    .pool_sizes(&[vk::DescriptorPoolSize {
                        ty: vk::DescriptorType::STORAGE_BUFFER,
                        descriptor_count: 3,
                    }]),
                None,
            )
        }
        .map_err(vk_error("SDR pack descriptor pool"))?;
        r.layout = unsafe {
            device.create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .set_layouts(&[r.descriptors])
                    .push_constant_ranges(&[vk::PushConstantRange::default()
                        .stage_flags(vk::ShaderStageFlags::COMPUTE)
                        .size(8)]),
                None,
            )
        }
        .map_err(vk_error("SDR pack pipeline layout"))?;
        checkpoint(fault, SdrPackFault::Pipeline)?;
        let shader: &[u8] = match format {
            SdrPackFormat::Nv12 => include_bytes!(concat!(env!("OUT_DIR"), "/sdr_pack_nv12.spv")),
            SdrPackFormat::P010 => include_bytes!(concat!(env!("OUT_DIR"), "/sdr_pack_p010.spv")),
        };
        r.pipeline = crate::backend::create_pipeline(device, r.layout, shader)?;
        r.pool = unsafe {
            device.create_command_pool(
                &vk::CommandPoolCreateInfo::default().queue_family_index(context.info.queue_family),
                None,
            )
        }
        .map_err(vk_error("SDR pack command pool"))?;
        r.command = unsafe {
            device.allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::default()
                    .command_pool(r.pool)
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .command_buffer_count(1),
            )
        }
        .map_err(vk_error("SDR pack command"))?[0];
        r.fence = unsafe { device.create_fence(&vk::FenceCreateInfo::default(), None) }
            .map_err(vk_error("SDR pack fence"))?;
        if context.info.timestamp_valid_bits == 0 {
            return Err(Error::Vulkan(
                "SDR pack qualification requires compute timestamps".into(),
            ));
        }
        r.queries = unsafe {
            device.create_query_pool(
                &vk::QueryPoolCreateInfo::default()
                    .query_type(vk::QueryType::TIMESTAMP)
                    .query_count(4),
                None,
            )
        }
        .map_err(vk_error("SDR pack timestamps"))?;
        Ok(r)
    }
    pub(crate) fn process_completed(
        &mut self,
        rgb: vk::Buffer,
        fault: Option<SdrPackFault>,
        planes: Option<[ExternalPlaneImage; 2]>,
    ) -> Result<SdrPackOutput> {
        self.process_completed_with_readback(rgb, fault, planes, true)
    }

    pub(crate) fn process_completed_with_readback(
        &mut self,
        rgb: vk::Buffer,
        fault: Option<SdrPackFault>,
        planes: Option<[ExternalPlaneImage; 2]>,
        readback: bool,
    ) -> Result<SdrPackOutput> {
        if self.context.is_abandoned() || self.pending || rgb == vk::Buffer::null() {
            return Err(Error::Vulkan(
                "SDR pack device abandoned, slot busy, or null RGB buffer".into(),
            ));
        }
        let started = Instant::now();
        let device = &self.context.device;
        unsafe {
            device
                .reset_command_pool(self.pool, vk::CommandPoolResetFlags::empty())
                .map_err(vk_error("SDR pack reset pool"))?;
            device
                .reset_descriptor_pool(self.descriptor_pool, vk::DescriptorPoolResetFlags::empty())
                .map_err(vk_error("SDR pack reset descriptors"))?;
            device
                .reset_fences(&[self.fence])
                .map_err(vk_error("SDR pack reset fence"))?;
        }
        let set = unsafe {
            device.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(self.descriptor_pool)
                    .set_layouts(&[self.descriptors]),
            )
        }
        .map_err(vk_error("SDR pack descriptor allocate"))?[0];
        let infos = [
            [vk::DescriptorBufferInfo::default()
                .buffer(rgb)
                .range(u64::from(self.pixels) * 12)],
            [vk::DescriptorBufferInfo::default()
                .buffer(self.buffers[0].handle)
                .range(u64::from(self.words) * 4)],
            [vk::DescriptorBufferInfo::default()
                .buffer(self.buffers[1].handle)
                .range(12)],
        ];
        let writes: Vec<_> = infos
            .iter()
            .enumerate()
            .map(|(i, info)| {
                vk::WriteDescriptorSet::default()
                    .dst_set(set)
                    .dst_binding(i as u32)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .buffer_info(info)
            })
            .collect();
        unsafe {
            device.update_descriptor_sets(&writes, &[]);
            device
                .begin_command_buffer(
                    self.command,
                    &vk::CommandBufferBeginInfo::default()
                        .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
                )
                .map_err(vk_error("SDR pack begin"))?;
            device.cmd_fill_buffer(self.command, self.buffers[1].handle, 0, 12, 0);
            let barriers = [vk::MemoryBarrier2::default()
                .src_stage_mask(
                    vk::PipelineStageFlags2::HOST
                        | vk::PipelineStageFlags2::COMPUTE_SHADER
                        | vk::PipelineStageFlags2::TRANSFER,
                )
                .src_access_mask(
                    vk::AccessFlags2::HOST_WRITE
                        | vk::AccessFlags2::SHADER_WRITE
                        | vk::AccessFlags2::TRANSFER_WRITE,
                )
                .dst_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
                .dst_access_mask(vk::AccessFlags2::SHADER_READ | vk::AccessFlags2::SHADER_WRITE)];
            device.cmd_pipeline_barrier2(
                self.command,
                &vk::DependencyInfo::default().memory_barriers(&barriers),
            );
            device.cmd_bind_pipeline(self.command, vk::PipelineBindPoint::COMPUTE, self.pipeline);
            device.cmd_bind_descriptor_sets(
                self.command,
                vk::PipelineBindPoint::COMPUTE,
                self.layout,
                0,
                &[set],
                &[],
            );
            device.cmd_push_constants(
                self.command,
                self.layout,
                vk::ShaderStageFlags::COMPUTE,
                0,
                bytemuck::cast_slice(&[self.width, self.height]),
            );
        }
        checkpoint(fault, SdrPackFault::Dispatch)?;
        unsafe {
            device.cmd_reset_query_pool(self.command, self.queries, 0, 2);
            device.cmd_write_timestamp2(
                self.command,
                vk::PipelineStageFlags2::COMPUTE_SHADER,
                self.queries,
                0,
            );
            device.cmd_dispatch(self.command, self.words.div_ceil(64), 1, 1);
            device.cmd_write_timestamp2(
                self.command,
                vk::PipelineStageFlags2::COMPUTE_SHADER,
                self.queries,
                1,
            );
            let barriers = [vk::MemoryBarrier2::default()
                .src_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
                .src_access_mask(vk::AccessFlags2::SHADER_WRITE)
                .dst_stage_mask(vk::PipelineStageFlags2::HOST)
                .dst_access_mask(vk::AccessFlags2::HOST_READ)];
            device.cmd_pipeline_barrier2(
                self.command,
                &vk::DependencyInfo::default().memory_barriers(&barriers),
            );
            device
                .end_command_buffer(self.command)
                .map_err(vk_error("SDR pack end"))?;
        }
        {
            let _lock = self
                .context
                .queue_submit_lock
                .lock()
                .map_err(|_| Error::Vulkan("SDR pack queue lock poisoned".into()))?;
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
                return Err(vk_error("SDR pack submit")(e));
            }
            self.pending = true;
        }
        self.wait()?;
        checkpoint(fault, SdrPackFault::AfterFence)?;
        let raw = self.buffers[1].read(&self.context.device, 12)?;
        let diagnostics: [u32; 3] = std::array::from_fn(|i| {
            u32::from_le_bytes(raw[i * 4..i * 4 + 4].try_into().expect("counter"))
        });
        if diagnostics.iter().any(|v| *v != 0) {
            return Err(Error::Vulkan(format!(
                "SDR pack invalid RGB diagnostics: {diagnostics:?}"
            )));
        }
        checkpoint(fault, SdrPackFault::Readback)?;
        let bytes = if readback {
            self.buffers[0].read(&self.context.device, self.bytes)?
        } else {
            Vec::new()
        };
        let gpu_pack = self.gpu_duration(0)?;
        let pack_duration = started.elapsed();
        let (copy_duration, gpu_copy) = if let Some(planes) = planes {
            (self.copy_external(planes, fault)?, self.gpu_duration(2)?)
        } else {
            (Duration::ZERO, Duration::ZERO)
        };
        Ok(SdrPackOutput {
            bytes,
            diagnostics,
            pack_duration,
            copy_duration,
            gpu_pack,
            gpu_copy,
            buffer_bytes: self.buffers.iter().map(|b| b.size).sum(),
            validation_errors: self.context.validation_error_count(),
        })
    }
    fn copy_external(
        &mut self,
        planes: [ExternalPlaneImage; 2],
        fault: Option<SdrPackFault>,
    ) -> Result<Duration> {
        let started = Instant::now();
        let kinds = match self.format {
            SdrPackFormat::Nv12 => [ExternalPlaneKind::Y, ExternalPlaneKind::Uv],
            SdrPackFormat::P010 => [ExternalPlaneKind::P010Y, ExternalPlaneKind::P010Uv],
        };
        let sample_bytes = if self.format == SdrPackFormat::P010 {
            2u64
        } else {
            1
        };
        // Validate both descriptors before either import consumes its FD.
        for (i, plane) in planes.iter().enumerate() {
            let (width, height) = if i == 0 {
                (self.width, self.height)
            } else {
                (self.width / 2, self.height / 2)
            };
            let row_bytes = u64::from(self.width) * sample_bytes;
            let end = plane
                .row_pitch
                .checked_mul(u64::from(height - 1))
                .and_then(|v| v.checked_add(row_bytes))
                .and_then(|v| v.checked_add(plane.offset));
            if plane.kind != kinds[i]
                || plane.access != ExternalImageAccess::Write
                || (plane.width, plane.height) != (width, height)
                || plane.row_pitch < row_bytes
                || plane.offset % sample_bytes != 0
                || plane.row_pitch % sample_bytes != 0
                || end.is_none_or(|end| end > plane.object_size)
            {
                return Err(Error::Vulkan(format!(
                    "SDR pack external plane {i} descriptor mismatch"
                )));
            }
        }
        let [y, uv] = planes;
        checkpoint(fault, SdrPackFault::ExternalImport)?;
        let (y, _) = ImportedExternalPlane::import(&self.context, y)?;
        let (uv, _) = ImportedExternalPlane::import(&self.context, uv)?;
        let planes = [&y, &uv];
        let device = &self.context.device;
        unsafe {
            device
                .reset_command_pool(self.pool, vk::CommandPoolResetFlags::empty())
                .map_err(vk_error("SDR copy reset pool"))?;
            device
                .reset_fences(&[self.fence])
                .map_err(vk_error("SDR copy reset fence"))?;
            device
                .begin_command_buffer(
                    self.command,
                    &vk::CommandBufferBeginInfo::default()
                        .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
                )
                .map_err(vk_error("SDR copy begin"))?;
        }
        let range = vk::ImageSubresourceRange::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .level_count(1)
            .layer_count(1);
        let acquire = planes.map(|plane| {
            vk::ImageMemoryBarrier2::default()
                .src_stage_mask(vk::PipelineStageFlags2::NONE)
                .src_access_mask(vk::AccessFlags2::NONE)
                .dst_stage_mask(vk::PipelineStageFlags2::COPY)
                .dst_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
                .old_layout(vk::ImageLayout::GENERAL)
                .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_FOREIGN_EXT)
                .dst_queue_family_index(self.context.info.queue_family)
                .image(plane.image)
                .subresource_range(range)
        });
        let buffer = [vk::BufferMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
            .src_access_mask(vk::AccessFlags2::SHADER_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::COPY)
            .dst_access_mask(vk::AccessFlags2::TRANSFER_READ)
            .buffer(self.buffers[0].handle)
            .offset(0)
            .size(vk::WHOLE_SIZE)];
        unsafe {
            device.cmd_reset_query_pool(self.command, self.queries, 2, 2);
            device.cmd_pipeline_barrier2(
                self.command,
                &vk::DependencyInfo::default()
                    .buffer_memory_barriers(&buffer)
                    .image_memory_barriers(&acquire),
            );
        }
        checkpoint(fault, SdrPackFault::ExternalCopy)?;
        unsafe {
            device.cmd_write_timestamp2(
                self.command,
                vk::PipelineStageFlags2::COPY,
                self.queries,
                2,
            );
            for (i, plane) in planes.iter().enumerate() {
                device.cmd_copy_buffer_to_image(
                    self.command,
                    self.buffers[0].handle,
                    plane.image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[vk::BufferImageCopy::default()
                        .buffer_offset(if i == 0 {
                            0
                        } else {
                            u64::from(self.pixels) * sample_bytes
                        })
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
            device.cmd_write_timestamp2(
                self.command,
                vk::PipelineStageFlags2::COPY,
                self.queries,
                3,
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
                .src_queue_family_index(self.context.info.queue_family)
                .dst_queue_family_index(vk::QUEUE_FAMILY_FOREIGN_EXT)
                .image(plane.image)
                .subresource_range(range)
        });
        unsafe {
            device.cmd_pipeline_barrier2(
                self.command,
                &vk::DependencyInfo::default().image_memory_barriers(&release),
            );
            device
                .end_command_buffer(self.command)
                .map_err(vk_error("SDR copy end"))?;
        }
        {
            let _lock = self
                .context
                .queue_submit_lock
                .lock()
                .map_err(|_| Error::Vulkan("SDR copy queue lock poisoned".into()))?;
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
                return Err(vk_error("SDR copy submit")(e));
            }
            self.pending = true;
        }
        // Imported planes consult the same abandonment flag, so uncertain fence
        // completion also retains their in-flight image and memory allocations.
        self.wait()?;
        checkpoint(fault, SdrPackFault::ExternalAfterFence)?;
        Ok(started.elapsed())
    }
    fn gpu_duration(&self, first: u32) -> Result<Duration> {
        let mut values = [0u64; 2];
        unsafe {
            self.context.device.get_query_pool_results(
                self.queries,
                first,
                &mut values,
                vk::QueryResultFlags::TYPE_64 | vk::QueryResultFlags::WAIT,
            )
        }
        .map_err(vk_error("SDR pack timestamp readback"))?;
        let bits = self.context.info.timestamp_valid_bits;
        let mask = if bits == 64 {
            u64::MAX
        } else {
            (1u64 << bits) - 1
        };
        Ok(Duration::from_secs_f64(
            (values[1].wrapping_sub(values[0]) & mask) as f64
                * f64::from(self.context.info.timestamp_period_ns)
                / 1e9,
        ))
    }
    fn wait(&mut self) -> Result<()> {
        if let Err(e) = unsafe {
            self.context
                .device
                .wait_for_fences(&[self.fence], true, 5_000_000_000)
        } {
            self.context.abandon();
            return Err(vk_error("SDR pack fence (device abandoned)")(e));
        }
        self.pending = false;
        Ok(())
    }
}
impl Drop for SdrPackResources {
    fn drop(&mut self) {
        if self.pending {
            let _ = self.wait();
        }
        if self.context.is_abandoned() {
            std::mem::forget(std::mem::take(&mut self.buffers));
            if let Some(allocator) = self.allocator.take() {
                std::mem::forget(allocator);
            }
            return;
        }
        let device = &self.context.device;
        unsafe {
            device.destroy_query_pool(self.queries, None);
            device.destroy_fence(self.fence, None);
            device.destroy_command_pool(self.pool, None);
            device.destroy_pipeline(self.pipeline, None);
            device.destroy_pipeline_layout(self.layout, None);
            device.destroy_descriptor_pool(self.descriptor_pool, None);
            device.destroy_descriptor_set_layout(self.descriptors, None);
        }
        if let Some(allocator) = self.allocator.as_mut() {
            for buffer in &mut self.buffers {
                buffer.destroy(device, allocator);
            }
        }
        // The allocator owns Vulkan memory blocks even after all Buffer
        // allocations are freed. Destroy those blocks before the last context
        // Arc can release the device during automatic field destruction.
        drop(self.allocator.take());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn f64_reference(width: u32, height: u32, rgb: &[[f32; 3]], format: SdrPackFormat) -> Vec<u8> {
        use asciiflow_cpu::sdr_output::{NonlinearBt709Rgb, pack};
        let pixels: Vec<_> = rgb
            .iter()
            .map(|v| NonlinearBt709Rgb(v.map(f64::from)))
            .collect();
        let format = match format {
            SdrPackFormat::Nv12 => asciiflow_core::PixelFormat::Nv12,
            SdrPackFormat::P010 => asciiflow_core::PixelFormat::P010Le,
        };
        pack(width, height, &pixels, format).unwrap()
    }
    #[test]
    #[ignore = "requires Vulkan 1.3; run explicitly on the qualification device"]
    fn sdr_pack_exact_f64_parity_reuse_and_faults() {
        let mut packer = VulkanSdrPackQualification::new().unwrap();
        let colors = [
            [0., 0., 0.],
            [1., 1., 1.],
            [1., 0., 0.],
            [0., 1., 0.],
            [0., 0., 1.],
            [0.5, 0.5, 0.5],
            [0.01, 0.9, 0.2],
            [0.99, 0.2, 0.7],
        ];
        for format in [SdrPackFormat::Nv12, SdrPackFormat::P010] {
            for (width, height) in [(2, 2), (6, 4), (64, 48)] {
                let rgb: Vec<_> = (0..width * height)
                    .map(|i| colors[i as usize % colors.len()])
                    .collect();
                let expected = f64_reference(width, height, &rgb, format);
                for _ in 0..3 {
                    let got = packer.process(width, height, &rgb, format, None).unwrap();
                    assert_eq!(got.bytes, expected);
                    assert_eq!(got.diagnostics, [0; 3]);
                }
            }
            for fault in [
                SdrPackFault::Buffer,
                SdrPackFault::Descriptor,
                SdrPackFault::Pipeline,
                SdrPackFault::Dispatch,
                SdrPackFault::AfterFence,
                SdrPackFault::Readback,
            ] {
                assert!(
                    packer
                        .process(2, 2, &colors[..4], format, Some(fault))
                        .is_err()
                );
                assert_eq!(
                    packer
                        .process(2, 2, &colors[..4], format, None)
                        .unwrap()
                        .bytes,
                    f64_reference(2, 2, &colors[..4], format)
                );
            }
            for value in [f32::NAN, f32::INFINITY, -0.001, 1.001] {
                let bad = [[value, 0.5, 0.5]; 4];
                let error = packer
                    .process(2, 2, &bad, format, None)
                    .unwrap_err()
                    .to_string();
                assert!(error.contains("invalid RGB diagnostics"), "{error}");
                assert!(packer.process(2, 2, &colors[..4], format, None).is_ok());
            }
        }
        assert_eq!(packer.validation_error_count(), 0);
    }
}
