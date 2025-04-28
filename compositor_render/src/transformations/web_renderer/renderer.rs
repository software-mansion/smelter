use std::sync::{Arc, Mutex};

use bytes::Bytes;
use log::info;

use crate::{
    state::{RegisterCtx, RenderCtx},
    transformations::web_renderer::{
        browser_client::BrowserClient, chromium_sender::ChromiumSender,
    },
    wgpu::{
        common_pipeline::CreateShaderError,
        texture::{BGRATexture, NodeTexture, Texture},
    },
    RendererId, Resolution,
};

use super::{
    embedder::{EmbedError, EmbeddingHelper, RenderInfo},
    node::EmbeddingData,
    shader::WebRendererShader,
    FrameData, SourceTransforms, WebEmbeddingMethod, WebRendererSpec,
};

#[derive(Debug)]
pub struct WebRenderer {
    spec: WebRendererSpec,
    frame_data: FrameData,
    source_transforms: SourceTransforms,
    embedding_helper: EmbeddingHelper,

    website_texture: BGRATexture,
    render_website_shader: WebRendererShader,
}

impl WebRenderer {
    pub fn new(
        ctx: &RegisterCtx,
        instance_id: &RendererId,
        spec: WebRendererSpec,
    ) -> Result<Self, CreateWebRendererError> {
        if ctx.chromium.context.is_none() {
            return Err(CreateWebRendererError::WebRendererDisabled);
        }

        info!("Starting web renderer for {}", &spec.url);

        let frame_data = Arc::new(Mutex::new(Bytes::new()));
        let source_transforms = Arc::new(Mutex::new(Vec::new()));

        let client = BrowserClient::new(
            frame_data.clone(),
            source_transforms.clone(),
            spec.resolution,
        );
        let chromium_sender = ChromiumSender::new(ctx, instance_id, &spec, client);
        let embedding_helper = EmbeddingHelper::new(ctx, chromium_sender, spec.embedding_method);
        let render_website_shader = WebRendererShader::new(&ctx.wgpu_ctx)?;
        let website_texture = BGRATexture::new(&ctx.wgpu_ctx, spec.resolution);

        Ok(Self {
            spec,
            frame_data,
            source_transforms,
            embedding_helper,
            website_texture,
            render_website_shader,
        })
    }

    pub fn render(
        &self,
        ctx: &RenderCtx,
        sources: &[&NodeTexture],
        embedding_data: &EmbeddingData,
        target: &mut NodeTexture,
    ) -> Result<(), RenderWebsiteError> {
        self.embedding_helper
            .prepare_embedding(sources, embedding_data)
            .map_err(|err| RenderWebsiteError::EmbeddingFailed(self.spec.url.clone(), err))?;

        if let Some(frame) = self.retrieve_frame() {
            let target = target.ensure_size(ctx.wgpu_ctx, self.spec.resolution);
            self.website_texture.upload(ctx.wgpu_ctx, &frame);

            let render_textures = self.prepare_textures(sources);

            self.render_website_shader
                .render(ctx.wgpu_ctx, &render_textures, target);
        }

        Ok(())
    }

    fn prepare_textures<'a>(
        &'a self,
        sources: &'a [&NodeTexture],
    ) -> Vec<(Option<&'a Texture>, RenderInfo)> {
        let mut source_info = sources
            .iter()
            .zip(self.source_transforms.lock().unwrap().iter())
            .map(|(node_texture, transform)| {
                (
                    node_texture.texture(),
                    RenderInfo::source_transform(transform),
                )
            })
            .collect();

        let website_info = (Some(self.website_texture.texture()), RenderInfo::website());

        let mut result = Vec::new();
        match self.spec.embedding_method {
            WebEmbeddingMethod::NativeEmbeddingOverContent => {
                result.push(website_info);
                result.append(&mut source_info);
            }
            WebEmbeddingMethod::NativeEmbeddingUnderContent => {
                result.append(&mut source_info);
                result.push(website_info);
            }
            WebEmbeddingMethod::ChromiumEmbedding => {
                result.push(website_info);
            }
        };

        result
    }

    fn retrieve_frame(&self) -> Option<Bytes> {
        let frame_data = self.frame_data.lock().unwrap();
        if frame_data.is_empty() {
            return None;
        }
        Some(frame_data.clone())
    }

    pub fn resolution(&self) -> Resolution {
        self.spec.resolution
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CreateWebRendererError {
    #[error(transparent)]
    CreateShaderFailed(#[from] CreateShaderError),

    #[error("Web rendering can not be used because it was disabled in the init request")]
    WebRendererDisabled,
}

#[derive(Debug, thiserror::Error)]
pub enum RenderWebsiteError {
    #[error("Failed to embed source on the website \"{0}\": {1}")]
    EmbeddingFailed(String, #[source] EmbedError),
}
