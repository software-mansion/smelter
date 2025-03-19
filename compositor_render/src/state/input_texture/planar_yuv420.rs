use crate::{
    wgpu::{texture::PlanarYuvTextures, RenderingMode, WgpuCtx},
    Frame, Resolution, YuvPlanes,
};

use super::rgb_to_srgb::RgbToSrgbConverter;

struct PlanarYuv420Input {
    upload_textures: PlanarYuvTextures,
    yuv_bind_group: wgpu::BindGroup,
    color_space_converter: Option<RgbToSrgbConverter>,
}

impl PlanarYuv420Input {
    pub fn new(ctx: &WgpuCtx) -> Self {
        let upload_textures = PlanarYuvTextures::new(
            ctx,
            Resolution {
                width: 1,
                height: 1,
            },
        );
        let yuv_bind_group = upload_textures.new_bind_group(ctx, &ctx.format.planar_yuv_layout);

        Self {
            upload_textures,
            yuv_bind_group,
            color_space_converter: None,
        }
    }

    pub fn upload(
        &mut self,
        ctx: &WgpuCtx,
        planes: YuvPlanes,
        variant: PlanarYuvTextures,
        resolution: Resolution,
    ) {
    }

    fn maybe_recreate(&mut self, ctx: &WgpuCtx, resolution: Resolution) {
        if resolution == self.upload_textures.resolution {
            return;
        }
        self.upload_textures = PlanarYuvTextures::new(ctx, resolution);
        self.yuv_bind_group = self
            .upload_textures
            .new_bind_group(ctx, &ctx.format.planar_yuv_layout);
        if ctx.mode == RenderingMode::WebGl {
            self.color_space_converter = RgbToSrgbConverter::new()
        }
    }
}
