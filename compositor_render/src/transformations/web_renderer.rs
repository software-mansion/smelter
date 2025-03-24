use crate::Resolution;
use bytes::Bytes;
use nalgebra_glm::Mat4;
use std::sync::{Arc, Mutex};

#[cfg(feature = "web_renderer")]
mod renderer;

#[cfg(feature = "web_renderer")]
pub mod process_helper;

#[cfg(not(feature = "web_renderer"))]
#[path = "web_renderer/disabled_renderer.rs"]
mod renderer;

mod tranformation_matrices;
mod utils;

pub use renderer::*;

pub mod chromium_context;
pub(crate) mod node;

#[cfg(feature = "web_renderer")]
pub mod browser_client;
#[cfg(feature = "web_renderer")]
mod chromium_sender;
#[cfg(feature = "web_renderer")]
mod chromium_sender_thread;
#[cfg(feature = "web_renderer")]
mod embedder;
#[cfg(feature = "web_renderer")]
mod shader;
#[cfg(feature = "web_renderer")]
mod shared_memory;

pub const EMBED_SOURCE_FRAMES_MESSAGE: &str = "EMBED_SOURCE_FRAMES";
pub const UNEMBED_SOURCE_FRAMES_MESSAGE: &str = "UNEMBED_SOURCE_FRAMES";
pub const GET_FRAME_POSITIONS_MESSAGE: &str = "GET_FRAME_POSITIONS";

pub(super) type FrameData = Arc<Mutex<Bytes>>;
pub(super) type SourceTransforms = Arc<Mutex<Vec<Mat4>>>;

#[derive(Debug, Clone, Copy)]
pub struct WebRendererInitOptions {
    pub enable: bool,
    pub enable_gpu: bool,
}

#[derive(Debug, Clone)]
pub struct WebRendererSpec {
    pub url: String,
    pub resolution: Resolution,
    pub embedding_method: WebEmbeddingMethod,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WebEmbeddingMethod {
    /// Send frames to chromium directly and render it on canvas
    ChromiumEmbedding,

    /// Render sources on top of the rendered website
    NativeEmbeddingOverContent,

    /// Render sources below the website.
    /// The website's background has to be transparent
    NativeEmbeddingUnderContent,
}
