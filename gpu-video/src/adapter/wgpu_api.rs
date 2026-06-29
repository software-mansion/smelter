use crate::{
    VideoDeviceInitError,
    adapter::VideoAdapterInfo,
    backends::{WgpuBackend, backend_from_wgpu},
    device::VideoDeviceDescriptor,
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
    ) -> Result<(wgpu::Device, wgpu::Queue), VideoDeviceInitError>;
}

impl VideoAdapterExt for wgpu::Adapter {
    fn video_adapter_info(&self) -> Option<VideoAdapterInfo> {
        let backend = backend_from_wgpu(self.get_info().backend)?;
        backend.retrieve_adapter_info(self)
    }

    fn request_device_with_video_support(
        &self,
        desc: &VideoDeviceDescriptor,
    ) -> Result<(wgpu::Device, wgpu::Queue), VideoDeviceInitError> {
        let backend = backend_from_wgpu(self.get_info().backend)
            .ok_or(VideoDeviceInitError::NotSuitableAdapter)?;
        backend.create_and_register_device(self, desc)
    }
}
