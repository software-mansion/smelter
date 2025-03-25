pub mod error;
pub mod event_handler;
pub mod scene;

pub(crate) mod registry;
pub(crate) mod transformations;
pub(crate) mod utils;
pub(crate) mod wgpu;

mod event_loop;
mod state;
mod types;

pub use event_loop::{EventLoop, EventLoopError};
pub use types::*;

pub use registry::RegistryType;
pub use state::Renderer;
pub use state::RendererOptions;
pub use state::RendererSpec;

pub use wgpu::WgpuFeatures;
pub use wgpu::{create_wgpu_ctx, required_wgpu_features, set_required_wgpu_limits, WgpuComponents};

pub mod image {
    pub use crate::transformations::image_renderer::{ImageSource, ImageSpec, ImageType};
}

pub mod shader {
    pub use crate::transformations::shader::ShaderSpec;
}

pub mod web_renderer {
    pub use crate::transformations::web_renderer::{
        WebEmbeddingMethod, WebRendererInitOptions, WebRendererSpec, EMBED_SOURCE_FRAMES_MESSAGE,
        GET_FRAME_POSITIONS_MESSAGE, UNEMBED_SOURCE_FRAMES_MESSAGE,
    };

    #[cfg(feature = "web_renderer")]
    pub use crate::transformations::web_renderer::process_helper;
}
