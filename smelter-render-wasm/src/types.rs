use std::time::Duration;

use smelter_render::InputId;
use tracing::error;
use wasm_bindgen::prelude::*;

use crate::wasm::{self, InputFrameKind};

#[wasm_bindgen(typescript_custom_section)]
const TS_DEFINITIONS: &'static str = r#"
export interface InputFrame {
  inputId: string;
  frame: VideoFrame | HTMLVideoElement;
  ptsMs: number;
}

export interface InputFrameSet {
  frames: Array<InputFrame>;
  ptsMs: number;
}

export interface Resolution {
  width: number;
  height: number;
}

export interface OutputFrame {
  outputId: string;
  resolution: Resolution;
  data: Uint8ClampedArray;
}

export interface OutputFrameSet {
  frames: Array<OutputFrame>;
  ptsMs: number;
}
"#;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "InputFrameSet")]
    pub type InputFrameSet;

    #[wasm_bindgen(typescript_type = "OutputFrameSet")]
    pub type OutputFrameSet;
}

impl TryFrom<InputFrameSet> for wasm::InputFrameSet {
    type Error = JsValue;

    fn try_from(value: InputFrameSet) -> Result<Self, Self::Error> {
        let pts_ms = js_sys::Reflect::get(&value.obj, &"ptsMs".into())?
            .as_f64()
            .ok_or_else(|| JsValue::from_str("Expected ptsMs to be a number."))?;

        let frames =
            js_sys::Reflect::get(&value.obj, &"frames".into())?.dyn_into::<js_sys::Array>()?;

        Ok(Self {
            pts: Duration::from_secs_f64(pts_ms / 1000.0),
            frames: frames
                .into_iter()
                .map(wasm::InputFrame::try_from)
                .collect::<Result<Vec<_>, JsValue>>()?,
        })
    }
}

impl TryFrom<JsValue> for wasm::InputFrame {
    type Error = JsValue;

    fn try_from(value: JsValue) -> Result<Self, Self::Error> {
        let id = InputId(
            js_sys::Reflect::get(&value, &"inputId".into())?
                .as_string()
                .ok_or(JsValue::from_str("Expected string used as a inputId"))?
                .into(),
        );
        let pts_ms = js_sys::Reflect::get(&value, &"ptsMs".into())?
            .as_f64()
            .ok_or(JsValue::from_str("Expected number used as a ptsMs"))?;

        let frame = js_sys::Reflect::get(&value, &"frame".into())?;
        let frame = if frame.is_instance_of::<web_sys::VideoFrame>() {
            InputFrameKind::VideoFrame(frame.dyn_into()?)
        } else if frame.is_instance_of::<web_sys::HtmlVideoElement>() {
            InputFrameKind::HtmlVideoElement(frame.dyn_into()?)
        } else if frame.is_undefined() {
            return Err(JsValue::from_str("missing frame"));
        } else {
            error!("Error {:?}", frame);
            return Err(JsValue::from_str(
                "Unknown frame, expected VideoFrame or HtmlVideoElement",
            ));
        };

        Ok(Self {
            id,
            pts: Duration::from_secs_f64(pts_ms / 1000.0),
            frame,
        })
    }
}

impl From<wasm::OutputFrameSet> for OutputFrameSet {
    fn from(value: wasm::OutputFrameSet) -> Self {
        let result = js_sys::Object::new();
        result.set("ptsMs", value.pts.as_millis());
        result.set(
            "frames",
            value
                .frames
                .into_iter()
                .map(|frame| frame.into())
                .collect::<Vec<JsValue>>(),
        );
        OutputFrameSet { obj: result.into() }
    }
}

impl From<wasm::OutputFrame> for JsValue {
    fn from(value: wasm::OutputFrame) -> Self {
        let resolution = js_sys::Object::new();
        resolution.set("width", value.resolution.width);
        resolution.set("height", value.resolution.height);

        let result = js_sys::Object::new();
        result.set("outputId", value.output_id.0.as_ref());
        result.set("resolution", resolution);
        result.set("data", value.data);

        result.into()
    }
}

pub trait ObjectExt {
    fn set<T: Into<JsValue>>(&self, key: &str, value: T);
}

impl ObjectExt for js_sys::Object {
    fn set<T: Into<JsValue>>(&self, key: &str, value: T) {
        js_sys::Reflect::set(self, &JsValue::from_str(key), &value.into()).unwrap();
    }
}
