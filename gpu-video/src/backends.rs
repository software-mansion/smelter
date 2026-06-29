use std::sync::Arc;

use crate::{
    VideoInstanceInitError,
    instance::{VideoInstanceBackend, VideoInstanceDescriptor},
};

#[cfg(vulkan)]
pub mod vulkan;

pub(crate) trait CoreBackend: Send + Sync {
    fn new_instance(
        &self,
        desc: &VideoInstanceDescriptor,
    ) -> Result<Arc<dyn VideoInstanceBackend>, VideoInstanceInitError>;
}

#[cfg(feature = "wgpu")]
pub(crate) trait WgpuBackend: CoreBackend {
    fn device_key_from_wgpu_device(
        &self,
        device: &wgpu::Device,
    ) -> crate::global_registry::VideoDeviceKey;

    fn retrieve_adapter_info(
        &self,
        wgpu_adapter: &wgpu::Adapter,
    ) -> Option<crate::capabilities::VideoAdapterInfo>;

    fn create_and_register_device(
        &self,
        wgpu_adapter: &wgpu::Adapter,
        desc: &crate::parameters::VideoDeviceDescriptor,
    ) -> Result<(wgpu::Device, wgpu::Queue), crate::VideoDeviceInitError>;
}

pub(crate) fn default_backend() -> impl CoreBackend {
    #[cfg(vulkan)]
    vulkan::VulkanBackend
}

#[cfg(feature = "wgpu")]
pub(crate) fn backend_from_wgpu(backend: wgpu::Backend) -> Option<impl WgpuBackend> {
    match backend {
        #[cfg(vulkan)]
        wgpu::Backend::Vulkan => Some(vulkan::VulkanBackend),
        _ => None,
    }
}
