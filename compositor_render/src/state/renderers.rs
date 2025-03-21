use std::sync::Arc;

use crate::{
    error::InitRendererEngineError,
    registry::{RegistryType, RendererRegistry},
    transformations::{
        image::Image, layout::LayoutRenderer, shader::Shader, web_renderer::WebRenderer,
    },
};

use super::WgpuCtx;

pub(crate) struct Renderers {
    pub(crate) shaders: RendererRegistry<Arc<Shader>>,
    pub(crate) web_renderers: RendererRegistry<Arc<WebRenderer>>,
    pub(crate) images: RendererRegistry<Image>,
    pub(crate) layout: LayoutRenderer,
}

impl Renderers {
    pub fn new(wgpu_ctx: Arc<WgpuCtx>) -> Result<Self, InitRendererEngineError> {
        Ok(Self {
            shaders: RendererRegistry::new(RegistryType::Shader),
            web_renderers: RendererRegistry::new(RegistryType::WebRenderer),
            images: RendererRegistry::new(RegistryType::Image),
            layout: LayoutRenderer::new(&wgpu_ctx)
                .map_err(InitRendererEngineError::LayoutTransformationsInitError)?,
        })
    }
}
