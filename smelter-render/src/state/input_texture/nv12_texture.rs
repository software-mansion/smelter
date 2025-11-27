use std::sync::Arc;

use tracing::error;

use crate::{
    NvPlanes, RenderingMode, Resolution,
    state::node_texture::NodeTextureState,
    wgpu::{
        WgpuCtx,
        texture::{NV12Texture, NV12TextureViewCreateError},
    },
};

use super::convert_linear_to_srgb::RgbToSrgbConverter;

pub(super) struct NV12Input {
    nv12_texture: NV12Texture,
    color_space_converter: Option<RgbToSrgbConverter>,
    bind_group: wgpu::BindGroup,
}

impl NV12Input {
    pub fn new_from_texture(
        ctx: &WgpuCtx,
        texture: Arc<wgpu::Texture>,
    ) -> Result<Self, NV12TextureViewCreateError> {
        let size = texture.size();
        let nv12_texture = NV12Texture::from_wgpu_texture(texture)?;
        let color_space_converter = match ctx.mode {
            RenderingMode::WebGl => Some(RgbToSrgbConverter::new(ctx, size.into())),
            _ => None,
        };
        let bind_group = nv12_texture.new_bind_group(ctx);
        Ok(Self {
            nv12_texture,
            color_space_converter,
            bind_group,
        })
    }

    pub fn new_uploadable(ctx: &WgpuCtx, resolution: Resolution) -> Self {
        let nv12_texture = NV12Texture::new_uploadable(ctx, resolution);
        let color_space_converter = match ctx.mode {
            RenderingMode::WebGl => Some(RgbToSrgbConverter::new(ctx, resolution)),
            _ => None,
        };
        let bind_group = nv12_texture.new_bind_group(ctx);
        Self {
            nv12_texture,
            color_space_converter,
            bind_group,
        }
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
        self.bind_group = self.nv12_texture.new_bind_group(ctx);
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

    pub fn upload(&mut self, ctx: &WgpuCtx, planes: NvPlanes, resolution: Resolution) {
        self.maybe_recreate_before_upload(ctx, resolution);
        self.nv12_texture.upload(ctx, &planes);
    }

    fn maybe_recreate_before_upload(&mut self, ctx: &WgpuCtx, resolution: Resolution) {
        if self.nv12_texture.uploadable() && self.resolution() == resolution {
            return;
        }

        self.nv12_texture = NV12Texture::new_uploadable(ctx, resolution);
        self.bind_group = self.nv12_texture.new_bind_group(ctx);
        if ctx.mode == RenderingMode::WebGl {
            self.color_space_converter = Some(RgbToSrgbConverter::new(ctx, resolution))
        }
    }

    pub fn convert(&mut self, ctx: &WgpuCtx, dest: &NodeTextureState) {
        match dest {
            NodeTextureState::GpuOptimized { texture, .. } => {
                // write to sRGB texture as if it was linear
                ctx.format.nv12_to_rgba_linear.convert(
                    ctx,
                    &self.bind_group,
                    texture.linear_view(),
                );
            }
            NodeTextureState::CpuOptimized { texture, .. } => {
                ctx.format
                    .nv12_to_rgba_linear
                    .convert(ctx, &self.bind_group, texture.view());
            }
            NodeTextureState::WebGl { texture, .. } => {
                let Some(color_space_converter) = &mut self.color_space_converter else {
                    error!("Missing color space converter");
                    return;
                };

                ctx.format.nv12_to_rgba_linear.convert(
                    ctx,
                    &self.bind_group,
                    color_space_converter.texture.view(),
                );
                color_space_converter.convert(ctx, texture.texture());
            }
        }
    }
}
