use std::sync::Arc;

use ash::vk;
use wgpu::hal::api::Vulkan as VkApi;

use super::{
    DmaBufError, DmaBufFrame, Nv12DmaBufDescriptor,
    missing_required_vulkan_device_extension, nv12, required_wgpu_features,
};

pub(crate) struct DmaBufInterop {
    pub(super) device: wgpu::Device,
    pub(super) vulkan: Arc<VulkanDmaBufDevice>,
}

pub(super) struct VulkanDmaBufDevice {
    pub(super) device: ash::Device,
    pub(super) external_memory_fd: ash::khr::external_memory_fd::Device,
    pub(super) external_semaphore_fd: ash::khr::external_semaphore_fd::Device,
    pub(super) instance: ash::Instance,
    pub(super) memory_properties: vk::PhysicalDeviceMemoryProperties,
    pub(super) nv12_modifiers: Box<[vk::DrmFormatModifierProperties2EXT]>,
    pub(super) physical_device: vk::PhysicalDevice,
}

impl DmaBufInterop {
    pub(crate) fn new(device: &wgpu::Device) -> Result<Self, DmaBufError> {
        let missing_features = required_wgpu_features().difference(device.features());
        if !missing_features.is_empty() {
            return Err(DmaBufError::UnsupportedDevice(format!(
                "NV12 DMA-BUF interop requires wgpu features {missing_features:?}"
            )));
        }

        let hal_device_guard = unsafe {
            device.as_hal::<VkApi>().ok_or_else(|| {
                DmaBufError::UnsupportedDevice(
                    "NV12 DMA-BUF interop requires a Vulkan wgpu device".into(),
                )
            })?
        };
        let hal_device = &*hal_device_guard;
        let enabled_extensions = hal_device.enabled_device_extensions();
        let vk_device = hal_device.raw_device().clone();
        let instance = hal_device.shared_instance().raw_instance().clone();
        let physical_device = hal_device.raw_physical_device();
        let vulkan = VulkanDmaBufDevice {
            external_memory_fd: ash::khr::external_memory_fd::Device::new(
                &instance, &vk_device,
            ),
            external_semaphore_fd: ash::khr::external_semaphore_fd::Device::new(
                &instance, &vk_device,
            ),
            memory_properties: unsafe {
                instance.get_physical_device_memory_properties(physical_device)
            },
            nv12_modifiers: nv12::nv12_modifier_properties(&instance, physical_device)
                .into_boxed_slice(),
            device: vk_device,
            instance,
            physical_device,
        };
        if let Some(extension) = missing_required_vulkan_device_extension(|extension| {
            enabled_extensions.contains(&extension)
        }) {
            return Err(DmaBufError::UnsupportedDevice(format!(
                "NV12 DMA-BUF interop requires Vulkan device extension {}",
                extension.to_string_lossy()
            )));
        }
        vulkan.validate_sync_file_support()?;

        Ok(Self { device: device.clone(), vulkan: Arc::new(vulkan) })
    }

    pub(crate) fn import_nv12_texture(
        &self,
        descriptor: Nv12DmaBufDescriptor,
    ) -> Result<Arc<DmaBufFrame>, DmaBufError> {
        nv12::import_nv12_dmabuf_texture(self, descriptor)
    }
}

impl VulkanDmaBufDevice {
    fn validate_sync_file_support(&self) -> Result<(), DmaBufError> {
        let mut properties = vk::ExternalSemaphoreProperties::default();
        let info = vk::PhysicalDeviceExternalSemaphoreInfo::default()
            .handle_type(vk::ExternalSemaphoreHandleTypeFlags::SYNC_FD);
        unsafe {
            self.instance.get_physical_device_external_semaphore_properties(
                self.physical_device,
                &info,
                &mut properties,
            );
        }
        let features = properties.external_semaphore_features;
        if !features.contains(
            vk::ExternalSemaphoreFeatureFlags::IMPORTABLE
                | vk::ExternalSemaphoreFeatureFlags::EXPORTABLE,
        ) {
            return Err(DmaBufError::UnsupportedDevice(
                "NV12 DMA-BUF interop requires importable/exportable Vulkan SYNC_FD semaphores"
                    .into(),
            ));
        }

        Ok(())
    }
}
