use std::fmt::Display;
use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

mod audio;
mod component;
#[cfg(not(target_arch = "wasm32"))]
mod from_audio;
mod from_color;
mod from_component;
#[cfg(not(target_arch = "wasm32"))]
mod from_register_input;
#[cfg(not(target_arch = "wasm32"))]
mod from_register_output;
mod from_renderer;
mod from_types;
mod from_util;
#[cfg(not(target_arch = "wasm32"))]
mod from_video;
mod register_input;
mod register_output;
mod renderer;
mod util;
mod video;

#[cfg(test)]
mod from_util_test;

pub use component::Component;
pub use component::Image;
pub use component::InputStream;
pub use component::Rescaler;
pub use component::Shader;
pub use component::Text;
pub use component::Tiles;
pub use component::View;
pub use component::WebView;

pub use register_input::DeckLink;
pub use register_input::Mp4Input;
pub use register_input::RtpInput;
pub use register_input::WhipInput;
pub use register_output::Mp4Output;
pub use register_output::RtmpOutput;
pub use register_output::RtpOutput;
pub use register_output::WhipOutput;

pub use renderer::ImageSpec;
pub use renderer::ShaderSpec;
pub use renderer::WebRendererSpec;

pub use util::Resolution;
pub use util::TypeError;

pub use audio::Audio;
pub use audio::MixingStrategy;

pub use video::Video;

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct ComponentId(Arc<str>);

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct RendererId(Arc<str>);

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct OutputId(Arc<str>);

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct InputId(Arc<str>);

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateOutputRequest {
    pub video: Option<Video>,
    pub audio: Option<Audio>,
    pub schedule_time_ms: Option<f64>,
}

impl Display for InputId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl Display for OutputId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
