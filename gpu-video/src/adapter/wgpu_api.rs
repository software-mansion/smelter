use crate::{
    VideoAdapter, VideoDevice, VideoInstance, VulkanInitError, adapter::VideoAdapterInfo,
    device::VideoDeviceDescriptor,
};
use wgpu::hal::vulkan::Api as VkApi;

/// [`wgpu::Adapter`] extension that exposes video capabilities of an adapter.
pub trait VideoAdapterExt {
    /// Retrieves information about adapter and its video capabilities.
    /// Returns `None` if it doesn't support any video operations.
    fn video_adapter_info(&self) -> Option<VideoAdapterInfo>;

    /// Creates a device capable of creating video decoders and encoders
    /// via [`VideoDeviceExt`](crate::VideoDeviceExt)
    fn request_device_with_video_support(
        &self,
        desc: &VideoDeviceDescriptor,
    ) -> Result<(wgpu::Device, wgpu::Queue), VulkanInitError>;
}

#[cfg(vulkan)]
impl VideoAdapterExt for wgpu::Adapter {
    fn video_adapter_info(&self) -> Option<VideoAdapterInfo> {
        use crate::instance::VideoInstanceDescriptor;
        use ash::vk;

        let hal_adapter = unsafe { self.as_hal::<VkApi>().unwrap() };
        let physical_device = hal_adapter.raw_physical_device();
        let instance = hal_adapter.shared_instance();
        let instance = VideoInstance::new_unowned(
            instance.raw_instance().clone(),
            instance.entry().clone().into(),
            &VideoInstanceDescriptor {
                enable_validations: instance.extensions().contains(&vk::EXT_DEBUG_UTILS_NAME),
                ..Default::default()
            },
        );

        VideoAdapter::new(&instance, physical_device).map(|a| a.info)
    }

    fn request_device_with_video_support(
        &self,
        desc: &VideoDeviceDescriptor,
    ) -> Result<(wgpu::Device, wgpu::Queue), VulkanInitError> {
        use crate::instance::VideoInstanceDescriptor;
        use ash::vk;

        let hal_adapter = unsafe { self.as_hal::<VkApi>().unwrap() };
        let physical_device = hal_adapter.raw_physical_device();

        let instance = hal_adapter.shared_instance();
        let instance = VideoInstance::new_unowned(
            instance.raw_instance().clone(),
            // TODO: does entry even need arc?
            instance.entry().clone().into(),
            &VideoInstanceDescriptor {
                enable_validations: instance.extensions().contains(&vk::EXT_DEBUG_UTILS_NAME),
                ..Default::default()
            },
        );

        let video_adapter =
            VideoAdapter::new(&instance, physical_device).ok_or(VulkanInitError::NoDevice)?;

        VideoDevice::new_with_wgpu(&instance, self, video_adapter, desc.clone())
    }
}
