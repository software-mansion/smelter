use std::sync::Arc;

use log::error;

use crate::{
    error::ErrorStack,
    scene::ComponentId,
    state::{RenderCtx, node_texture::NodeTexture},
    wgpu::{
        WgpuCtx,
        texture::{TextureExt, utils::pad_to_256},
    },
};

use super::WebRenderer;

pub struct WebRendererNode {
    renderer: Arc<WebRenderer>,
    embedding_data: EmbeddingData,
}

impl WebRendererNode {
    pub fn new(children_ids: Vec<ComponentId>, renderer: Arc<WebRenderer>) -> Self {
        Self {
            renderer,
            embedding_data: EmbeddingData {
                buffers: Vec::new(),
                children_ids,
            },
        }
    }

    pub fn render(
        &mut self,
        ctx: &mut RenderCtx,
        sources: &[&NodeTexture],
        target: &mut NodeTexture,
    ) {
        self.ensure_buffers(ctx.wgpu_ctx, sources);
        if let Err(err) = self
            .renderer
            .render(ctx, sources, &self.embedding_data, target)
        {
            error!(
                "Failed to run web render: {}",
                ErrorStack::new(&err).into_string()
            );
        }
    }

    fn ensure_buffers(&mut self, wgpu_ctx: &WgpuCtx, sources: &[&NodeTexture]) {
        self.embedding_data.buffers.resize_with(sources.len(), || {
            let buffer = wgpu_ctx.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Temporary texture buffer"),
                size: 0,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });

            Arc::new(buffer)
        });

        for (texture, buffer) in sources.iter().zip(&mut self.embedding_data.buffers) {
            let Some(texture_state) = texture.state() else {
                continue;
            };

            Self::ensure_buffer_size(wgpu_ctx, buffer, texture_state.texture());
        }
    }

    fn ensure_buffer_size(ctx: &WgpuCtx, buffer: &mut Arc<wgpu::Buffer>, texture: &wgpu::Texture) {
        let texture_size = texture.size();
        let texture_size = (pad_to_256(4 * texture_size.width) * texture_size.height) as u64;
        if buffer.size() != texture_size {
            *buffer = Arc::new(texture.new_download_buffer(ctx));
        }
    }
}

pub struct EmbeddingData {
    pub buffers: Vec<Arc<wgpu::Buffer>>,
    pub children_ids: Vec<ComponentId>,
}
