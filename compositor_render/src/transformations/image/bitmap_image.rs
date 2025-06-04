use image::ImageFormat;

use crate::{
    state::node_texture::NodeTextureState,
    wgpu::{
        texture::{RgbaLinearTexture, RgbaSrgbTexture},
        WgpuCtx,
    },
    RenderingMode, Resolution,
};

pub struct BitmapNodeState {
    was_rendered: bool,
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
        maybe_resolution: Option<Resolution>,
    ) -> Result<Self, image::ImageError> {
        let img = image::load_from_memory_with_format(&data, format)?;
        let original_resolution = Resolution {
            width: img.width() as usize,
            height: img.height() as usize,
        };
        let resolution = maybe_resolution.unwrap_or(original_resolution);

        match ctx.mode {
            RenderingMode::GpuOptimized | RenderingMode::WebGl => {
                let src_texture = RgbaSrgbTexture::new(ctx, original_resolution);
                src_texture.upload(ctx, &img.to_rgba8());
                ctx.queue.submit([]);

                let mut dst_texture = RgbaSrgbTexture::new(ctx, resolution);

                if original_resolution != resolution {
                    let src_bind_group = src_texture.new_bind_group(ctx);
                    ctx.utils.srgb_rgba_add_premult_alpha.render(
                        ctx,
                        &src_bind_group,
                        dst_texture.view(),
                    );
                } else {
                    dst_texture = src_texture;
                }

                Ok(Self::Srgb {
                    bg: dst_texture.new_bind_group(ctx),
                    texture: dst_texture,
                })
            }
            RenderingMode::CpuOptimized => {
                let image_buffer = if original_resolution != resolution {
                    let resized_img = image::imageops::resize(
                        &img,
                        resolution.width as u32,
                        resolution.height as u32,
                        image::imageops::FilterType::Lanczos3,
                    );
                    image::DynamicImage::from(resized_img).to_rgba8().into_raw()
                } else {
                    img.to_rgba8().into_raw()
                };

                let texture = RgbaLinearTexture::new(ctx, resolution);
                texture.upload(ctx, &image_buffer);
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
    pub fn new() -> Self {
        Self {
            was_rendered: false,
        }
    }
}
