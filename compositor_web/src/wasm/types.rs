use std::time::Duration;

use compositor_render::{error::ErrorStack, web_renderer::WebRendererInitOptions, InputId};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use wasm_bindgen::prelude::*;

#[derive(Debug, Deserialize)]
pub struct RendererOptions {
    pub stream_fallback_timeout_ms: u64,
    pub logger_level: LoggerLevel,
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

#[wasm_bindgen]
pub struct FrameSet {
    pub pts_ms: f64,

    #[wasm_bindgen(skip)]
    pub frames: js_sys::Map,
}

#[wasm_bindgen]
impl FrameSet {
    #[wasm_bindgen(constructor)]
    pub fn new(pts_ms: f64, frames: js_sys::Map) -> Self {
        Self { pts_ms, frames }
    }

    #[wasm_bindgen(getter)]
    pub fn frames(&self) -> js_sys::Map {
        self.frames.clone()
    }

    #[wasm_bindgen(setter)]
    pub fn set_frames(&mut self, frames: js_sys::Map) {
        self.frames = frames;
    }
}

pub struct InputFrame {
    pub id: InputId,
    pub frame: web_sys::ImageBitmap,
    #[allow(dead_code)]
    pub pts: Duration,
}

#[wasm_bindgen]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FrameFormat {
    RgbaBytes,
    YuvBytes,
}

impl From<FrameFormat> for compositor_render::OutputFrameFormat {
    fn from(value: FrameFormat) -> Self {
        match value {
            FrameFormat::RgbaBytes => compositor_render::OutputFrameFormat::RgbaWgpuTexture,
            FrameFormat::YuvBytes => compositor_render::OutputFrameFormat::PlanarYuv420Bytes,
        }
    }
}

impl From<RendererOptions> for compositor_render::RendererOptions {
    fn from(value: RendererOptions) -> Self {
        Self {
            web_renderer: WebRendererInitOptions {
                enable: false,
                enable_gpu: false,
            },
            // Framerate is only required by web renderer which is not used
            framerate: compositor_render::Framerate { num: 30, den: 1 },
            stream_fallback_timeout: Duration::from_millis(value.stream_fallback_timeout_ms),
            force_gpu: false,
            wgpu_features: wgpu::Features::empty(),
            wgpu_ctx: None,
            load_system_fonts: true,
            rendering_mode: compositor_render::RenderingMode::WebGl,
        }
    }
}

impl TryFrom<JsValue> for InputFrame {
    type Error = JsValue;

    fn try_from(entry: JsValue) -> Result<Self, Self::Error> {
        // 0 - map key
        let id = js_sys::Reflect::get_u32(&entry, 0)?
            .as_string()
            .ok_or(JsValue::from_str("Expected string used as a key"))?;
        let id = InputId(id.into());

        // 1 - map value
        let value = js_sys::Reflect::get_u32(&entry, 1)?;
        let frame: web_sys::ImageBitmap = js_sys::Reflect::get(&value, &"frame".into())?.into();
        let pts = Duration::from_secs_f64(
            js_sys::Reflect::get(&value, &"ptsMs".into())?
                .as_f64()
                .unwrap_or(0.0)
                / 1000.0,
        );
        Ok(Self { id, frame, pts })
    }
}

pub fn from_js_value<T: DeserializeOwned>(value: JsValue) -> Result<T, JsValue> {
    serde_wasm_bindgen::from_value(value).map_err(to_js_error)
}

pub fn to_js_error(error: impl std::error::Error + 'static) -> JsValue {
    let error_stack = ErrorStack::new(&error);
    JsValue::from_str(&error_stack.into_string())
}
