use asciiflow_core::{Error, Result};
use ash::{Entry, vk};
use std::{
    ffi::{CStr, c_void},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

#[derive(Clone, Debug)]
pub struct DeviceInfo {
    pub name: String,
    pub vendor_id: u32,
    pub device_id: u32,
    pub device_type: vk::PhysicalDeviceType,
    pub api_version: u32,
    pub driver_version: u32,
    pub queue_family: u32,
    pub timestamp_valid_bits: u32,
    pub timestamp_period_ns: f32,
    pub non_coherent_atom_size: vk::DeviceSize,
    pub max_storage_buffer_range: vk::DeviceSize,
    pub max_compute_work_group_invocations: u32,
    pub max_compute_work_group_size: [u32; 3],
    pub dma_buf_interop: bool,
    pub memory_heaps: Vec<MemoryHeapInfo>,
    pub memory_types: Vec<MemoryTypeInfo>,
}

#[derive(Clone, Debug)]
pub struct MemoryHeapInfo {
    pub index: u32,
    pub size: vk::DeviceSize,
    pub flags: vk::MemoryHeapFlags,
}

#[derive(Clone, Debug)]
pub struct MemoryTypeInfo {
    pub index: u32,
    pub heap_index: u32,
    pub property_flags: vk::MemoryPropertyFlags,
}

impl DeviceInfo {
    pub fn api_version_string(&self) -> String {
        format!(
            "{}.{}.{}",
            vk::api_version_major(self.api_version),
            vk::api_version_minor(self.api_version),
            vk::api_version_patch(self.api_version)
        )
    }

    pub fn planner_device_kind(&self) -> asciiflow_core::VulkanDeviceKind {
        match self.device_type {
            vk::PhysicalDeviceType::DISCRETE_GPU => asciiflow_core::VulkanDeviceKind::DiscreteGpu,
            vk::PhysicalDeviceType::INTEGRATED_GPU => {
                asciiflow_core::VulkanDeviceKind::IntegratedGpu
            }
            vk::PhysicalDeviceType::VIRTUAL_GPU => asciiflow_core::VulkanDeviceKind::VirtualGpu,
            vk::PhysicalDeviceType::CPU => asciiflow_core::VulkanDeviceKind::Cpu,
            _ => asciiflow_core::VulkanDeviceKind::Other,
        }
    }

    pub fn auto_eligible(&self) -> bool {
        self.device_type != vk::PhysicalDeviceType::CPU
    }
}

pub(crate) struct VulkanContext {
    pub entry: Entry,
    pub instance: ash::Instance,
    pub physical_device: vk::PhysicalDevice,
    pub device: ash::Device,
    pub queue: vk::Queue,
    pub queue_submit_lock: Mutex<()>,
    pub external_memory_fd: Option<ash::khr::external_memory_fd::Device>,
    pub info: DeviceInfo,
    debug_utils: Option<ash::ext::debug_utils::Instance>,
    debug_messenger: vk::DebugUtilsMessengerEXT,
    validation_errors: Option<Box<AtomicUsize>>,
    abandoned: Arc<AtomicBool>,
}

struct InstanceOwner {
    entry: Option<Entry>,
    instance: Option<ash::Instance>,
    debug_utils: Option<ash::ext::debug_utils::Instance>,
    debug_messenger: vk::DebugUtilsMessengerEXT,
    validation_errors: Option<Box<AtomicUsize>>,
}

#[derive(Clone)]
struct DeviceCandidate {
    physical_device: vk::PhysicalDevice,
    info: DeviceInfo,
}

impl InstanceOwner {
    fn new(entry: Entry, instance: ash::Instance) -> Self {
        Self {
            entry: Some(entry),
            instance: Some(instance),
            debug_utils: None,
            debug_messenger: vk::DebugUtilsMessengerEXT::null(),
            validation_errors: None,
        }
    }

    fn into_parts(
        mut self,
    ) -> (
        Entry,
        ash::Instance,
        Option<ash::ext::debug_utils::Instance>,
        vk::DebugUtilsMessengerEXT,
        Option<Box<AtomicUsize>>,
    ) {
        let entry = self.entry.take().expect("Vulkan entry missing");
        let instance = self.instance.take().expect("Vulkan instance missing");
        let debug_utils = self.debug_utils.take();
        let debug_messenger = std::mem::replace(
            &mut self.debug_messenger,
            vk::DebugUtilsMessengerEXT::null(),
        );
        let validation_errors = self.validation_errors.take();
        (
            entry,
            instance,
            debug_utils,
            debug_messenger,
            validation_errors,
        )
    }
}

impl Drop for InstanceOwner {
    fn drop(&mut self) {
        unsafe {
            if let Some(loader) = &self.debug_utils {
                loader.destroy_debug_utils_messenger(self.debug_messenger, None);
            }
            if let Some(instance) = &self.instance {
                instance.destroy_instance(None);
            }
        }
    }
}

pub fn enumerate_devices() -> Result<Vec<DeviceInfo>> {
    let entry = unsafe { Entry::load() }
        .map_err(|error| Error::Vulkan(format!("failed to load system Vulkan loader: {error}")))?;
    let app = application_info();
    let instance = unsafe {
        entry.create_instance(
            &vk::InstanceCreateInfo::default().application_info(&app),
            None,
        )
    }
    .map_err(vk_error("failed to create Vulkan 1.3 instance"))?;
    let result = collect_devices(&instance);
    unsafe { instance.destroy_instance(None) };
    result
}

impl VulkanContext {
    pub fn new() -> Result<Self> {
        let validation =
            std::env::var_os("ASCIIFLOW_VULKAN_VALIDATION").is_some_and(|value| value != "0");
        Self::create(validation)
    }

    fn create(validation: bool) -> Result<Self> {
        let entry = unsafe { Entry::load() }
            .map_err(|e| Error::Vulkan(format!("failed to load system Vulkan loader: {e}")))?;
        let app = application_info();
        let layer_name = c"VK_LAYER_KHRONOS_validation";
        if validation {
            let layers = unsafe { entry.enumerate_instance_layer_properties() }
                .map_err(vk_error("failed to enumerate Vulkan layers"))?;
            if !layers
                .iter()
                .any(|layer| unsafe { CStr::from_ptr(layer.layer_name.as_ptr()) } == layer_name)
            {
                return Err(Error::Vulkan("ASCIIFLOW_VULKAN_VALIDATION=1 but VK_LAYER_KHRONOS_validation is not installed".into()));
            }
        }
        let layer_names = if validation {
            vec![layer_name.as_ptr()]
        } else {
            Vec::new()
        };
        let extensions = if validation {
            vec![ash::ext::debug_utils::NAME.as_ptr()]
        } else {
            Vec::new()
        };
        let enabled_validation_features =
            [vk::ValidationFeatureEnableEXT::SYNCHRONIZATION_VALIDATION];
        let mut validation_features = vk::ValidationFeaturesEXT::default()
            .enabled_validation_features(&enabled_validation_features);
        let mut create_info = vk::InstanceCreateInfo::default()
            .application_info(&app)
            .enabled_layer_names(&layer_names)
            .enabled_extension_names(&extensions);
        if validation {
            create_info = create_info.push_next(&mut validation_features);
        }
        let instance = unsafe { entry.create_instance(&create_info, None) }
            .map_err(vk_error("failed to create Vulkan 1.3 instance"))?;
        let mut owner = InstanceOwner::new(entry, instance);
        if validation {
            owner.validation_errors = Some(Box::new(AtomicUsize::new(0)));
            let error_counter = owner
                .validation_errors
                .as_ref()
                .expect("validation counter missing")
                .as_ref() as *const AtomicUsize as *mut c_void;
            let loader = ash::ext::debug_utils::Instance::new(
                owner.entry.as_ref().expect("Vulkan entry missing"),
                owner.instance.as_ref().expect("Vulkan instance missing"),
            );
            let info = vk::DebugUtilsMessengerCreateInfoEXT::default()
                .message_severity(
                    vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                        | vk::DebugUtilsMessageSeverityFlagsEXT::ERROR,
                )
                .message_type(
                    vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                        | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                        | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
                )
                .pfn_user_callback(Some(debug_callback))
                .user_data(error_counter);
            let messenger = unsafe { loader.create_debug_utils_messenger(&info, None) }
                .map_err(vk_error("failed to create Vulkan validation messenger"))?;
            owner.debug_utils = Some(loader);
            owner.debug_messenger = messenger;
        }

        let candidates =
            collect_device_candidates(owner.instance.as_ref().expect("Vulkan instance missing"))?;
        let allow_cpu =
            std::env::var_os("ASCIIFLOW_VULKAN_ALLOW_CPU").is_some_and(|value| value != "0");
        let selected = candidates
            .into_iter()
            .filter(|candidate| {
                allow_cpu || candidate.info.device_type != vk::PhysicalDeviceType::CPU
            })
            .max_by_key(|candidate| device_score(candidate.info.device_type))
            .ok_or_else(|| Error::Vulkan("no suitable compute-capable Vulkan device was found (CPU Vulkan devices require ASCIIFLOW_VULKAN_ALLOW_CPU=1)".into()))?;
        let physical_device = selected.physical_device;
        let selected = selected.info;
        let priority = [1.0f32];
        let queue_info = [vk::DeviceQueueCreateInfo::default()
            .queue_family_index(selected.queue_family)
            .queue_priorities(&priority)];
        let features = vk::PhysicalDeviceFeatures::default().shader_int64(true);
        let mut features12 =
            vk::PhysicalDeviceVulkan12Features::default().storage_buffer8_bit_access(true);
        let mut features13 = vk::PhysicalDeviceVulkan13Features::default().synchronization2(true);
        let extension_names = if selected.dma_buf_interop {
            vec![
                ash::khr::external_memory_fd::NAME.as_ptr(),
                ash::ext::external_memory_dma_buf::NAME.as_ptr(),
                ash::ext::image_drm_format_modifier::NAME.as_ptr(),
                ash::ext::queue_family_foreign::NAME.as_ptr(),
            ]
        } else {
            Vec::new()
        };
        let device_info = vk::DeviceCreateInfo::default()
            .queue_create_infos(&queue_info)
            .enabled_extension_names(&extension_names)
            .enabled_features(&features)
            .push_next(&mut features12)
            .push_next(&mut features13);
        let device = unsafe {
            owner
                .instance
                .as_ref()
                .expect("Vulkan instance missing")
                .create_device(physical_device, &device_info, None)
        }
        .map_err(vk_error("failed to create Vulkan compute device"))?;
        let queue = unsafe { device.get_device_queue(selected.queue_family, 0) };
        let external_memory_fd = selected.dma_buf_interop.then(|| {
            ash::khr::external_memory_fd::Device::new(
                owner.instance.as_ref().expect("Vulkan instance missing"),
                &device,
            )
        });
        let (entry, instance, debug_utils, debug_messenger, validation_errors) = owner.into_parts();
        Ok(Self {
            entry,
            instance,
            physical_device,
            device,
            queue,
            queue_submit_lock: Mutex::new(()),
            external_memory_fd,
            info: selected,
            debug_utils,
            debug_messenger,
            validation_errors,
            abandoned: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn validation_error_count(&self) -> usize {
        self.validation_errors
            .as_ref()
            .map_or(0, |counter| counter.load(Ordering::Relaxed))
    }

    pub(crate) fn abandon(&self) {
        self.abandoned.store(true, Ordering::Release);
    }

    pub(crate) fn is_abandoned(&self) -> bool {
        self.abandoned.load(Ordering::Acquire)
    }

    pub(crate) fn abandonment_flag(&self) -> Arc<AtomicBool> {
        self.abandoned.clone()
    }
}

impl Drop for VulkanContext {
    fn drop(&mut self) {
        if self.is_abandoned() {
            // The driver did not confirm that submitted work completed. Leaking
            // the device is safer than destroying resources that may still be
            // referenced by the GPU. Process exit reclaims the kernel objects.
            return;
        }
        unsafe {
            let _ = &self.entry;
            self.device.destroy_device(None);
            if let Some(loader) = &self.debug_utils {
                loader.destroy_debug_utils_messenger(self.debug_messenger, None);
            }
            self.instance.destroy_instance(None);
        }
    }
}

fn collect_devices(instance: &ash::Instance) -> Result<Vec<DeviceInfo>> {
    Ok(collect_device_candidates(instance)?
        .into_iter()
        .map(|candidate| candidate.info)
        .collect())
}

fn collect_device_candidates(instance: &ash::Instance) -> Result<Vec<DeviceCandidate>> {
    let physical_devices = unsafe { instance.enumerate_physical_devices() }
        .map_err(vk_error("failed to enumerate Vulkan physical devices"))?;
    let mut result = Vec::new();
    for physical in physical_devices {
        let properties = unsafe { instance.get_physical_device_properties(physical) };
        let extensions = unsafe { instance.enumerate_device_extension_properties(physical) }
            .map_err(vk_error("failed to enumerate Vulkan device extensions"))?;
        let has_extension = |expected: &CStr| {
            extensions.iter().any(|extension| unsafe {
                CStr::from_ptr(extension.extension_name.as_ptr()) == expected
            })
        };
        let dma_buf_interop = [
            ash::khr::external_memory_fd::NAME,
            ash::ext::external_memory_dma_buf::NAME,
            ash::ext::image_drm_format_modifier::NAME,
            ash::ext::queue_family_foreign::NAME,
        ]
        .into_iter()
        .all(has_extension);
        let memory = unsafe { instance.get_physical_device_memory_properties(physical) };
        if properties.api_version < vk::API_VERSION_1_3 {
            continue;
        }
        let mut f12 = vk::PhysicalDeviceVulkan12Features::default();
        let mut f13 = vk::PhysicalDeviceVulkan13Features::default();
        let mut features = vk::PhysicalDeviceFeatures2::default()
            .push_next(&mut f12)
            .push_next(&mut f13);
        unsafe { instance.get_physical_device_features2(physical, &mut features) };
        if features.features.shader_int64 == 0
            || f12.storage_buffer8_bit_access == 0
            || f13.synchronization2 == 0
        {
            continue;
        }
        let queues = unsafe { instance.get_physical_device_queue_family_properties(physical) };
        let Some((queue_family, queue_properties)) = queues.iter().enumerate().find(|(_, q)| {
            q.queue_flags.contains(vk::QueueFlags::COMPUTE) && q.timestamp_valid_bits > 0
        }) else {
            continue;
        };
        let name = unsafe { CStr::from_ptr(properties.device_name.as_ptr()) }
            .to_string_lossy()
            .into_owned();
        result.push(DeviceCandidate {
            physical_device: physical,
            info: DeviceInfo {
                name,
                vendor_id: properties.vendor_id,
                device_id: properties.device_id,
                device_type: properties.device_type,
                api_version: properties.api_version,
                driver_version: properties.driver_version,
                queue_family: queue_family as u32,
                timestamp_valid_bits: queue_properties.timestamp_valid_bits,
                timestamp_period_ns: properties.limits.timestamp_period,
                non_coherent_atom_size: properties.limits.non_coherent_atom_size,
                max_storage_buffer_range: properties.limits.max_storage_buffer_range as u64,
                max_compute_work_group_invocations: properties
                    .limits
                    .max_compute_work_group_invocations,
                max_compute_work_group_size: properties.limits.max_compute_work_group_size,
                dma_buf_interop,
                memory_heaps: memory
                    .memory_heaps_as_slice()
                    .iter()
                    .enumerate()
                    .map(|(index, heap)| MemoryHeapInfo {
                        index: index as u32,
                        size: heap.size,
                        flags: heap.flags,
                    })
                    .collect(),
                memory_types: memory
                    .memory_types_as_slice()
                    .iter()
                    .enumerate()
                    .map(|(index, memory_type)| MemoryTypeInfo {
                        index: index as u32,
                        heap_index: memory_type.heap_index,
                        property_flags: memory_type.property_flags,
                    })
                    .collect(),
            },
        });
    }
    Ok(result)
}

fn application_info<'a>() -> vk::ApplicationInfo<'a> {
    vk::ApplicationInfo::default()
        .application_name(c"AsciiFlow")
        .application_version(vk::make_api_version(0, 2, 0, 0))
        .engine_name(c"AsciiFlow")
        .engine_version(vk::make_api_version(0, 2, 0, 0))
        .api_version(vk::API_VERSION_1_3)
}

fn device_score(kind: vk::PhysicalDeviceType) -> u8 {
    match kind {
        vk::PhysicalDeviceType::DISCRETE_GPU => 4,
        vk::PhysicalDeviceType::INTEGRATED_GPU => 3,
        vk::PhysicalDeviceType::VIRTUAL_GPU => 2,
        vk::PhysicalDeviceType::CPU => 1,
        _ => 0,
    }
}
pub(crate) fn vk_error(context: &'static str) -> impl FnOnce(vk::Result) -> Error {
    move |error| {
        if error == vk::Result::ERROR_DEVICE_LOST {
            Error::DeviceLost(format!("{context}: {error:?}"))
        } else {
            Error::Vulkan(format!("{context}: {error:?}"))
        }
    }
}

unsafe extern "system" fn debug_callback(
    severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    _kind: vk::DebugUtilsMessageTypeFlagsEXT,
    data: *const vk::DebugUtilsMessengerCallbackDataEXT<'_>,
    user: *mut c_void,
) -> vk::Bool32 {
    if severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::ERROR) && !user.is_null() {
        unsafe { &*(user.cast::<AtomicUsize>()) }.fetch_add(1, Ordering::Relaxed);
    }
    let message = if data.is_null() {
        c"<no validation message>"
    } else {
        unsafe { CStr::from_ptr((*data).p_message) }
    };
    eprintln!("Vulkan validation: {}", message.to_string_lossy());
    vk::FALSE
}
