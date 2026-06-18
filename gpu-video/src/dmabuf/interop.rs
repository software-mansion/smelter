use ash::vk;
use wgpu::hal::api::Vulkan as VkApi;

use std::sync::Arc;

use super::{
    DmaBufError, missing_required_sync_vulkan_device_extension, required_wgpu_features,
};

pub(crate) struct DmaBufInterop {
    pub(super) vulkan: Arc<VulkanDmaBufDevice>,
}

pub(super) struct VulkanDmaBufDevice {
    pub(super) device: ash::Device,
    pub(super) external_semaphore_fd: ash::khr::external_semaphore_fd::Device,
    pub(super) instance: ash::Instance,
    pub(super) physical_device: vk::PhysicalDevice,
}

impl DmaBufInterop {
    pub(crate) fn new(device: &wgpu::Device) -> Result<Self, DmaBufError> {
        let missing_features = required_wgpu_features().difference(device.features());
        if !missing_features.is_empty() {
            return Err(DmaBufError::UnsupportedDevice(format!(
                "Quick Sync DMA-BUF interop requires wgpu features {missing_features:?}"
            )));
        }

        let hal_device_guard = unsafe {
            device.as_hal::<VkApi>().ok_or_else(|| {
                DmaBufError::UnsupportedDevice(
                    "Quick Sync DMA-BUF interop requires a Vulkan wgpu device".into(),
                )
            })?
        };
        let hal_device = &*hal_device_guard;
        let enabled_extensions = hal_device.enabled_device_extensions();
        let vk_device = hal_device.raw_device().clone();
        let instance = hal_device.shared_instance().raw_instance().clone();
        let physical_device = hal_device.raw_physical_device();
        let vulkan = VulkanDmaBufDevice {
            external_semaphore_fd: ash::khr::external_semaphore_fd::Device::new(
                &instance, &vk_device,
            ),
            device: vk_device,
            instance,
            physical_device,
        };
        if let Some(extension) = missing_required_sync_vulkan_device_extension(|extension| {
            enabled_extensions.contains(&extension)
        }) {
            return Err(DmaBufError::UnsupportedDevice(format!(
                "Quick Sync DMA-BUF sync requires Vulkan device extension {}",
                extension.to_string_lossy()
            )));
        }
        vulkan.validate_sync_file_support()?;

        Ok(Self { vulkan: Arc::new(vulkan) })
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
                "Quick Sync DMA-BUF interop requires importable/exportable Vulkan SYNC_FD semaphores"
                    .into(),
            ));
        }

        Ok(())
    }
}
