use objc2_core_video as cv;
use objc2_metal as mtl;

use crate::backends::video_toolbox::{
    error::{VTDecoderError, VTInitError},
    metal_interop::SyncCache,
    wgpu_api::wgpu_texture_from_pixel_buffer,
};

pub(crate) fn make_texture_cache(device: &wgpu::Device) -> Result<SyncCache, VTInitError> {
    SyncCache::new_from_wgpu(device, mtl::MTLTextureUsage::ShaderRead)
}

pub(crate) fn to_wgpu_texture(
    device: &wgpu::Device,
    cache: &SyncCache,
    buffer: &cv::CVBuffer,
) -> Result<wgpu::Texture, VTDecoderError> {
    Ok(wgpu_texture_from_pixel_buffer(
        cache,
        device,
        buffer,
        wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::TEXTURE_BINDING,
        wgpu::TextureUses::RESOURCE,
        "gpu-video output",
    )?)
}
