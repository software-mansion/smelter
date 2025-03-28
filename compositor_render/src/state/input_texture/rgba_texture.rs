use std::sync::Arc;

use tracing::warn;

use crate::{state::node_texture::NodeTextureState, wgpu::WgpuCtx, Resolution};

pub(super) struct RgbaTextureInput {
    texture: Arc<wgpu::Texture>,
}

impl RgbaTextureInput {
    pub fn new(texture: Arc<wgpu::Texture>) -> Self {
        Self { texture }
    }

    pub fn resolution(&self) -> Resolution {
        self.texture.size().into()
    }

    pub fn update(&mut self, texture: Arc<wgpu::Texture>) {
        self.texture = texture;
    }

    fn bind_group(&self, ctx: &WgpuCtx) -> wgpu::BindGroup {
        let view = self.texture.create_view(&wgpu::TextureViewDescriptor {
            ..Default::default()
        });
        ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("RgbaTextureInput"),
            layout: &ctx.format.single_texture_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            }],
        })
    }

    pub fn convert(&self, ctx: &WgpuCtx, dest: &NodeTextureState) {
        match (dest, self.texture.format()) {
            (
                NodeTextureState::GpuOptimized { texture, .. },
                wgpu::TextureFormat::Rgba8UnormSrgb,
            ) => ctx.utils.srgb_rgba_add_premult_alpha.render(
                ctx,
                &self.bind_group(ctx),
                texture.srgb_view(),
            ),
            (NodeTextureState::CpuOptimized { texture, .. }, wgpu::TextureFormat::Rgba8Unorm) => {
                ctx.utils.linear_rgba_add_premult_alpha.render(
                    ctx,
                    &self.bind_group(ctx),
                    texture.view(),
                )
            }
            (NodeTextureState::WebGl { texture, .. }, wgpu::TextureFormat::Rgba8UnormSrgb) => ctx
                .utils
                .srgb_rgba_add_premult_alpha
                .render(ctx, &self.bind_group(ctx), texture.view()),
            (_, format) => {
                warn!("Unsupported format: {format:?}")
            }
        }
    }
}
