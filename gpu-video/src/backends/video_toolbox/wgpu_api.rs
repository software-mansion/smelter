use std::sync::Arc;

use objc2_core_foundation as cf;
use objc2_core_video as cv;
use objc2_metal as mtl;
use objc2_metal::MTLDevice;

use crate::{
    WgpuTexturesDecoder,
    adapter::VideoAdapterInfo,
    backends::{
        WgpuBackend,
        video_toolbox::{
            VTBackend, VTDevice, allocate_retained,
            decoder::VTDecoder,
            encoder::{H264Codec, H265Codec, VTEncoder},
            error::{OSStatusError, VTInitError},
        },
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
        wgpu_device: wgpu::Device,
        wgpu_queue: wgpu::Queue,
        parameters: crate::device::EncoderParametersH264,
    ) -> Result<crate::WgpuTexturesEncoderH264, crate::VideoEncoderError> {
        let encoder = VTEncoder::<H264Codec>::new_wgpu(
            &wgpu_device,
            parameters.input_parameters,
            parameters.output_parameters,
        )?;

        Ok(crate::WgpuTexturesEncoderH264 {
            wgpu_device,
            wgpu_queue,
            encoder: Box::new(encoder),
        })
    }

    fn create_wgpu_textures_encoder_h265(
        self: Arc<Self>,
        wgpu_device: wgpu::Device,
        wgpu_queue: wgpu::Queue,
        parameters: crate::device::EncoderParametersH265,
    ) -> Result<crate::WgpuTexturesEncoderH265, crate::VideoEncoderError> {
        let encoder = VTEncoder::<H265Codec>::new_wgpu(
            &wgpu_device,
            parameters.input_parameters,
            parameters.output_parameters,
        )?;

        Ok(crate::WgpuTexturesEncoderH265 {
            wgpu_device,
            wgpu_queue,
            encoder: Box::new(encoder),
        })
    }
}

pub(crate) fn make_texture_cache(
    device: &wgpu::Device,
    usage: mtl::MTLTextureUsage,
) -> Result<SyncCache, VTInitError> {
    let metal_device = unsafe {
        device
            .as_hal::<wgpu::hal::metal::Api>()
            .ok_or(VTInitError::NotMetalBackend)?
            .raw_device()
            .clone()
    };

    let texture_attributes = unsafe {
        cf::CFDictionary::<cf::CFString, cf::CFNumber>::from_slices(
            &[cv::kCVMetalTextureUsage],
            &[cf::CFNumber::new_i64(usage.0 as i64).as_ref()],
        )
    };

    let texture_cache = unsafe {
        allocate_retained(|ptr| {
            cv::CVMetalTextureCache::create(
                None,
                None,
                &metal_device,
                Some(texture_attributes.as_ref()),
                ptr,
            )
        })?
    };

    Ok(SyncCache(texture_cache))
}

pub(crate) struct SyncCache(pub(crate) cf::CFRetained<cv::CVMetalTextureCache>);
unsafe impl Send for SyncCache {}

pub(crate) struct SendSyncCVBuffer(pub(crate) cf::CFRetained<cv::CVBuffer>);
unsafe impl Send for SendSyncCVBuffer {}
unsafe impl Sync for SendSyncCVBuffer {}

#[derive(Debug, thiserror::Error)]
pub enum MetalTextureError {
    #[error(transparent)]
    OSStatus(#[from] OSStatusError),

    #[error("Failed to extract Metal texture from CVMetalTexture")]
    ExtractionFailed,
}

pub(crate) fn wgpu_texture_from_pixel_buffer(
    cache: &SyncCache,
    device: &wgpu::Device,
    buffer: &cv::CVBuffer,
    usage: wgpu::TextureUsages,
    initial_use: wgpu::TextureUses,
    label: &str,
) -> Result<wgpu::Texture, MetalTextureError> {
    let width = cv::CVPixelBufferGetWidth(buffer);
    let height = cv::CVPixelBufferGetHeight(buffer);
    let y_width = cv::CVPixelBufferGetWidthOfPlane(buffer, 0);
    let y_height = cv::CVPixelBufferGetHeightOfPlane(buffer, 0);
    let uv_width = cv::CVPixelBufferGetWidthOfPlane(buffer, 1);
    let uv_height = cv::CVPixelBufferGetHeightOfPlane(buffer, 1);

    cache.0.flush(0);
    let texture_y = unsafe {
        allocate_retained(|ptr| {
            cv::CVMetalTextureCache::create_texture_from_image(
                None,
                &cache.0,
                buffer,
                None,
                mtl::MTLPixelFormat::R8Unorm,
                y_width,
                y_height,
                0,
                ptr,
            )
        })?
    };
    let mtl_texture_y =
        cv::CVMetalTextureGetTexture(&texture_y).ok_or(MetalTextureError::ExtractionFailed)?;

    let texture_uv = unsafe {
        allocate_retained(|ptr| {
            cv::CVMetalTextureCache::create_texture_from_image(
                None,
                &cache.0,
                buffer,
                None,
                mtl::MTLPixelFormat::RG8Unorm,
                uv_width,
                uv_height,
                1,
                ptr,
            )
        })?
    };
    let mtl_texture_uv =
        cv::CVMetalTextureGetTexture(&texture_uv).ok_or(MetalTextureError::ExtractionFailed)?;

    let guard_y = SendSyncCVBuffer(texture_y);
    let guard_uv = SendSyncCVBuffer(texture_uv);

    unsafe {
        let texture = wgpu::hal::metal::Device::texture_from_raw_planar(
            [mtl_texture_y, mtl_texture_uv],
            wgpu::TextureFormat::NV12,
            mtl::MTLTextureType::Type2D,
            1,
            1,
            wgpu::hal::CopyExtent {
                width: width as u32,
                height: height as u32,
                depth: 1,
            },
            Some(Box::new(move || {
                drop(guard_y);
                drop(guard_uv);
            })),
        );

        let texture = device.create_texture_from_hal::<wgpu::hal::metal::Api>(
            texture,
            &wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: width as u32,
                    height: height as u32,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::NV12,
                usage,
                view_formats: &[],
            },
            initial_use,
        );

        Ok(texture)
    }
}
