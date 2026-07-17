use std::sync::Arc;

use crate::{
    VideoDecoderError, VideoEncoderError,
    backends::{WgpuBackend, backend_from_wgpu},
    decoders::FrameCallback,
    device::{DecoderParameters, EncoderParametersH264, EncoderParametersH265},
    global_registry::{GlobalRegistry, RegistryError},
};

/// Extension that exposes the video capabilities of a device.
/// The device must have been created using [`VideoAdapterExt::request_device_with_video_support`](`crate::VideoAdapterExt::request_device_with_video_support`), otherwise [`VideoDeviceExt::video`] will return a registry error.
pub trait VideoDeviceExt {
    fn video(&self) -> Result<crate::VideoDevice, RegistryError>;
}

impl VideoDeviceExt for wgpu::Device {
    fn video(&self) -> Result<crate::VideoDevice, RegistryError> {
        let backend =
            backend_from_wgpu(self.adapter_info().backend).ok_or(RegistryError::DeviceNotFound)?;
        let device_key = backend.device_key_from_wgpu_device(self);
        let video_device = GlobalRegistry::get_device(&device_key)?;

        Ok(crate::VideoDevice {
            inner: video_device,
            wgpu_device: Some(self.clone()),
        })
    }
}

pub(crate) trait WgpuVideoDeviceBackend: Send + Sync {
    fn create_wgpu_textures_decoder_h264(
        self: Arc<Self>,
        wgpu_device: wgpu::Device,
        wgpu_queue: wgpu::Queue,
        parameters: DecoderParameters,
        on_frame_callback: FrameCallback<wgpu::Texture>,
    ) -> Result<crate::WgpuTexturesDecoderH264, VideoDecoderError>;

    fn create_wgpu_textures_encoder_h264(
        self: Arc<Self>,
        wgpu_device: wgpu::Device,
        wgpu_queue: wgpu::Queue,
        parameters: EncoderParametersH264,
    ) -> Result<crate::WgpuTexturesEncoderH264, VideoEncoderError>;

    fn create_wgpu_textures_encoder_h265(
        self: Arc<Self>,
        wgpu_device: wgpu::Device,
        wgpu_queue: wgpu::Queue,
        parameters: EncoderParametersH265,
    ) -> Result<crate::WgpuTexturesEncoderH265, VideoEncoderError>;
}
