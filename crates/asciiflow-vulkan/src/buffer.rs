use crate::context::{DeviceInfo, vk_error};
use asciiflow_core::{Error, Result};
use ash::vk;
use gpu_allocator::{
    MemoryLocation,
    vulkan::{Allocation, AllocationCreateDesc, AllocationScheme, Allocator},
};
use std::{
    ptr::NonNull,
    time::{Duration, Instant},
};

#[derive(Clone, Debug)]
pub(crate) struct BufferMemoryInfo {
    pub name: &'static str,
    pub memory_type_index: u32,
    pub heap_index: u32,
    pub property_flags: vk::MemoryPropertyFlags,
}

enum BufferMemory {
    Managed(Allocation),
    Dedicated {
        memory: vk::DeviceMemory,
        mapped_address: usize,
        property_flags: vk::MemoryPropertyFlags,
    },
}

pub(crate) struct ReadTimings {
    pub invalidate: Duration,
    pub copy: Duration,
}

pub(crate) struct Buffer {
    pub handle: vk::Buffer,
    memory: Option<BufferMemory>,
    pub size: vk::DeviceSize,
    non_coherent_atom_size: vk::DeviceSize,
    pub memory_info: BufferMemoryInfo,
}
impl Buffer {
    pub fn new(
        device: &ash::Device,
        allocator: &mut Allocator,
        size: vk::DeviceSize,
        usage: vk::BufferUsageFlags,
        location: MemoryLocation,
        name: &'static str,
        device_info: &DeviceInfo,
    ) -> Result<Self> {
        let info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let handle = unsafe { device.create_buffer(&info, None) }
            .map_err(vk_error("failed to create Vulkan buffer"))?;
        let requirements = unsafe { device.get_buffer_memory_requirements(handle) };
        let allocation = match allocator.allocate(&AllocationCreateDesc {
            name,
            requirements,
            location,
            linear: true,
            allocation_scheme: AllocationScheme::GpuAllocatorManaged,
        }) {
            Ok(value) => value,
            Err(error) => {
                unsafe { device.destroy_buffer(handle, None) };
                return Err(Error::Vulkan(format!("failed to allocate {name}: {error}")));
            }
        };
        let property_flags = allocation.memory_properties();
        let memory_type = device_info.memory_types.iter().find(|memory_type| {
            requirements.memory_type_bits & (1 << memory_type.index) != 0
                && memory_type.property_flags == property_flags
        });
        let Some(memory_type) = memory_type else {
            unsafe { device.destroy_buffer(handle, None) };
            allocator.free(allocation).ok();
            return Err(Error::Vulkan(format!(
                "failed to identify memory type for {name}"
            )));
        };
        let memory_type_index = memory_type.index;
        let heap_index = memory_type.heap_index;
        if let Err(error) =
            unsafe { device.bind_buffer_memory(handle, allocation.memory(), allocation.offset()) }
        {
            unsafe { device.destroy_buffer(handle, None) };
            allocator.free(allocation).ok();
            return Err(Error::Vulkan(format!("failed to bind {name}: {error:?}")));
        }
        Ok(Self {
            handle,
            memory: Some(BufferMemory::Managed(allocation)),
            size,
            non_coherent_atom_size: device_info.non_coherent_atom_size,
            memory_info: BufferMemoryInfo {
                name,
                memory_type_index,
                heap_index,
                property_flags,
            },
        })
    }

    pub fn new_cached(
        device: &ash::Device,
        size: vk::DeviceSize,
        usage: vk::BufferUsageFlags,
        name: &'static str,
        device_info: &DeviceInfo,
    ) -> Result<Option<Self>> {
        let info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let handle = unsafe { device.create_buffer(&info, None) }
            .map_err(vk_error("failed to create Vulkan buffer"))?;
        let requirements = unsafe { device.get_buffer_memory_requirements(handle) };
        let wanted = vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_CACHED;
        let Some(memory_type) = device_info.memory_types.iter().find(|memory_type| {
            requirements.memory_type_bits & (1 << memory_type.index) != 0
                && memory_type.property_flags.contains(wanted)
        }) else {
            unsafe { device.destroy_buffer(handle, None) };
            return Ok(None);
        };
        let mut dedicated = vk::MemoryDedicatedAllocateInfo::default().buffer(handle);
        let allocation_info = vk::MemoryAllocateInfo::default()
            .allocation_size(requirements.size)
            .memory_type_index(memory_type.index)
            .push_next(&mut dedicated);
        let memory = match unsafe { device.allocate_memory(&allocation_info, None) } {
            Ok(memory) => memory,
            Err(error) => {
                unsafe { device.destroy_buffer(handle, None) };
                return Err(vk_error("failed to allocate cached Vulkan memory")(error));
            }
        };
        if let Err(error) = unsafe { device.bind_buffer_memory(handle, memory, 0) } {
            unsafe {
                device.free_memory(memory, None);
                device.destroy_buffer(handle, None);
            }
            return Err(vk_error("failed to bind cached Vulkan memory")(error));
        }
        let mapped_address = match unsafe {
            device.map_memory(memory, 0, vk::WHOLE_SIZE, vk::MemoryMapFlags::empty())
        } {
            Ok(pointer) => NonNull::new(pointer)
                .expect("Vulkan returned a null mapped pointer")
                .as_ptr() as usize,
            Err(error) => {
                unsafe {
                    device.destroy_buffer(handle, None);
                    device.free_memory(memory, None);
                }
                return Err(vk_error("failed to map cached Vulkan memory")(error));
            }
        };
        Ok(Some(Self {
            handle,
            memory: Some(BufferMemory::Dedicated {
                memory,
                mapped_address,
                property_flags: memory_type.property_flags,
            }),
            size,
            non_coherent_atom_size: device_info.non_coherent_atom_size,
            memory_info: BufferMemoryInfo {
                name,
                memory_type_index: memory_type.index,
                heap_index: memory_type.heap_index,
                property_flags: memory_type.property_flags,
            },
        }))
    }
    pub fn write(&mut self, device: &ash::Device, data: &[u8]) -> Result<()> {
        if data.len() as u64 > self.size {
            return Err(Error::Vulkan("staging write exceeds buffer".into()));
        }
        let (pointer, coherent) = self.mapped()?;
        unsafe {
            std::ptr::copy_nonoverlapping(data.as_ptr(), pointer.as_ptr().cast::<u8>(), data.len());
            if !coherent {
                device
                    .flush_mapped_memory_ranges(&[self.mapped_range()])
                    .map_err(vk_error("failed to flush staging memory"))?;
            }
        }
        Ok(())
    }
    pub fn read(&self, device: &ash::Device, length: usize) -> Result<Vec<u8>> {
        if length as u64 > self.size {
            return Err(Error::Vulkan("staging read exceeds buffer".into()));
        }
        let mut bytes = vec![0; length];
        self.read_into(device, &mut bytes)?;
        Ok(bytes)
    }
    pub fn read_into(&self, device: &ash::Device, destination: &mut [u8]) -> Result<ReadTimings> {
        if destination.len() as u64 > self.size {
            return Err(Error::Vulkan("staging read exceeds buffer".into()));
        }
        let (pointer, coherent) = self.mapped()?;
        let invalidate_started = Instant::now();
        if !coherent {
            unsafe { device.invalidate_mapped_memory_ranges(&[self.mapped_range()]) }
                .map_err(vk_error("failed to invalidate readback memory"))?;
        }
        let invalidate = invalidate_started.elapsed();
        let copy_started = Instant::now();
        unsafe {
            std::ptr::copy_nonoverlapping(
                pointer.as_ptr().cast::<u8>(),
                destination.as_mut_ptr(),
                destination.len(),
            );
        }
        Ok(ReadTimings {
            invalidate,
            copy: copy_started.elapsed(),
        })
    }
    fn mapped(&self) -> Result<(NonNull<std::ffi::c_void>, bool)> {
        match self.memory.as_ref().expect("buffer allocation missing") {
            BufferMemory::Managed(allocation) => Ok((
                allocation
                    .mapped_ptr()
                    .ok_or_else(|| Error::Vulkan("buffer is not mapped".into()))?,
                allocation
                    .memory_properties()
                    .contains(vk::MemoryPropertyFlags::HOST_COHERENT),
            )),
            BufferMemory::Dedicated {
                mapped_address,
                property_flags,
                ..
            } => Ok((
                NonNull::new(*mapped_address as *mut std::ffi::c_void)
                    .expect("mapped Vulkan address became null"),
                property_flags.contains(vk::MemoryPropertyFlags::HOST_COHERENT),
            )),
        }
    }
    fn mapped_range(&self) -> vk::MappedMemoryRange<'static> {
        let (memory, allocation_offset) =
            match self.memory.as_ref().expect("buffer allocation missing") {
                BufferMemory::Managed(allocation) => {
                    (unsafe { allocation.memory() }, allocation.offset())
                }
                BufferMemory::Dedicated { memory, .. } => (*memory, 0),
            };
        let offset = allocation_offset / self.non_coherent_atom_size * self.non_coherent_atom_size;
        // SAFETY: the allocation remains owned by this Buffer until destroy(),
        // and the mapped range is used only while the allocation is alive.
        vk::MappedMemoryRange::default()
            .memory(memory)
            .offset(offset)
            .size(vk::WHOLE_SIZE)
    }
    pub fn destroy(&mut self, device: &ash::Device, allocator: &mut Allocator) {
        unsafe { device.destroy_buffer(self.handle, None) };
        if let Some(memory) = self.memory.take() {
            match memory {
                BufferMemory::Managed(allocation) => {
                    // Destruction runs during normal completion, cancellation,
                    // and error unwinding. A cleanup failure must not replace
                    // the original pipeline error with a panic.
                    allocator.free(allocation).ok();
                }
                BufferMemory::Dedicated { memory, .. } => unsafe {
                    device.unmap_memory(memory);
                    device.free_memory(memory, None);
                },
            }
        }
    }
}
