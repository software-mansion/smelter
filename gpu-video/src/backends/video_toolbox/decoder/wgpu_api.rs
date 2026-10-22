use objc2_core_foundation as cf;
use objc2_core_video as cv;
use objc2_metal as mtl;

use crate::VideoTexture;
use crate::backends::video_toolbox::{
    error::{VTDecoderError, VTInitError},
    metal_interop::IOSurfaceTextures,
    wgpu_api::video_texture_from_pixel_buffer,
};

pub(crate) fn make_textures(device: &wgpu::Device) -> Result<IOSurfaceTextures, VTInitError> {
    IOSurfaceTextures::new_from_wgpu(device, mtl::MTLTextureUsage::ShaderRead)
}

pub(crate) fn to_video_texture(
    device: &wgpu::Device,
    textures: &IOSurfaceTextures,
    buffer: &cf::CFRetained<cv::CVBuffer>,
) -> Result<VideoTexture, VTDecoderError> {
    Ok(video_texture_from_pixel_buffer(
        textures,
        device,
        buffer,
        wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::TEXTURE_BINDING,
        wgpu::TextureUses::RESOURCE,
        "gpu-video output",
    )?)
}
