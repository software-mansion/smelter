use std::sync::Arc;

use tracing::warn;

use crate::{
    state::node_texture::NodeTextureState,
    wgpu::{RenderingMode, WgpuCtx},
    Resolution,
};

use super::rgb_to_srgb::{copy_texture_to_texture, RgbToSrgbConverter};

pub(super) struct RgbaTextureInput {
    texture: Arc<wgpu::Texture>,
    color_space_converter: Option<RgbToSrgbConverter>,
}

impl RgbaTextureInput {
    pub fn new(ctx: &WgpuCtx, texture: Arc<wgpu::Texture>) -> Self {
        let mut input = Self {
            texture: texture.clone(),
            color_space_converter: None,
        };
        input.update(ctx, texture);
        input
    }

    pub fn resolution(&self) -> Resolution {
        self.texture.size().into()
    }

    pub fn update(&mut self, ctx: &WgpuCtx, texture: Arc<wgpu::Texture>) {
        let size = texture.size();
        self.texture = texture;
        match (ctx.mode, &self.color_space_converter) {
            (RenderingMode::WebGl, Some(converter)) if converter.size() != size => {
                self.color_space_converter = Some(RgbToSrgbConverter::new(ctx, size.into()))
            }
            (RenderingMode::WebGl, None) => {
                self.color_space_converter = Some(RgbToSrgbConverter::new(ctx, size.into()))
            }
            _ => (),
        }
    }

    pub fn convert(&self, ctx: &WgpuCtx, dest: &NodeTextureState) {
        match dest {
            NodeTextureState::Gpu { texture, .. } => match self.texture.format() {
                wgpu::TextureFormat::Rgba8Unorm => {
                    copy_texture_to_texture(ctx, texture.texture(), dest.texture());
                }
                wgpu::TextureFormat::Rgba8UnormSrgb => {
                    copy_texture_to_texture(ctx, texture.texture(), dest.texture());
                }
                _ => return,
            },
            NodeTextureState::CpuOptimized { texture, .. } => match self.texture.format() {
                wgpu::TextureFormat::Rgba8Unorm => {
                    copy_texture_to_texture(ctx, texture.texture(), dest.texture());
                }
                wgpu::TextureFormat::Rgba8UnormSrgb => {
                    copy_texture_to_texture(ctx, texture.texture(), dest.texture());
                }
                _ => return,
            },
            NodeTextureState::WebGl { texture, .. } => match self.texture.format() {
                wgpu::TextureFormat::Rgba8Unorm => {
                    copy_texture_to_texture(ctx, texture.texture(), dest.texture());
                }
                wgpu::TextureFormat::Rgba8UnormSrgb => {
                    warn!("convert");
                    copy_texture_to_texture(ctx, texture.texture(), dest.texture());
                }
                _ => return,
            },
        }
    }
}
