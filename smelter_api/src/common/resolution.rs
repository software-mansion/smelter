use compositor_render::scene;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct Resolution {
    /// Width in pixels.
    pub width: usize,
    /// Height in pixels.
    pub height: usize,
}

impl From<Resolution> for compositor_render::Resolution {
    fn from(resolution: Resolution) -> Self {
        Self {
            width: resolution.width,
            height: resolution.height,
        }
    }
}

impl From<Resolution> for scene::Size {
    fn from(resolution: Resolution) -> Self {
        Self {
            width: resolution.width as f32,
            height: resolution.height as f32,
        }
    }
}
