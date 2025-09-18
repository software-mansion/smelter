use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use smelter_render::web_renderer;

use crate::*;

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WebRendererSpec {
    /// Url of a website that you want to render.
    pub url: String,
    /// Resolution.
    pub resolution: Resolution,
    /// Mechanism used to render input frames on the website.
    pub embedding_method: Option<WebEmbeddingMethod>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WebEmbeddingMethod {
    /// Pass raw input frames as JS buffers so they can be rendered, for example, using a `<canvas>` component.
    /// :::warning
    /// This method might have a significant performance impact, especially for a large number of inputs.
    /// :::
    ChromiumEmbedding,

    /// Render a website without any inputs and overlay them over the website content.
    NativeEmbeddingOverContent,

    /// Render a website without any inputs and overlay them under the website content.
    NativeEmbeddingUnderContent,
}

impl TryFrom<WebRendererSpec> for smelter_render::RendererSpec {
    type Error = TypeError;

    fn try_from(spec: WebRendererSpec) -> Result<Self, Self::Error> {
        let embedding_method = match spec.embedding_method {
            Some(WebEmbeddingMethod::ChromiumEmbedding) => {
                web_renderer::WebEmbeddingMethod::ChromiumEmbedding
            }
            Some(WebEmbeddingMethod::NativeEmbeddingOverContent) => {
                web_renderer::WebEmbeddingMethod::NativeEmbeddingOverContent
            }
            Some(WebEmbeddingMethod::NativeEmbeddingUnderContent) => {
                web_renderer::WebEmbeddingMethod::NativeEmbeddingUnderContent
            }
            None => web_renderer::WebEmbeddingMethod::NativeEmbeddingOverContent,
        };

        let spec = web_renderer::WebRendererSpec {
            url: spec.url,
            resolution: spec.resolution.into(),
            embedding_method,
        };
        Ok(Self::WebRenderer(spec))
    }
}
