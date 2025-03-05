use std::time::Duration;

use compositor_render::{
    error::ErrorStack, web_renderer::WebRendererInitOptions, InputId, Resolution,
};
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

impl From<LoggerLevel> for log::Level {
    fn from(value: LoggerLevel) -> Self {
        match value {
            LoggerLevel::Error => log::Level::Error,
            LoggerLevel::Warn => log::Level::Warn,
            LoggerLevel::Info => log::Level::Info,
            LoggerLevel::Debug => log::Level::Debug,
            LoggerLevel::Trace => log::Level::Trace,
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

pub struct Frame {
    pub id: InputId,
    pub resolution: Resolution,
    pub format: FrameFormat,
    pub data: Vec<u8>,
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
        }
    }
}

impl TryFrom<JsValue> for Frame {
    type Error = JsValue;

    fn try_from(entry: JsValue) -> Result<Self, Self::Error> {
        // 0 - map key
        let id = js_sys::Reflect::get_u32(&entry, 0)?
            .as_string()
            .ok_or(JsValue::from_str("Expected string used as a key"))?;
        let id = InputId(id.into());

        // 1 - map value
        let value = js_sys::Reflect::get_u32(&entry, 1)?;
        let resolution =
            from_js_value::<compositor_api::types::Resolution>(value.get("resolution")?)?.into();
        let format: FrameFormat = from_js_value(value.get("format")?)?;
        let data: js_sys::Uint8ClampedArray = value.get("data")?.into();

        Ok(Self {
            id,
            resolution,
            format,
            data: data.to_vec(),
        })
    }
}

pub fn from_js_value<T: DeserializeOwned>(value: JsValue) -> Result<T, JsValue> {
    serde_wasm_bindgen::from_value(value).map_err(to_js_error)
}

pub fn to_js_error(error: impl std::error::Error + 'static) -> JsValue {
    let error_stack = ErrorStack::new(&error);
    JsValue::from_str(&error_stack.into_string())
}

trait JsValueExt {
    fn get(&self, key: &str) -> Result<JsValue, JsValue>;
}

impl JsValueExt for JsValue {
    fn get(&self, key: &str) -> Result<JsValue, JsValue> {
        js_sys::Reflect::get(self, &JsValue::from_str(key))
    }
}
