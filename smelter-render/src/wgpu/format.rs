use interleaved_yuv_to_rgba::InterleavedYuv422ToRgbaConverter;
use nv12_to_rgba::Nv12ToRgbaConverter;

use self::{planar_yuv_to_rgba::PlanarYuvToRgbaConverter, rgba_to_yuv::RgbaToYuvConverter};

use super::{
    WgpuCtx,
    texture::{NV12Texture, PlanarYuvTextures},
};

mod interleaved_yuv_to_rgba;
mod nv12_to_rgba;
mod planar_yuv_to_rgba;
mod rgba_to_yuv;

#[derive(Debug)]
pub struct TextureFormat {
    pub planar_yuv_to_rgba_linear: PlanarYuvToRgbaConverter,
    pub interleaved_yuv_to_rgba_linear: InterleavedYuv422ToRgbaConverter,
    pub nv12_to_rgba_linear: Nv12ToRgbaConverter,
    pub rgba_to_yuv: RgbaToYuvConverter,

    pub single_texture_layout: wgpu::BindGroupLayout,
    pub planar_yuv_layout: wgpu::BindGroupLayout,
    pub nv12_layout: wgpu::BindGroupLayout,
}

impl TextureFormat {
    pub fn new(device: &wgpu::Device) -> Self {
        let single_texture_layout = create_single_texture_bgl(device);
        let planar_yuv_layout = PlanarYuvTextures::new_bind_group_layout(device);
        let nv12_layout = NV12Texture::new_bind_group_layout(device);

        let planar_yuv_to_rgba_linear = PlanarYuvToRgbaConverter::new(
            device,
            &planar_yuv_layout,
            wgpu::TextureFormat::Rgba8Unorm,
        );
        let interleaved_yuv_to_rgba_linear = InterleavedYuv422ToRgbaConverter::new(
            device,
            &single_texture_layout,
            wgpu::TextureFormat::Rgba8Unorm,
        );
        let nv12_to_rgba_linear =
            Nv12ToRgbaConverter::new(device, &nv12_layout, wgpu::TextureFormat::Rgba8Unorm);

        let rgba_to_yuv = RgbaToYuvConverter::new(device, &single_texture_layout);

        Self {
            planar_yuv_to_rgba_linear,
            interleaved_yuv_to_rgba_linear,
            nv12_to_rgba_linear,

            rgba_to_yuv,

            single_texture_layout,
            planar_yuv_layout,
            nv12_layout,
        }
    }
}

fn create_single_texture_bgl(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            count: None,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Texture {
                multisampled: false,
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
            },
        }],
    })
}
