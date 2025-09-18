use std::sync::Arc;

use crate::{
    scene::ComponentId,
    state::{node_texture::NodeTexture, RegisterCtx, RenderCtx},
    Framerate, RendererId, Resolution,
};

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

#[derive(Debug)]
pub struct WebRenderer {
    spec: WebRendererSpec,
}

impl WebRenderer {
    pub fn new(
        _ctx: &RegisterCtx,
        _instance_id: &RendererId,
        _spec: WebRendererSpec,
    ) -> Result<Self, CreateWebRendererError> {
        Err(CreateWebRendererError::WebRenderingNotAvailable)
    }

    pub fn resolution(&self) -> Resolution {
        self.spec.resolution
    }
}

pub struct WebRendererNode {
    _internal: (),
}

impl WebRendererNode {
    pub fn new(_children_ids: Vec<ComponentId>, _renderer: Arc<WebRenderer>) -> Self {
        // it will never be called because it is impossible to instantiate WebRenderer
        unreachable!();
    }

    pub fn render(
        &mut self,
        _ctx: &mut RenderCtx,
        _sources: &[&NodeTexture],
        _target: &mut NodeTexture,
    ) {
        unreachable!()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CreateWebRendererError {
    #[error("Web rendering feature is not available")]
    WebRenderingNotAvailable,
}

#[derive(Debug)]
pub struct ChromiumContext {
    _internal: (),
}

#[derive(Debug, thiserror::Error)]
#[error("Web rendering feature is not available")]
pub struct ChromiumContextInitError;

impl ChromiumContext {
    pub fn new(
        _framerate: Framerate,
        _enable_gpu: bool,
    ) -> Result<Arc<Self>, ChromiumContextInitError> {
        Err(ChromiumContextInitError)
    }

    pub fn run_event_loop(&self) -> Result<(), ChromiumContextInitError> {
        unreachable!()
    }

    pub fn run_event_loop_single_iter(&self) -> Result<(), ChromiumContextInitError> {
        unreachable!()
    }
}
