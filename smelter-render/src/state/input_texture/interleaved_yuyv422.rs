use tracing::error;

use crate::{
    RenderingMode, Resolution,
    state::node_texture::NodeTextureState,
    wgpu::{WgpuCtx, texture::InterleavedYuyv422Texture},
};

use super::convert_linear_to_srgb::RgbToSrgbConverter;

pub(super) struct InterleavedYuyv422Input {
    upload_textures: InterleavedYuyv422Texture,
    yuv_bind_group: wgpu::BindGroup,
    color_space_converter: Option<RgbToSrgbConverter>,
}

impl InterleavedYuyv422Input {
    pub fn new(ctx: &WgpuCtx) -> Self {
        let upload_textures = InterleavedYuyv422Texture::new(ctx, Resolution::MIN_2X2);
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

    pub fn upload(&mut self, ctx: &WgpuCtx, data: &[u8], resolution: Resolution) {
        self.maybe_recreate(ctx, resolution);
        self.upload_textures.upload(ctx, data);
    }

    pub fn convert(&mut self, ctx: &WgpuCtx, dest: &NodeTextureState) {
        match dest {
            NodeTextureState::GpuOptimized { texture, .. } => {
                // write to sRGB texture as if it was linear
                ctx.format.interleaved_yuyv_to_rgba_linear.convert(
                    ctx,
                    &self.yuv_bind_group,
                    texture.linear_view(),
                );
            }
            NodeTextureState::CpuOptimized { texture, .. } => {
                ctx.format.interleaved_yuyv_to_rgba_linear.convert(
                    ctx,
                    &self.yuv_bind_group,
                    texture.view(),
                );
            }
            NodeTextureState::WebGl { texture, .. } => {
                let Some(color_space_converter) = &mut self.color_space_converter else {
                    error!("Missing color space converter");
                    return;
                };
                ctx.format.interleaved_yuyv_to_rgba_linear.convert(
                    ctx,
                    &self.yuv_bind_group,
                    color_space_converter.texture.view(),
                );
                // copy from rgb texture to srgb texture
                color_space_converter.convert(ctx, texture.texture());
            }
        }
    }

    fn maybe_recreate(&mut self, ctx: &WgpuCtx, resolution: Resolution) {
        if resolution == self.upload_textures.resolution {
            return;
        }
        self.upload_textures = InterleavedYuyv422Texture::new(ctx, resolution);
        self.yuv_bind_group = self.upload_textures.new_bind_group(ctx);
        if ctx.mode == RenderingMode::WebGl {
            self.color_space_converter = Some(RgbToSrgbConverter::new(ctx, resolution))
        }
    }
}
