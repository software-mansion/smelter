use crate::{
    VideoAdapter, VideoInitError, VideoInstance,
    adapter::VideoAdapterInfo,
    device::{VideoDevice, VideoDeviceDescriptor},
};

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
    ) -> Result<(wgpu::Device, wgpu::Queue), VideoInitError>;
}

impl VideoAdapterExt for wgpu::Adapter {
    fn video_adapter_info(&self) -> Option<VideoAdapterInfo> {
        with_video_adapter_from_wgpu(self, |adapter| adapter.info)
    }

    fn request_device_with_video_support(
        &self,
        desc: &VideoDeviceDescriptor,
    ) -> Result<(wgpu::Device, wgpu::Queue), VideoInitError> {
        with_video_adapter_from_wgpu(self, |adapter| {
            VideoDevice::create_and_register_wgpu(self, adapter, desc.clone())
        })
        .ok_or(VideoInitError::NoDevice)?
    }
}

#[cfg(vulkan)]
fn with_video_adapter_from_wgpu<F, R>(wgpu_adapter: &wgpu::Adapter, use_adapter: F) -> Option<R>
where
    F: Fn(VideoAdapter<'_>) -> R,
{
    use crate::instance::VideoInstanceDescriptor;
    use ash::vk;
    use wgpu::hal::vulkan::Api as VkApi;

    let hal_adapter = unsafe { wgpu_adapter.as_hal::<VkApi>()? };
    let physical_device = hal_adapter.raw_physical_device();
    let instance = hal_adapter.shared_instance();
    let instance = VideoInstance::new_unowned(
        instance.raw_instance().clone(),
        instance.entry().clone(),
        &VideoInstanceDescriptor {
            enable_validations: instance.extensions().contains(&vk::EXT_DEBUG_UTILS_NAME),
            ..Default::default()
        },
    );

    VideoAdapter::new(&instance, physical_device).map(use_adapter)
}
