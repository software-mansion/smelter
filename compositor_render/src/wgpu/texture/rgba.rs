use crate::{
    wgpu::{common_pipeline, ctx::RenderingMode, WgpuCtx},
    Resolution,
};

use super::base::{new_texture, TextureExt};

#[derive(Debug)]
pub struct RGBATexture {
    texture: wgpu::Texture,
    view: RgbaTextureView,
}

#[derive(Debug)]
pub enum RgbaTextureView {
    MultiView {
        rgb_view: wgpu::TextureView,
        srgb_view: wgpu::TextureView,
    },
    Rgb(wgpu::TextureView),
    Srgb(wgpu::TextureView),
}

impl RGBATexture {
    pub fn new(ctx: &WgpuCtx, resolution: Resolution) -> Self {
        Self::new_texture(&ctx.device, ctx.mode, resolution)
    }

    pub fn empty(device: &wgpu::Device, mode: RenderingMode) -> Self {
        Self::new_texture(
            device,
            mode,
            Resolution {
                width: 1,
                height: 1,
            },
        )
    }

    fn new_texture(device: &wgpu::Device, mode: RenderingMode, resolution: Resolution) -> Self {
        let format = match mode {
            RenderingMode::CpuOptimzied => wgpu::TextureFormat::Rgba8Unorm,
            _ => wgpu::TextureFormat::Rgba8UnormSrgb,
        };
        let size = wgpu::Extent3d {
            width: resolution.width as u32,
            height: resolution.height as u32,
            depth_or_array_layers: 1,
        };
        let usage = wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::TEXTURE_BINDING;

        let texture = new_texture(
            &device,
            None,
            size,
            format,
            usage,
            match mode {
                RenderingMode::CpuOptimzied => &[wgpu::TextureFormat::Rgba8Unorm],
                RenderingMode::Gpu => &[
                    wgpu::TextureFormat::Rgba8UnormSrgb,
                    wgpu::TextureFormat::Rgba8Unorm,
                ],
                RenderingMode::WebGl => &[wgpu::TextureFormat::Rgba8Unorm],
            },
        );

        let view = RgbaTextureView::new(mode, &texture);

        Self { texture, view }
    }

    pub fn new_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        common_pipeline::create_single_texture_bgl(device)
    }

    pub fn new_bind_group(
        &self,
        ctx: &WgpuCtx,
        layout: &wgpu::BindGroupLayout,
    ) -> wgpu::BindGroup {
        ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("texture bind group"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(self.view.default_view()),
            }],
        })
    }

    pub fn new_raw_bind_group(
        &self,
        ctx: &WgpuCtx,
        layout: &wgpu::BindGroupLayout,
    ) -> wgpu::BindGroup {
        ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("texture bind group"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(
                    self.raw_view().unwrap_or(self.view.default_view()),
                ),
            }],
        })
    }

    pub fn upload(&self, ctx: &WgpuCtx, data: &[u8]) {
        self.texture.upload_data(&ctx.queue, data, 4);
    }

    pub fn new_download_buffer(&self, ctx: &WgpuCtx) -> wgpu::Buffer {
        self.texture.new_download_buffer(ctx)
    }

    pub fn copy_to_buffer(&self, encoder: &mut wgpu::CommandEncoder, buffer: &wgpu::Buffer) {
        self.texture.copy_to_buffer(encoder, buffer);
    }

    pub fn size(&self) -> wgpu::Extent3d {
        self.texture.size()
    }

    pub fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }

    pub fn texture_owned(self) -> wgpu::Texture {
        self.texture
    }

    pub fn default_view(&self) -> &wgpu::TextureView {
        self.view.default_view()
    }

    /**
     * Returns Rgba8Unorm when possible
     */
    pub fn raw_view(&self) -> Option<&wgpu::TextureView> {
        match &self.view {
            RgbaTextureView::MultiView { rgb_view, .. } => Some(&rgb_view),
            RgbaTextureView::Rgb(texture_view) => Some(&texture_view),
            RgbaTextureView::Srgb(_) => None,
        }
    }
}

impl RgbaTextureView {
    fn new(mode: RenderingMode, texture: &wgpu::Texture) -> Self {
        match mode {
            RenderingMode::Gpu => Self::MultiView {
                rgb_view: texture.create_view(&wgpu::TextureViewDescriptor {
                    format: Some(wgpu::TextureFormat::Rgba8Unorm),
                    ..Default::default()
                }),
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
            RgbaTextureView::MultiView { srgb_view, .. } => srgb_view,
            RgbaTextureView::Rgb(texture_view) => texture_view,
            RgbaTextureView::Srgb(texture_view) => texture_view,
        }
    }
}
