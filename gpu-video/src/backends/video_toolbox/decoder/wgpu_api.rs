use std::sync::Mutex;

use objc2_core_foundation as cf;
use objc2_core_video as cv;
use objc2_metal as mtl;
use wgpu::hal::metal::Api as MtlApi;

use crate::{
    VideoTexture,
    backends::video_toolbox::{
        allocate_retained,
        error::{VTDecoderError, VTInitError},
    },
};

pub(crate) fn to_video_texture(
    device: &wgpu::Device,
    cache: &SyncCache,
    buffer: &cv::CVBuffer,
) -> Result<VideoTexture, VTDecoderError> {
    let cache = cache.0.lock().unwrap();
    cache.0.flush(0);

    let y_texture = plane_texture(
        &cache,
        device,
        buffer,
        0,
        wgpu::TextureFormat::R8Unorm,
        mtl::MTLPixelFormat::R8Unorm,
        "gpu-video output y plane",
    )?;
    let uv_texture = plane_texture(
        &cache,
        device,
        buffer,
        1,
        wgpu::TextureFormat::Rg8Unorm,
        mtl::MTLPixelFormat::RG8Unorm,
        "gpu-video output uv plane",
    )?;

    Ok(VideoTexture::from_planes(y_texture, uv_texture))
}

fn plane_texture(
    cache: &MetalTextureCache,
    device: &wgpu::Device,
    buffer: &cv::CVBuffer,
    plane_index: usize,
    format: wgpu::TextureFormat,
    mtl_format: mtl::MTLPixelFormat,
    label: &str,
) -> Result<wgpu::Texture, VTDecoderError> {
    let width = cv::CVPixelBufferGetWidthOfPlane(buffer, plane_index);
    let height = cv::CVPixelBufferGetHeightOfPlane(buffer, plane_index);

    let cv_texture = unsafe {
        allocate_retained(|ptr| {
            cv::CVMetalTextureCache::create_texture_from_image(
                None,
                &cache.0,
                buffer,
                None,
                mtl_format,
                width,
                height,
                plane_index,
                ptr,
            )
        })?
    };
    let mtl_texture = cv::CVMetalTextureGetTexture(&cv_texture)
        .ok_or(VTDecoderError::MetalTextureExtractionFailed)?;
    let guard = SendSyncCVBuffer(cv_texture);

    let size = wgpu::Extent3d {
        width: width as u32,
        height: height as u32,
        depth_or_array_layers: 1,
    };

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

        Ok(device.create_texture_from_hal::<MtlApi>(
            hal_texture,
            &wgpu::TextureDescriptor {
                label: Some(label),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            wgpu::TextureUses::RESOURCE,
        ))
    }
}

pub(crate) fn make_texture_cache(device: &wgpu::Device) -> Result<SyncCache, VTInitError> {
    let metal_device = unsafe {
        device
            .as_hal::<MtlApi>()
            .ok_or(VTInitError::NotMetalBackend)?
            .raw_device()
            .clone()
    };

    let texture_attributes = unsafe {
        cf::CFDictionary::<cf::CFString, cf::CFNumber>::from_slices(
            &[cv::kCVMetalTextureUsage],
            &[cf::CFNumber::new_i64(mtl::MTLTextureUsage::ShaderRead.0 as i64).as_ref()],
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

    Ok(SyncCache(Mutex::new(MetalTextureCache(texture_cache))))
}

pub(crate) struct SyncCache(Mutex<MetalTextureCache>);

struct MetalTextureCache(cf::CFRetained<cv::CVMetalTextureCache>);

// Safety: texture caches are not marked in docs as thread-affine (required to be used on the
// thread that created them)
unsafe impl Send for MetalTextureCache {}

#[allow(dead_code)]
struct SendSyncCVBuffer(cf::CFRetained<cv::CVBuffer>);
unsafe impl Send for SendSyncCVBuffer {}
unsafe impl Sync for SendSyncCVBuffer {}
