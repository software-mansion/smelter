use crate::{
    VideoInitError,
    adapter::VideoAdapterInfo,
    device::{VideoDevice, VideoDeviceDescriptor},
    vulkan::vulkan_adapter::with_video_adapter_from_wgpu,
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
