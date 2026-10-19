use std::io::Cursor;

use image::ImageFormat;

use crate::{
    RenderingMode, Resolution,
    state::node_texture::NodeTextureState,
    wgpu::{
        WgpuCtx,
        texture::{RgbaLinearTexture, RgbaSrgbTexture},
    },
};

use super::{ImageError, decoder_limits, validate_resolution};

pub struct BitmapNodeState {
    was_rendered: bool,
    resolution: Resolution,
}

#[derive(Debug)]
pub enum BitmapAsset {
    Srgb {
        texture: RgbaSrgbTexture,
        bg: wgpu::BindGroup,
    },
    Linear {
        texture: RgbaLinearTexture,
        bg: wgpu::BindGroup,
    },
}

impl BitmapAsset {
    pub(super) fn new(
        ctx: &WgpuCtx,
        data: bytes::Bytes,
        format: ImageFormat,
    ) -> Result<Self, ImageError> {
        let mut reader = image::io::Reader::with_format(Cursor::new(&data), format);
        reader.limits(decoder_limits(ctx));
        let img = reader.decode()?;
        let resolution = Resolution {
            width: img.width() as usize,
            height: img.height() as usize,
        };
        validate_resolution(ctx, resolution)?;

        match ctx.mode {
            RenderingMode::GpuOptimized | RenderingMode::WebGl => {
                let texture = RgbaSrgbTexture::new(ctx, resolution);
                texture.upload(ctx, &img.to_rgba8());
                ctx.queue.submit([]);

                Ok(Self::Srgb {
                    bg: texture.new_bind_group(ctx),
                    texture,
                })
            }
            RenderingMode::CpuOptimized => {
                let texture = RgbaLinearTexture::new(ctx, resolution);
                texture.upload(ctx, &img.to_rgba8());
                ctx.queue.submit([]);

                Ok(Self::Linear {
                    bg: texture.new_bind_group(ctx),
                    texture,
                })
            }
        }
    }

    pub(super) fn render(
        &self,
        ctx: &WgpuCtx,
        target: &NodeTextureState,
        state: &mut BitmapNodeState,
    ) {
        if state.was_rendered {
            return;
        }

        match &self {
            BitmapAsset::Srgb { bg, .. } => {
                ctx.utils
                    .srgb_rgba_add_premult_alpha
                    .render(ctx, bg, target.view());
            }
            BitmapAsset::Linear { bg, .. } => {
                ctx.utils
                    .linear_rgba_add_premult_alpha
                    .render(ctx, bg, target.view());
            }
        }
        state.was_rendered = true;
    }

    fn texture(&self) -> &wgpu::Texture {
        match self {
            BitmapAsset::Srgb { texture, .. } => texture.texture(),
            BitmapAsset::Linear { texture, .. } => texture.texture(),
        }
    }

    pub fn resolution(&self) -> Resolution {
        self.texture().size().into()
    }
}

impl BitmapNodeState {
    pub fn new(resolution: Resolution) -> Self {
        Self {
            was_rendered: false,
            resolution,
        }
    }
    pub fn resolution(&self) -> Resolution {
        self.resolution
    }
}
