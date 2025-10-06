use std::sync::Arc;

use tracing::error;

use crate::{
    RenderingMode, Resolution,
    state::node_texture::NodeTextureState,
    wgpu::{
        WgpuCtx,
        texture::{NV12Texture, NV12TextureViewCreateError},
    },
};

use super::convert_linear_to_srgb::RgbToSrgbConverter;

pub(super) struct NV12TextureInput {
    nv12_texture: NV12Texture,
    color_space_converter: Option<RgbToSrgbConverter>,
}

impl NV12TextureInput {
    pub fn new(
        ctx: &WgpuCtx,
        texture: Arc<wgpu::Texture>,
    ) -> Result<Self, NV12TextureViewCreateError> {
        let size = texture.size();
        let nv12_texture = NV12Texture::from_wgpu_texture(texture)?;
        let color_space_converter = match ctx.mode {
            RenderingMode::WebGl => Some(RgbToSrgbConverter::new(ctx, size.into())),
            _ => None,
        };
        Ok(Self {
            nv12_texture,
            color_space_converter,
        })
    }

    pub fn resolution(&self) -> Resolution {
        self.nv12_texture.texture().size().into()
    }

    pub fn update(
        &mut self,
        ctx: &WgpuCtx,
        texture: Arc<wgpu::Texture>,
    ) -> Result<(), NV12TextureViewCreateError> {
        self.nv12_texture = NV12Texture::from_wgpu_texture(texture)?;
        match (ctx.mode, &self.color_space_converter) {
            (RenderingMode::WebGl, Some(converter))
                if converter.size() != self.nv12_texture.texture().size() =>
            {
                self.color_space_converter = Some(RgbToSrgbConverter::new(ctx, self.resolution()))
            }
            (RenderingMode::WebGl, None) => {
                self.color_space_converter = Some(RgbToSrgbConverter::new(ctx, self.resolution()))
            }
            _ => (),
        };
        Ok(())
    }

    pub fn convert(&mut self, ctx: &WgpuCtx, dest: &NodeTextureState) {
        let bind_group = self.nv12_texture.new_bind_group(ctx);
        match dest {
            NodeTextureState::GpuOptimized { texture, .. } => {
                // write to sRGB texture as if it was linear
                ctx.format
                    .nv12_to_rgba_linear
                    .convert(ctx, &bind_group, texture.linear_view());
            }
            NodeTextureState::CpuOptimized { texture, .. } => {
                ctx.format
                    .nv12_to_rgba_linear
                    .convert(ctx, &bind_group, texture.view());
            }
            NodeTextureState::WebGl { texture, .. } => {
                let Some(color_space_converter) = &mut self.color_space_converter else {
                    error!("Missing color space converter");
                    return;
                };

                ctx.format.nv12_to_rgba_linear.convert(
                    ctx,
                    &bind_group,
                    color_space_converter.texture.view(),
                );
                color_space_converter.convert(ctx, texture.texture());
            }
        }
    }
}
