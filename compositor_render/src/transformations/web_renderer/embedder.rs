use crate::state::node_texture::NodeTexture;
use crate::state::RegisterCtx;
use crate::transformations::web_renderer::chromium_sender::ChromiumSender;
use crate::wgpu::texture::TextureExt;
use crate::wgpu::WgpuCtx;
use bytes::Bytes;
use crossbeam_channel::bounded;
use log::error;
use nalgebra_glm::Mat4;
use std::sync::Arc;

use super::chromium_sender::ChromiumSenderError;
use super::node::EmbeddingData;
use super::WebEmbeddingMethod;

#[derive(Debug)]
pub(super) struct EmbeddingHelper {
    wgpu_ctx: Arc<WgpuCtx>,
    chromium_sender: ChromiumSender,
    embedding_method: WebEmbeddingMethod,
}

impl EmbeddingHelper {
    pub fn new(
        ctx: &RegisterCtx,
        chromium_sender: ChromiumSender,
        embedding_method: WebEmbeddingMethod,
    ) -> Self {
        Self {
            wgpu_ctx: ctx.wgpu_ctx.clone(),
            chromium_sender,
            embedding_method,
        }
    }

    pub fn prepare_embedding(
        &self,
        sources: &[&NodeTexture],
        embedding_data: &EmbeddingData,
    ) -> Result<(), EmbedError> {
        match self.embedding_method {
            WebEmbeddingMethod::ChromiumEmbedding => {
                self.chromium_embedding(sources, embedding_data)?
            }
            WebEmbeddingMethod::NativeEmbeddingOverContent
            | WebEmbeddingMethod::NativeEmbeddingUnderContent => self
                .chromium_sender
                .request_frame_positions(embedding_data.children_ids.clone())?,
        }

        Ok(())
    }

    /// Send sources to chromium and render them on canvases via JS API
    fn chromium_embedding(
        &self,
        sources: &[&NodeTexture],
        embedding_data: &EmbeddingData,
    ) -> Result<(), EmbedError> {
        self.chromium_sender.ensure_shared_memory(sources)?;
        self.copy_sources_to_buffers(sources, &embedding_data.buffers)?;

        let mut pending_downloads = Vec::new();
        for (source_idx, (texture, buffer)) in
            sources.iter().zip(&embedding_data.buffers).enumerate()
        {
            let Some(texture_state) = texture.state() else {
                continue;
            };
            let size = texture_state.texture().size();
            pending_downloads.push(self.copy_buffer_to_shmem(source_idx, size, buffer.clone()));
        }

        self.wgpu_ctx.device.poll(wgpu::PollType::Wait)?;

        for pending in pending_downloads {
            pending()?;
        }

        self.chromium_sender
            .embed_sources(sources, embedding_data.children_ids.clone())
            .map_err(EmbedError::ChromiumSenderError)
    }

    fn copy_sources_to_buffers(
        &self,
        sources: &[&NodeTexture],
        buffers: &[Arc<wgpu::Buffer>],
    ) -> Result<(), EmbedError> {
        let mut encoder = self
            .wgpu_ctx
            .device
            .create_command_encoder(&Default::default());

        for (texture, buffer) in sources.iter().zip(buffers) {
            let Some(texture_state) = texture.state() else {
                continue;
            };
            texture_state.texture().copy_to_buffer(&mut encoder, buffer);
        }
        self.wgpu_ctx.queue.submit(Some(encoder.finish()));

        Ok(())
    }

    fn copy_buffer_to_shmem(
        &self,
        source_idx: usize,
        size: wgpu::Extent3d,
        buffer: Arc<wgpu::Buffer>,
    ) -> impl FnOnce() -> Result<(), EmbedError> + '_ {
        let (s, r) = bounded(1);
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                if let Err(err) = s.send(result) {
                    error!("channel send error: {err}")
                }
            });

        move || {
            r.recv().unwrap()?;

            self.chromium_sender
                .update_shared_memory(source_idx, buffer.clone(), size)?;
            buffer.unmap();

            Ok(())
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Zeroable, bytemuck::Pod)]
pub(super) struct RenderInfo {
    is_website_texture: i32,
    _padding: [i32; 3],
    transformation_matrix: [[f32; 4]; 4],
}

impl RenderInfo {
    pub fn website() -> Self {
        Self {
            is_website_texture: 1,
            _padding: Default::default(),
            transformation_matrix: Mat4::identity().transpose().into(),
        }
    }

    pub fn source_transform(transformation_matrix: &Mat4) -> Self {
        Self {
            is_website_texture: 0,
            _padding: [0; 3],
            transformation_matrix: transformation_matrix.transpose().into(),
        }
    }

    pub fn bytes(self) -> Bytes {
        Bytes::copy_from_slice(bytemuck::cast_slice(&[self]))
    }

    pub fn size() -> u32 {
        std::mem::size_of::<RenderInfo>() as u32
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EmbedError {
    #[error("Failed to download source frame")]
    DownloadFrame(#[from] wgpu::BufferAsyncError),

    #[error(transparent)]
    ChromiumSenderError(#[from] ChromiumSenderError),

    #[error("Failed to poll the GPU")]
    PollError(#[from] wgpu::PollError),
}
