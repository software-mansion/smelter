use crate::global_registry::{GlobalRegistry, RegistryError};

/// Extension that exposes the video capabilities of a device.
/// The device must have been created using [`VideoAdapterExt::request_device_with_video_support`](`crate::VideoAdapterExt::request_device_with_video_support`), otherwise [`VideoDeviceExt::video`] will return a registry error.
pub trait VideoDeviceExt {
    fn video(&self) -> Result<crate::VideoDevice, RegistryError>;
}

impl VideoDeviceExt for wgpu::Device {
    fn video(&self) -> Result<crate::VideoDevice, RegistryError> {
        let video_device = GlobalRegistry::get_device(&self.into())?;
        Ok(crate::VideoDevice {
            inner: video_device,
            wgpu_device: Some(self.clone()),
        })
    }
}
