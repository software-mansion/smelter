use interleaved_yuv_to_rgba::InterleavedYuv422ToRgbaConverter;
use nv12_to_rgba::Nv12ToRgbaConverter;

use self::{planar_yuv_to_rgba::PlanarYuvToRgbaConverter, rgba_to_yuv::RgbaToYuvConverter};

use super::{
    common_pipeline::create_single_texture_bgl,
    ctx::RenderingMode,
    texture::{NV12TextureView, PlanarYuvTextures},
    WgpuCtx,
};

mod interleaved_yuv_to_rgba;
mod nv12_to_rgba;
mod planar_yuv_to_rgba;
mod rgba_to_yuv;

#[derive(Debug)]
pub struct TextureFormat {
    pub planar_yuv_to_rgba_srgb: PlanarYuvToRgbaConverter,
    pub planar_yuv_to_rgba_linear: PlanarYuvToRgbaConverter,
    pub interleaved_yuv_to_rgba: InterleavedYuv422ToRgbaConverter,
    pub rgba_to_yuv: RgbaToYuvConverter,
    pub nv12_to_rgba: Nv12ToRgbaConverter,

    pub single_texture_layout: wgpu::BindGroupLayout,
    pub planar_yuv_layout: wgpu::BindGroupLayout,
    pub nv12_layout: wgpu::BindGroupLayout,
}

impl TextureFormat {
    pub fn new(device: &wgpu::Device, mode: RenderingMode) -> Self {
        let single_texture_layout = create_single_texture_bgl(device);
        let planar_yuv_layout = PlanarYuvTextures::new_bind_group_layout(device);
        let nv12_layout = NV12TextureView::new_bind_group_layout(device);

        let planar_yuv_to_rgba_srgb = PlanarYuvToRgbaConverter::new(
            device,
            &planar_yuv_layout,
            wgpu::TextureFormat::Rgba8UnormSrgb,
        );
        let planar_yuv_to_rgba_linear = PlanarYuvToRgbaConverter::new(
            device,
            &planar_yuv_layout,
            wgpu::TextureFormat::Rgba8Unorm,
        );
        let rgba_to_yuv = RgbaToYuvConverter::new(device, &single_texture_layout);
        let interleaved_yuv_to_rgba =
            InterleavedYuv422ToRgbaConverter::new(device, mode, &single_texture_layout);
        let nv12_to_rgba = Nv12ToRgbaConverter::new(device, mode, &nv12_layout);

        Self {
            planar_yuv_to_rgba_srgb,

            rgba_to_yuv,
            interleaved_yuv_to_rgba,
            nv12_to_rgba,

            single_texture_layout,
            planar_yuv_layout,
            nv12_layout,
        }
    }
}
