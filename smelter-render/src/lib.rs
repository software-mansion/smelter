pub mod error;
pub mod event_handler;
pub mod scene;

pub(crate) mod registry;
pub(crate) mod transformations;
pub(crate) mod wgpu;

mod state;
mod types;

pub use types::*;

pub use registry::RegistryType;
pub use state::Renderer;
pub use state::RendererOptions;
pub use state::RendererSpec;

pub use wgpu::{WgpuFeatures, required_wgpu_features, set_required_wgpu_limits};

pub mod image {
    pub use crate::transformations::image::{ImageSource, ImageSpec, ImageType};
}

pub mod shader {
    pub use crate::transformations::shader::ShaderSpec;
}

pub mod web_renderer {
    pub use crate::transformations::web_renderer::{
        ChromiumContext, ChromiumContextInitError, WebEmbeddingMethod, WebRendererSpec,
    };

    #[cfg(feature = "web-renderer")]
    pub use crate::transformations::web_renderer::process_helper;
}
