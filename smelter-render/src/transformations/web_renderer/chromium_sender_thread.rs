use std::path::PathBuf;
use std::{
    sync::Arc,
    thread::{self, JoinHandle},
};

use crossbeam_channel::{Receiver, Sender};
use log::error;

use crate::error::ErrorStack;
use crate::scene::ComponentId;
use crate::transformations::web_renderer::UNEMBED_SOURCE_FRAMES_MESSAGE;
use crate::transformations::web_renderer::chromium_sender::{
    ChromiumSenderMessage, UpdateSharedMemoryInfo,
};
use crate::transformations::web_renderer::shared_memory::{SharedMemory, SharedMemoryError};
use crate::wgpu::texture::utils::pad_to_256;
use crate::{RendererId, Resolution};

use super::{EMBED_SOURCE_FRAMES_MESSAGE, GET_FRAME_POSITIONS_MESSAGE, WebRendererSpec, utils};
use super::{browser_client::BrowserClient, chromium_context::ChromiumContext};

pub(super) struct ChromiumSenderThread {
    chromium_context: Arc<ChromiumContext>,
    url: String,
    web_renderer_id: RendererId,
    browser_client: BrowserClient,

    message_receiver: Receiver<ChromiumSenderMessage>,
    unmap_signal_sender: Sender<()>,
}

impl ChromiumSenderThread {
    pub fn new(
        chromium_context: Arc<ChromiumContext>,
        instance_id: &RendererId,
        spec: &WebRendererSpec,
        browser_client: BrowserClient,
        message_receiver: Receiver<ChromiumSenderMessage>,
        unmap_signal_sender: Sender<()>,
    ) -> Self {
        Self {
            chromium_context,
            url: spec.url.clone(),
            web_renderer_id: instance_id.clone(),
            browser_client,
            message_receiver,
            unmap_signal_sender,
        }
    }

    pub fn spawn(mut self) -> JoinHandle<()> {
        thread::spawn(move || self.run())
    }

    fn run(&mut self) {
        let Ok(browser) = self
            .chromium_context
            .start_browser(&self.url, self.browser_client.clone())
        else {
            error!("Couldn't start browser for {}", self.url);
            return;
        };

        let mut state = ThreadState::new(
            browser,
            self.chromium_context.instance_id(),
            &self.web_renderer_id,
        );
        loop {
            let result = match self.message_receiver.recv().unwrap() {
                ChromiumSenderMessage::EmbedSources {
                    resolutions,
                    children_ids,
                } => self.embed_frames(&mut state, resolutions, children_ids),
                ChromiumSenderMessage::EnsureSharedMemory { resolutions } => {
                    self.ensure_shared_memory(&mut state, resolutions)
                }
                ChromiumSenderMessage::UpdateSharedMemory(info) => {
                    self.update_shared_memory(&mut state, info)
                }
                ChromiumSenderMessage::GetFramePositions { children_ids } => {
                    self.get_frame_positions(&state, children_ids)
                }
                ChromiumSenderMessage::Quit => return,
            };

            if let Err(err) = result {
                error!(
                    "Error occurred in chromium sender thread.\n{}",
                    ErrorStack::new(&err).into_string()
                );
            }
        }
    }

    fn embed_frames(
        &self,
        state: &mut ThreadState,
        resolutions: Vec<Option<Resolution>>,
        children_ids: Vec<ComponentId>,
    ) -> Result<(), ChromiumSenderThreadError> {
        let mut process_message = libcef::ProcessMessageBuilder::new(EMBED_SOURCE_FRAMES_MESSAGE);

        // IPC message to chromium renderer subprocess consists of:
        // - shared memory path
        // - ID attribute of an HTML element
        // - texture width
        // - texture height
        for (source_idx, (resolution, id)) in resolutions.into_iter().zip(children_ids).enumerate()
        {
            let Some(Resolution { width, height }) = resolution else {
                continue;
            };

            process_message.write_string(state.shared_memory(source_idx)?.to_path_string())?;
            process_message.write_string(id.to_string())?;
            process_message.write_int(width as i32)?;
            process_message.write_int(height as i32)?;
        }

        let frame = state.browser.main_frame()?;
        frame.send_process_message(libcef::ProcessId::Renderer, process_message.build())?;

        Ok(())
    }

    fn ensure_shared_memory(
        &self,
        state: &mut ThreadState,
        resolutions: Vec<Option<Resolution>>,
    ) -> Result<(), ChromiumSenderThreadError> {
        fn size_from_resolution(resolution: Option<Resolution>) -> usize {
            match resolution {
                Some(res) => 4 * res.width * res.height,
                None => 1,
            }
        }

        let frame = state.browser.main_frame()?;
        let shared_memory = &mut state.shared_memory;

        // Ensure size for already existing shared memory
        for (resolution, shmem) in resolutions.iter().zip(shared_memory.iter_mut()) {
            let size = size_from_resolution(*resolution);
            // We resize shared memory only when it's too small.
            // This avoids some crashes caused by the lack of resize synchronization.
            if shmem.len() < size {
                // TODO: This should be synchronised
                let mut process_message =
                    libcef::ProcessMessage::new(UNEMBED_SOURCE_FRAMES_MESSAGE);
                process_message.write_string(0, shmem.to_path_string())?;
                frame.send_process_message(libcef::ProcessId::Renderer, process_message)?;
                // -----

                shmem.resize(size)?;
            }
        }

        // Create additional shared memory
        for (source_idx, resolution) in resolutions
            .into_iter()
            .enumerate()
            .skip(shared_memory.len())
        {
            let size = size_from_resolution(resolution);
            shared_memory.push(SharedMemory::new(
                &state.shared_memory_root_path,
                source_idx,
                size,
            )?);
        }

        Ok(())
    }

    // TODO: Synchronize shared memory access
    fn update_shared_memory(
        &self,
        state: &mut ThreadState,
        info: UpdateSharedMemoryInfo,
    ) -> Result<(), ChromiumSenderThreadError> {
        let shared_memory = state.shared_memory(info.source_idx)?;

        // Writes buffer data to shared memory
        {
            let range = info.buffer.slice(..).get_mapped_range();
            let chunks = range.chunks((pad_to_256(4 * info.size.width)) as usize);
            for (i, chunk) in chunks.enumerate() {
                let bytes_len = (4 * info.size.width) as usize;
                shared_memory.write(&chunk[..bytes_len], i * bytes_len)?;
            }
        }

        self.unmap_signal_sender.send(()).unwrap();
        Ok(())
    }

    fn get_frame_positions(
        &self,
        state: &ThreadState,
        children_ids: Vec<ComponentId>,
    ) -> Result<(), ChromiumSenderThreadError> {
        let mut message = libcef::ProcessMessage::new(GET_FRAME_POSITIONS_MESSAGE);
        for (index, id) in children_ids.into_iter().enumerate() {
            message.write_string(index, id.to_string())?;
        }

        let frame = state.browser.main_frame()?;
        frame.send_process_message(libcef::ProcessId::Renderer, message)?;

        Ok(())
    }
}

struct ThreadState {
    browser: libcef::Browser,
    shared_memory: Vec<SharedMemory>,
    shared_memory_root_path: PathBuf,
}

impl Drop for ThreadState {
    fn drop(&mut self) {
        if let Err(err) = self.browser.close() {
            error!("Failed to close browser: {err}")
        }
    }
}

impl ThreadState {
    fn new(browser: libcef::Browser, compositor_id: &str, web_renderer_id: &RendererId) -> Self {
        let shared_memory_root_path =
            utils::get_smelter_instance_tmp_path(compositor_id).join(web_renderer_id.to_string());
        let shared_memory = Vec::new();

        Self {
            browser,
            shared_memory,
            shared_memory_root_path,
        }
    }

    fn shared_memory(
        &mut self,
        source_idx: usize,
    ) -> Result<&mut SharedMemory, ChromiumSenderThreadError> {
        self.shared_memory
            .get_mut(source_idx)
            .ok_or(ChromiumSenderThreadError::SharedMemoryNotAllocated { source_idx })
    }
}

#[derive(Debug, thiserror::Error)]
enum ChromiumSenderThreadError {
    #[error("Browser is no longer alive")]
    BrowserNotAlive(#[from] libcef::BrowserError),

    #[error("Browser frame is no longer alive")]
    FrameNotAlive(#[from] libcef::FrameError),

    #[error(transparent)]
    SharedMemoryError(#[from] SharedMemoryError),

    #[error("Shared memory should already be allocated for \"{source_idx}\" input")]
    SharedMemoryNotAllocated { source_idx: usize },

    #[error(transparent)]
    ProcessMessageError(#[from] libcef::ProcessMessageError),
}
