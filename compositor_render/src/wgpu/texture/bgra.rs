use crate::{
    wgpu::{ctx::RenderingMode, WgpuCtx},
    Resolution,
};

use super::{base::new_texture, TextureExt};

#[derive(Debug)]
pub struct BGRATexture {
    texture: wgpu::Texture,
    view: BgraTextureView,
}

#[derive(Debug)]
pub enum BgraTextureView {
    MultiView {
        // rgb_view: wgpu::TextureView,
        srgb_view: wgpu::TextureView,
    },
    Rgb(wgpu::TextureView),
    Srgb(wgpu::TextureView),
}

impl BGRATexture {
    pub fn new(ctx: &WgpuCtx, resolution: Resolution) -> Self {
        let format = match ctx.mode {
            RenderingMode::CpuOptimzied => wgpu::TextureFormat::Rgba8Unorm,
            _ => wgpu::TextureFormat::Rgba8UnormSrgb,
        };
        let texture = new_texture(
            &ctx.device,
            None,
            wgpu::Extent3d {
                width: resolution.width as u32,
                height: resolution.height as u32,
                depth_or_array_layers: 1,
            },
            format,
            wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            match ctx.mode {
                RenderingMode::CpuOptimzied => &[wgpu::TextureFormat::Rgba8Unorm],
                RenderingMode::Gpu => &[
                    wgpu::TextureFormat::Rgba8UnormSrgb,
                    wgpu::TextureFormat::Rgba8Unorm,
                ],
                RenderingMode::WebGl => &[wgpu::TextureFormat::Rgba8Unorm],
            },
        );
        let view = BgraTextureView::new(ctx.mode, &texture);
        Self { texture, view }
    }

    pub fn upload(&self, ctx: &WgpuCtx, data: &[u8]) {
        self.texture.upload_data(&ctx.queue, data, 4);
    }

    pub fn default_view(&self) -> &wgpu::TextureView {
        self.view.default_view()
    }
}

impl BgraTextureView {
    fn new(mode: RenderingMode, texture: &wgpu::Texture) -> Self {
        match mode {
            RenderingMode::Gpu => Self::MultiView {
                // rgb_view: texture.create_view(&wgpu::TextureViewDescriptor {
                //     format: Some(wgpu::TextureFormat::Rgba8Unorm),
                //     ..Default::default()
                // }),
                srgb_view: texture.create_view(&wgpu::TextureViewDescriptor {
                    format: Some(wgpu::TextureFormat::Rgba8UnormSrgb),
                    ..Default::default()
                }),
            },
            RenderingMode::CpuOptimzied => {
                Self::Rgb(texture.create_view(&wgpu::TextureViewDescriptor {
                    format: Some(wgpu::TextureFormat::Rgba8Unorm),
                    ..Default::default()
                }))
            }
            RenderingMode::WebGl => Self::Srgb(texture.create_view(&wgpu::TextureViewDescriptor {
                format: Some(wgpu::TextureFormat::Rgba8UnormSrgb),
                ..Default::default()
            })),
        }
    }

    fn default_view(&self) -> &wgpu::TextureView {
        match self {
            BgraTextureView::MultiView { srgb_view, .. } => srgb_view,
            BgraTextureView::Rgb(texture_view) => texture_view,
            BgraTextureView::Srgb(texture_view) => texture_view,
        }
    }
}
