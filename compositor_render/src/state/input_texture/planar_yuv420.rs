use tracing::error;

use crate::{
    state::node_texture::NodeTextureState,
    wgpu::{
        texture::{PlanarYuvTextures, PlanarYuvVariant},
        WgpuCtx,
    },
    RenderingMode, Resolution, YuvPlanes,
};

use super::convert_linear_to_srgb::RgbToSrgbConverter;

pub(super) struct PlanarYuv420Input {
    upload_textures: PlanarYuvTextures,
    yuv_bind_group: wgpu::BindGroup,
    color_space_converter: Option<RgbToSrgbConverter>,
}

impl PlanarYuv420Input {
    pub fn new(ctx: &WgpuCtx) -> Self {
        let upload_textures = PlanarYuvTextures::new(ctx, Resolution::MIN_2X2);
        let yuv_bind_group = upload_textures.new_bind_group(ctx);

        Self {
            upload_textures,
            yuv_bind_group,
            color_space_converter: None,
        }
    }

    pub fn resolution(&self) -> Resolution {
        self.upload_textures.resolution
    }

    pub fn upload(
        &mut self,
        ctx: &WgpuCtx,
        planes: YuvPlanes,
        variant: PlanarYuvVariant,
        resolution: Resolution,
    ) {
        self.maybe_recreate(ctx, resolution);
        self.upload_textures.upload(ctx, &planes, variant);
    }

    pub fn convert(&mut self, ctx: &WgpuCtx, dest: &NodeTextureState) {
        match dest {
            NodeTextureState::GpuOptimized { texture, .. } => {
                // write to sRGB texture as if it was linear
                ctx.format.planar_yuv_to_rgba_linear.convert(
                    ctx,
                    self.upload_textures.variant(),
                    &self.yuv_bind_group,
                    texture.linear_view(),
                );
            }
            NodeTextureState::CpuOptimized { texture, .. } => {
                ctx.format.planar_yuv_to_rgba_linear.convert(
                    ctx,
                    self.upload_textures.variant(),
                    &self.yuv_bind_group,
                    texture.view(),
                );
            }
            NodeTextureState::WebGl { texture, .. } => {
                let Some(color_space_converter) = &mut self.color_space_converter else {
                    error!("Missing color space converter");
                    return;
                };
                ctx.format.planar_yuv_to_rgba_linear.convert(
                    ctx,
                    self.upload_textures.variant(),
                    &self.yuv_bind_group,
                    color_space_converter.texture.view(),
                );
                color_space_converter.convert(ctx, texture.texture());
            }
        }
    }

    fn maybe_recreate(&mut self, ctx: &WgpuCtx, resolution: Resolution) {
        if resolution == self.upload_textures.resolution {
            return;
        }
        self.upload_textures = PlanarYuvTextures::new(ctx, resolution);
        self.yuv_bind_group = self.upload_textures.new_bind_group(ctx);
        if ctx.mode == RenderingMode::WebGl {
            self.color_space_converter = Some(RgbToSrgbConverter::new(ctx, resolution))
        }
    }
}
