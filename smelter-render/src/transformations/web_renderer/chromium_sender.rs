use std::sync::Arc;

use crate::{
    RendererId, Resolution, scene::ComponentId, state::node_texture::NodeTexture,
    transformations::web_renderer::chromium_context::ChromiumContext,
};
use crossbeam_channel::{Receiver, Sender};
use tracing::error;

use super::{
    WebRendererSpec, browser_client::BrowserClient, chromium_sender_thread::ChromiumSenderThread,
};

#[derive(Debug)]
pub(super) struct ChromiumSender {
    message_sender: Sender<ChromiumSenderMessage>,
    /// Used for synchronizing buffer map and unmap operations
    unmap_signal_receiver: Receiver<()>,
}

impl Drop for ChromiumSender {
    fn drop(&mut self) {
        if let Err(err) = self.message_sender.send(ChromiumSenderMessage::Quit) {
            error!("Failed to close ChromiumSenderThread: {err}");
        }
    }
}

impl ChromiumSender {
    pub fn new(
        chromium_context: Arc<ChromiumContext>,
        instance_id: &RendererId,
        spec: &WebRendererSpec,
        browser_client: BrowserClient,
    ) -> Self {
        let (message_sender, message_receiver) = crossbeam_channel::unbounded();
        let (unmap_signal_sender, unmap_signal_receiver) = crossbeam_channel::bounded(0);

        ChromiumSenderThread::new(
            chromium_context,
            instance_id,
            spec,
            browser_client,
            message_receiver,
            unmap_signal_sender,
        )
        .spawn();

        Self {
            message_sender,
            unmap_signal_receiver,
        }
    }

    pub fn embed_sources(
        &self,
        sources: &[&NodeTexture],
        children_ids: Vec<ComponentId>,
    ) -> Result<(), ChromiumSenderError> {
        let resolutions = sources.iter().map(|texture| texture.resolution()).collect();
        self.message_sender
            .send(ChromiumSenderMessage::EmbedSources {
                resolutions,
                children_ids,
            })
            .map_err(|_| ChromiumSenderError::MessageChannelDisconnected)
    }

    pub fn ensure_shared_memory(
        &self,
        sources: &[&NodeTexture],
    ) -> Result<(), ChromiumSenderError> {
        let resolutions = sources.iter().map(|texture| texture.resolution()).collect();
        self.message_sender
            .send(ChromiumSenderMessage::EnsureSharedMemory { resolutions })
            .map_err(|_| ChromiumSenderError::MessageChannelDisconnected)
    }

    pub fn update_shared_memory(
        &self,
        source_idx: usize,
        buffer: Arc<wgpu::Buffer>,
        size: wgpu::Extent3d,
    ) -> Result<(), ChromiumSenderError> {
        let info = UpdateSharedMemoryInfo {
            source_idx,
            buffer,
            size,
        };

        self.message_sender
            .send(ChromiumSenderMessage::UpdateSharedMemory(info))
            .map_err(|_| ChromiumSenderError::MessageChannelDisconnected)?;

        // Wait until buffer unmap is possible
        self.unmap_signal_receiver
            .recv()
            .map_err(|_| ChromiumSenderError::MessageChannelDisconnected)
    }

    pub fn request_frame_positions(
        &self,
        children_ids: Vec<ComponentId>,
    ) -> Result<(), ChromiumSenderError> {
        self.message_sender
            .send(ChromiumSenderMessage::GetFramePositions { children_ids })
            .map_err(|_| ChromiumSenderError::MessageChannelDisconnected)
    }
}

pub(super) enum ChromiumSenderMessage {
    EmbedSources {
        resolutions: Vec<Option<Resolution>>,
        children_ids: Vec<ComponentId>,
    },
    EnsureSharedMemory {
        resolutions: Vec<Option<Resolution>>,
    },
    UpdateSharedMemory(UpdateSharedMemoryInfo),
    GetFramePositions {
        children_ids: Vec<ComponentId>,
    },
    Quit,
}

pub(super) struct UpdateSharedMemoryInfo {
    pub source_idx: usize,
    pub buffer: Arc<wgpu::Buffer>,
    pub size: wgpu::Extent3d,
}

#[derive(Debug, thiserror::Error)]
pub enum ChromiumSenderError {
    #[error("Chromium message channel is disconnected")]
    MessageChannelDisconnected,
}
