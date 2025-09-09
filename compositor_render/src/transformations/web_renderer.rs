use crate::Resolution;
use bytes::Bytes;
use nalgebra_glm::Mat4;
use std::sync::{Arc, Mutex};

mod renderer;

pub mod process_helper;

mod transformation_matrices;
mod utils;

pub use renderer::*;

mod browser_client;
mod chromium_context;
mod chromium_sender;
mod chromium_sender_thread;
mod embedder;
mod node;
mod shader;
mod shared_memory;

pub const EMBED_SOURCE_FRAMES_MESSAGE: &str = "EMBED_SOURCE_FRAMES";
pub const UNEMBED_SOURCE_FRAMES_MESSAGE: &str = "UNEMBED_SOURCE_FRAMES";
pub const GET_FRAME_POSITIONS_MESSAGE: &str = "GET_FRAME_POSITIONS";

pub(super) type FrameData = Arc<Mutex<Bytes>>;
pub(super) type SourceTransforms = Arc<Mutex<Vec<Mat4>>>;

pub(crate) use node::WebRendererNode;

pub use chromium_context::{ChromiumContext, ChromiumContextInitError};

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
