use serde::{de::DeserializeOwned, Deserialize, Serialize};
use smelter_render::{error::ErrorStack, InputId, OutputId, Resolution};
use std::{sync::Arc, time::Duration};
use wasm_bindgen::prelude::*;

pub struct WgpuCtx {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

#[derive(Debug, Deserialize)]
pub struct RendererOptions {
    pub stream_fallback_timeout_ms: u64,
    pub logger_level: LoggerLevel,
    /// On most platforms it's more performant to copy input VideoFrame data to CPU
    /// and then upload it to texture. But on macOS using dedicated wgpu copy_external_image_to_texture function
    /// results in better performance.
    pub upload_frames_with_copy_external: bool,
}

#[derive(Debug, Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum LoggerLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl From<LoggerLevel> for tracing::Level {
    fn from(value: LoggerLevel) -> Self {
        match value {
            LoggerLevel::Error => tracing::Level::ERROR,
            LoggerLevel::Warn => tracing::Level::WARN,
            LoggerLevel::Info => tracing::Level::INFO,
            LoggerLevel::Debug => tracing::Level::DEBUG,
            LoggerLevel::Trace => tracing::Level::TRACE,
        }
    }
}

pub struct InputFrameSet {
    pub pts: Duration,
    pub frames: Vec<InputFrame>,
}

pub struct InputFrame {
    pub id: InputId,
    pub pts: Duration,
    pub frame: InputFrameKind,
}

pub enum InputFrameKind {
    VideoFrame(web_sys::VideoFrame),
    HtmlVideoElement(web_sys::HtmlVideoElement),
}

pub struct OutputFrameSet {
    pub pts: Duration,
    pub frames: Vec<OutputFrame>,
}

pub struct OutputFrame {
    pub output_id: OutputId,
    pub resolution: Resolution,
    pub data: wasm_bindgen::Clamped<Vec<u8>>,
}

#[wasm_bindgen]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FrameFormat {
    RgbaBytes,
    YuvBytes,
}

impl From<FrameFormat> for smelter_render::OutputFrameFormat {
    fn from(value: FrameFormat) -> Self {
        match value {
            FrameFormat::RgbaBytes => smelter_render::OutputFrameFormat::RgbaWgpuTexture,
            FrameFormat::YuvBytes => smelter_render::OutputFrameFormat::PlanarYuv420Bytes,
        }
    }
}

pub fn new_render_options(
    value: RendererOptions,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
) -> smelter_render::RendererOptions {
    smelter_render::RendererOptions {
        chromium_context: None,
        // Framerate is only required by web renderer which is not used
        framerate: smelter_render::Framerate { num: 30, den: 1 },
        stream_fallback_timeout: Duration::from_millis(value.stream_fallback_timeout_ms),
        load_system_fonts: true,
        rendering_mode: smelter_render::RenderingMode::WebGl,

        device,
        queue,
    }
}

pub fn from_js_value<T: DeserializeOwned>(value: JsValue) -> Result<T, JsValue> {
    serde_wasm_bindgen::from_value(value).map_err(to_js_error)
}

pub fn to_js_error(error: impl std::error::Error + 'static) -> JsValue {
    let error_stack = ErrorStack::new(&error);
    JsValue::from_str(&error_stack.into_string())
}
