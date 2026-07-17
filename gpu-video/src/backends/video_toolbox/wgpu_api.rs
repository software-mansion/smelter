use std::sync::Arc;

use objc2_metal::MTLDevice;

use crate::{
    VideoEncoderError, WgpuTexturesDecoder,
    adapter::VideoAdapterInfo,
    backends::{
        WgpuBackend,
        video_toolbox::{VTBackend, VTDevice, decoder::VTDecoder, error::VTInitError},
    },
    device::WgpuVideoDeviceBackend,
    frame_sorter::FrameSorter,
    global_registry::{GlobalRegistry, VideoDeviceKey},
    parser::{h264::H264Parser, reference_manager::ReferenceContext},
};

use super::{caps, query_api_version};

impl WgpuBackend for VTBackend {
    fn device_key_from_wgpu_device(
        &self,
        device: &wgpu::Device,
    ) -> crate::global_registry::VideoDeviceKey {
        let hal = unsafe { device.as_hal::<wgpu::hal::metal::Api>().unwrap() };
        let registry_id = hal.raw_device().registryID();
        VideoDeviceKey::Metal { registry_id }
    }

    fn retrieve_adapter_info(
        &self,
        wgpu_adapter: &wgpu::Adapter,
    ) -> Option<crate::capabilities::VideoAdapterInfo> {
        let info = wgpu_adapter.get_info();
        let decode_capabilities = caps::query_decode_capabilities();
        let encode_capabilities = caps::query_encode_capabilities();

        Some(VideoAdapterInfo {
            name: info.name,
            driver_name: info.driver,
            driver_info: info.driver_info,
            device: info.device.to_string(),
            device_type: info.device_type.into(),
            vendor: info.vendor.to_string(),
            api_version: query_api_version(),
            supports_decoding: decode_capabilities.h264.is_some()
                || decode_capabilities.h265.is_some(),
            supports_encoding: encode_capabilities.h264.is_some()
                || encode_capabilities.h265.is_some(),
            decode_capabilities,
            encode_capabilities,
        })
    }

    fn create_and_register_device(
        &self,
        wgpu_adapter: &wgpu::Adapter,
        desc: &crate::parameters::VideoDeviceDescriptor,
    ) -> Result<(wgpu::Device, wgpu::Queue), crate::VideoDeviceInitError> {
        let (device, queue) =
            pollster::block_on(wgpu_adapter.request_device(&wgpu::DeviceDescriptor {
                label: Some("wgpu device created by the videotoolbox decoder"),
                required_features: desc.wgpu_features | wgpu::Features::TEXTURE_FORMAT_NV12,
                required_limits: desc.wgpu_limits.clone(),
                experimental_features: desc.wgpu_experimental_features,
                ..Default::default()
            }))
            .map_err(crate::WgpuInitError::WgpuRequestDeviceError)
            .map_err(VTInitError::from)?;

        let id = VTBackend.device_key_from_wgpu_device(&device);
        // VTDevice is empty, and MTLDevices actually only get destroyed at process exit.
        // Because of this, we never remove from the registry.
        GlobalRegistry::register_device(id, Arc::new(VTDevice {}));
        Ok((device, queue))
    }
}

impl WgpuVideoDeviceBackend for VTDevice {
    fn create_wgpu_textures_decoder_h264(
        self: Arc<Self>,
        wgpu_device: wgpu::Device,
        parameters: crate::device::DecoderParameters,
    ) -> Result<crate::WgpuTexturesDecoder, crate::VideoDecoderError> {
        let decoder = VTDecoder::new(Some(&wgpu_device), parameters.usage_flags)?;

        Ok(WgpuTexturesDecoder {
            wgpu_device,
            decoder: Box::new(decoder),
            parser: H264Parser::new_avcc_output(),
            reference_ctx: ReferenceContext::new(parameters.missed_frame_handling),
            frame_sorter: FrameSorter::default(),
        })
    }

    fn create_wgpu_textures_encoder_h264(
        self: Arc<Self>,
        _wgpu_device: wgpu::Device,
        _wgpu_queue: wgpu::Queue,
        _parameters: crate::device::EncoderParametersH264,
    ) -> Result<crate::WgpuTexturesEncoderH264, crate::VideoEncoderError> {
        Err(VideoEncoderError::EncoderUnsupported)
    }

    fn create_wgpu_textures_encoder_h265(
        self: Arc<Self>,
        _wgpu_device: wgpu::Device,
        _wgpu_queue: wgpu::Queue,
        _parameters: crate::device::EncoderParametersH265,
    ) -> Result<crate::WgpuTexturesEncoderH265, crate::VideoEncoderError> {
        Err(VideoEncoderError::EncoderUnsupported)
    }
}
