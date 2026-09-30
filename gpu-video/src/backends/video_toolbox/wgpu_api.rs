use std::sync::Arc;

use objc2::{rc::Retained, runtime::ProtocolObject};

use objc2_core_video as cv;
use objc2_metal as mtl;
use objc2_metal::MTLDevice;

use crate::{
    EncodedOutputChunk, OutputFrame, VideoTexture, WgpuTexturesDecoderH264,
    adapter::VideoAdapterInfo,
    backends::{
        WgpuBackend,
        video_toolbox::{
            VTBackend, VTDevice,
            decoders_h264::VTDecoderH264,
            encoder::{H264Codec, H265Codec, VTEncoder},
            error::VTInitError,
            metal_interop::{
                MetalTextureError, SendSyncCVBuffer, SyncCache, plane_textures_from_pixel_buffer,
            },
        },
    },
    device::WgpuVideoDeviceBackend,
    global_registry::{GlobalRegistry, VideoDeviceKey},
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
            supports_decoding: decode_capabilities.h264.is_some(),
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
                required_features: desc.wgpu_features,
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
        _wgpu_queue: wgpu::Queue,
        parameters: crate::device::DecoderParameters,
        on_frame_callback: Box<dyn FnMut(OutputFrame<VideoTexture>) + Send>,
    ) -> Result<WgpuTexturesDecoderH264, crate::VideoDecoderError> {
        let backend = VTDecoderH264::new_wgpu_textures(wgpu_device, parameters, on_frame_callback)?;

        Ok(WgpuTexturesDecoderH264 {
            backend: Box::new(backend),
        })
    }

    fn create_wgpu_textures_encoder_h264(
        self: Arc<Self>,
        wgpu_device: wgpu::Device,
        wgpu_queue: wgpu::Queue,
        parameters: crate::device::EncoderParametersH264,
        on_chunk_callback: Box<dyn FnMut(EncodedOutputChunk<Vec<u8>>) + Send>,
    ) -> Result<crate::WgpuTexturesEncoderH264, crate::VideoEncoderError> {
        let encoder = VTEncoder::<H264Codec>::new_wgpu(
            &wgpu_device,
            parameters.input_parameters,
            parameters.output_parameters,
            on_chunk_callback,
        )?;

        Ok(crate::WgpuTexturesEncoderH264 {
            wgpu_device,
            wgpu_queue,
            backend: Box::new(encoder),
        })
    }

    fn create_wgpu_textures_encoder_h265(
        self: Arc<Self>,
        wgpu_device: wgpu::Device,
        wgpu_queue: wgpu::Queue,
        parameters: crate::device::EncoderParametersH265,
        on_chunk_callback: Box<dyn FnMut(EncodedOutputChunk<Vec<u8>>) + Send>,
    ) -> Result<crate::WgpuTexturesEncoderH265, crate::VideoEncoderError> {
        let encoder = VTEncoder::<H265Codec>::new_wgpu(
            &wgpu_device,
            parameters.input_parameters,
            parameters.output_parameters,
            on_chunk_callback,
        )?;

        Ok(crate::WgpuTexturesEncoderH265 {
            wgpu_device,
            wgpu_queue,
            backend: Box::new(encoder),
        })
    }
}

impl SyncCache {
    pub(crate) fn new_from_wgpu(
        device: &wgpu::Device,
        usage: mtl::MTLTextureUsage,
    ) -> Result<Self, VTInitError> {
        let metal_device = unsafe {
            device
                .as_hal::<wgpu::hal::metal::Api>()
                .ok_or(VTInitError::NotMetalBackend)?
                .raw_device()
                .clone()
        };

        Self::new_from_mtl(&metal_device, usage)
    }
}

pub(crate) fn video_texture_from_pixel_buffer(
    cache: &SyncCache,
    device: &wgpu::Device,
    buffer: &cv::CVBuffer,
    usage: wgpu::TextureUsages,
    initial_use: wgpu::TextureUses,
    label: &str,
) -> Result<VideoTexture, MetalTextureError> {
    let planes = plane_textures_from_pixel_buffer(cache, buffer)?;
    let [y_guard, uv_guard] = planes.guards;

    let y_texture = plane_wgpu_texture(
        device,
        planes.y,
        y_guard,
        plane_size(buffer, 0),
        wgpu::TextureFormat::R8Unorm,
        usage,
        initial_use,
        &format!("{label} y plane"),
    );
    let uv_texture = plane_wgpu_texture(
        device,
        planes.uv,
        uv_guard,
        plane_size(buffer, 1),
        wgpu::TextureFormat::Rg8Unorm,
        usage,
        initial_use,
        &format!("{label} uv plane"),
    );

    Ok(VideoTexture::from_planes(y_texture, uv_texture))
}

fn plane_size(buffer: &cv::CVBuffer, plane_index: usize) -> wgpu::Extent3d {
    wgpu::Extent3d {
        width: cv::CVPixelBufferGetWidthOfPlane(buffer, plane_index) as u32,
        height: cv::CVPixelBufferGetHeightOfPlane(buffer, plane_index) as u32,
        depth_or_array_layers: 1,
    }
}

#[expect(clippy::too_many_arguments)]
fn plane_wgpu_texture(
    device: &wgpu::Device,
    mtl_texture: Retained<ProtocolObject<dyn mtl::MTLTexture>>,
    guard: SendSyncCVBuffer,
    size: wgpu::Extent3d,
    format: wgpu::TextureFormat,
    usage: wgpu::TextureUsages,
    initial_use: wgpu::TextureUses,
    label: &str,
) -> wgpu::Texture {
    unsafe {
        let hal_texture = wgpu::hal::metal::Device::texture_from_raw(
            mtl_texture,
            format,
            mtl::MTLTextureType::Type2D,
            1,
            1,
            wgpu::hal::CopyExtent {
                width: size.width,
                height: size.height,
                depth: 1,
            },
            Some(Box::new(move || drop(guard))),
        );

        device.create_texture_from_hal::<wgpu::hal::metal::Api>(
            hal_texture,
            &wgpu::TextureDescriptor {
                label: Some(label),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            },
            initial_use,
        )
    }
}
