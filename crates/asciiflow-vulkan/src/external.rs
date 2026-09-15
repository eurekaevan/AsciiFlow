use crate::context::{VulkanContext, vk_error};
use asciiflow_core::{Error, Result};
use ash::vk;
use std::{
    os::fd::{AsRawFd, IntoRawFd, OwnedFd},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExternalPlaneKind {
    Y,
    Uv,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExternalImageAccess {
    Read,
    Write,
}

pub struct ExternalPlaneImage {
    pub fd: OwnedFd,
    pub object_size: u64,
    pub modifier: u64,
    pub offset: u64,
    pub row_pitch: u64,
    pub width: u32,
    pub height: u32,
    pub kind: ExternalPlaneKind,
    pub access: ExternalImageAccess,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ExternalImageTimings {
    pub capability_query: Duration,
    pub image_create: Duration,
    pub dma_buf_import: Duration,
    pub memory_bind: Duration,
    pub destroy: Duration,
}

pub(crate) struct ImportedExternalPlane {
    device: ash::Device,
    abandoned: Arc<AtomicBool>,
    pub image: vk::Image,
    memory: vk::DeviceMemory,
    pub kind: ExternalPlaneKind,
    pub width: u32,
    pub height: u32,
}

impl ImportedExternalPlane {
    pub(crate) fn import(
        context: &VulkanContext,
        input: ExternalPlaneImage,
    ) -> Result<(Self, ExternalImageTimings)> {
        if !context.info.dma_buf_interop {
            return Err(Error::Vulkan(
                "Vulkan device lacks the required DMA-BUF, DRM-modifier, external-memory-fd, or foreign-queue extension"
                    .into(),
            ));
        }
        if input.width == 0 || input.height == 0 || input.row_pitch == 0 {
            return Err(Error::Vulkan(
                "external image dimensions and row pitch must be non-zero".into(),
            ));
        }
        if input.offset >= input.object_size {
            return Err(Error::Vulkan(format!(
                "external image offset {} is outside DMA-BUF size {}",
                input.offset, input.object_size
            )));
        }
        let format = match input.kind {
            ExternalPlaneKind::Y => vk::Format::R8_UNORM,
            ExternalPlaneKind::Uv => vk::Format::R8G8_UNORM,
        };
        let query_started = Instant::now();
        require_modifier_support(context, format, input.modifier, input.access)?;
        let capability_query = query_started.elapsed();

        let plane_layout = [vk::SubresourceLayout {
            offset: input.offset,
            size: 0,
            row_pitch: input.row_pitch,
            array_pitch: 0,
            depth_pitch: 0,
        }];
        let mut external_info = vk::ExternalMemoryImageCreateInfo::default()
            .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
        let mut modifier_info = vk::ImageDrmFormatModifierExplicitCreateInfoEXT::default()
            .drm_format_modifier(input.modifier)
            .plane_layouts(&plane_layout);
        let usage = match input.access {
            ExternalImageAccess::Read => vk::ImageUsageFlags::TRANSFER_SRC,
            ExternalImageAccess::Write => vk::ImageUsageFlags::TRANSFER_DST,
        };
        let create_info = vk::ImageCreateInfo::default()
            .push_next(&mut external_info)
            .push_next(&mut modifier_info)
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(vk::Extent3D {
                width: input.width,
                height: input.height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);
        let create_started = Instant::now();
        let image = unsafe { context.device.create_image(&create_info, None) }
            .map_err(vk_error("failed to create DRM-modifier external image"))?;
        let image_create = create_started.elapsed();
        let mut guard = ImportedPlaneGuard::new(&context.device, image);

        let requirements = unsafe { context.device.get_image_memory_requirements(image) };
        let loader = context
            .external_memory_fd
            .as_ref()
            .ok_or_else(|| Error::Vulkan("external-memory-fd loader was not initialized".into()))?;
        let mut fd_properties = vk::MemoryFdPropertiesKHR::default();
        unsafe {
            loader.get_memory_fd_properties(
                vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT,
                input.fd.as_raw_fd(),
                &mut fd_properties,
            )
        }
        .map_err(vk_error("failed to query DMA-BUF memory types"))?;
        let compatible = requirements.memory_type_bits & fd_properties.memory_type_bits;
        let memory_type_index = (0..context.info.memory_types.len() as u32)
            .find(|index| compatible & (1 << index) != 0)
            .ok_or_else(|| {
                Error::Vulkan(format!(
                    "DMA-BUF memory types {:#x} do not intersect image requirements {:#x}",
                    fd_properties.memory_type_bits, requirements.memory_type_bits
                ))
            })?;
        let mut import_info = vk::ImportMemoryFdInfoKHR::default()
            .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT)
            .fd(input.fd.as_raw_fd());
        let mut dedicated_info = vk::MemoryDedicatedAllocateInfo::default().image(image);
        let allocate_info = vk::MemoryAllocateInfo::default()
            .push_next(&mut import_info)
            .push_next(&mut dedicated_info)
            .allocation_size(requirements.size)
            .memory_type_index(memory_type_index);
        let import_started = Instant::now();
        let memory = unsafe { context.device.allocate_memory(&allocate_info, None) }
            .map_err(vk_error("failed to import DMA-BUF memory"))?;
        // Vulkan takes ownership of an imported fd only when allocation succeeds.
        let _owned_by_vulkan = input.fd.into_raw_fd();
        let dma_buf_import = import_started.elapsed();
        guard.memory = memory;
        let bind_started = Instant::now();
        unsafe { context.device.bind_image_memory(image, memory, 0) }
            .map_err(vk_error("failed to bind imported DMA-BUF image memory"))?;
        let memory_bind = bind_started.elapsed();
        guard.disarm();
        Ok((
            Self {
                device: context.device.clone(),
                abandoned: context.abandonment_flag(),
                image,
                memory,
                kind: input.kind,
                width: input.width,
                height: input.height,
            },
            ExternalImageTimings {
                capability_query,
                image_create,
                dma_buf_import,
                memory_bind,
                destroy: Duration::ZERO,
            },
        ))
    }

    pub(crate) fn destroy(mut self) -> Duration {
        let started = Instant::now();
        if self.abandoned.load(Ordering::Acquire) {
            self.image = vk::Image::null();
            self.memory = vk::DeviceMemory::null();
            return started.elapsed();
        }
        unsafe {
            self.device.destroy_image(self.image, None);
            self.device.free_memory(self.memory, None);
        }
        self.image = vk::Image::null();
        self.memory = vk::DeviceMemory::null();
        started.elapsed()
    }
}

impl Drop for ImportedExternalPlane {
    fn drop(&mut self) {
        if self.abandoned.load(Ordering::Acquire) {
            return;
        }
        unsafe {
            self.device.destroy_image(self.image, None);
            self.device.free_memory(self.memory, None);
        }
    }
}

fn require_modifier_support(
    context: &VulkanContext,
    format: vk::Format,
    modifier: u64,
    access: ExternalImageAccess,
) -> Result<()> {
    let mut count_list = vk::DrmFormatModifierPropertiesListEXT::default();
    let mut properties = vk::FormatProperties2::default().push_next(&mut count_list);
    unsafe {
        context.instance.get_physical_device_format_properties2(
            context.physical_device,
            format,
            &mut properties,
        )
    };
    let mut modifiers = vec![
        vk::DrmFormatModifierPropertiesEXT::default();
        count_list.drm_format_modifier_count as usize
    ];
    let mut list = vk::DrmFormatModifierPropertiesListEXT::default()
        .drm_format_modifier_properties(&mut modifiers);
    let mut properties = vk::FormatProperties2::default().push_next(&mut list);
    unsafe {
        context.instance.get_physical_device_format_properties2(
            context.physical_device,
            format,
            &mut properties,
        )
    };
    let modifier_properties = modifiers
        .iter()
        .find(|entry| entry.drm_format_modifier == modifier)
        .ok_or_else(|| {
            Error::Vulkan(format!(
                "Vulkan format {format:?} does not support DRM modifier {modifier:#018x}"
            ))
        })?;
    if modifier_properties.drm_format_modifier_plane_count != 1 {
        return Err(Error::Vulkan(format!(
            "DRM modifier {modifier:#018x} for {format:?} has {} Vulkan memory planes; Stage 3A supports one memory plane per exported layer",
            modifier_properties.drm_format_modifier_plane_count
        )));
    }
    let required_feature = match access {
        ExternalImageAccess::Read => vk::FormatFeatureFlags::TRANSFER_SRC,
        ExternalImageAccess::Write => vk::FormatFeatureFlags::TRANSFER_DST,
    };
    if !modifier_properties
        .drm_format_modifier_tiling_features
        .contains(required_feature)
    {
        return Err(Error::Vulkan(format!(
            "DRM modifier {modifier:#018x} for {format:?} lacks {required_feature:?} support"
        )));
    }

    let mut modifier_info = vk::PhysicalDeviceImageDrmFormatModifierInfoEXT::default()
        .drm_format_modifier(modifier)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);
    let mut external_info = vk::PhysicalDeviceExternalImageFormatInfo::default()
        .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
    let image_info = vk::PhysicalDeviceImageFormatInfo2::default()
        .push_next(&mut modifier_info)
        .push_next(&mut external_info)
        .format(format)
        .ty(vk::ImageType::TYPE_2D)
        .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
        .usage(match access {
            ExternalImageAccess::Read => vk::ImageUsageFlags::TRANSFER_SRC,
            ExternalImageAccess::Write => vk::ImageUsageFlags::TRANSFER_DST,
        });
    let mut external_properties = vk::ExternalImageFormatProperties::default();
    let mut image_properties =
        vk::ImageFormatProperties2::default().push_next(&mut external_properties);
    unsafe {
        context
            .instance
            .get_physical_device_image_format_properties2(
                context.physical_device,
                &image_info,
                &mut image_properties,
            )
    }
    .map_err(vk_error(
        "DRM modifier image is not supported for DMA-BUF import",
    ))?;
    if !external_properties
        .external_memory_properties
        .external_memory_features
        .contains(vk::ExternalMemoryFeatureFlags::IMPORTABLE)
    {
        return Err(Error::Vulkan(format!(
            "DRM modifier {modifier:#018x} for {format:?} is not importable from DMA-BUF"
        )));
    }
    Ok(())
}

struct ImportedPlaneGuard<'a> {
    device: &'a ash::Device,
    image: vk::Image,
    memory: vk::DeviceMemory,
}

impl<'a> ImportedPlaneGuard<'a> {
    fn new(device: &'a ash::Device, image: vk::Image) -> Self {
        Self {
            device,
            image,
            memory: vk::DeviceMemory::null(),
        }
    }

    fn disarm(&mut self) {
        self.image = vk::Image::null();
        self.memory = vk::DeviceMemory::null();
    }
}

impl Drop for ImportedPlaneGuard<'_> {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_image(self.image, None);
            self.device.free_memory(self.memory, None);
        }
    }
}
