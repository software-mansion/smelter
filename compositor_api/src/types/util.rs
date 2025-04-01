use std::{error::Error, fmt::Display};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct Resolution {
    /// Width in pixels.
    pub width: usize,
    /// Height in pixels.
    pub height: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct Transition {
    /// Duration of a transition in milliseconds.
    pub duration_ms: f64,
    /// (**default=`"linear"`**) Easing function to be used for the transition.
    pub easing_function: Option<EasingFunction>,
    /// (**default=`false`**) On scene update, if there is already a transition in progress,
    /// it will be canceled and the new transition will start from the current state.
    pub reset_on_update: Option<bool>,
}

/// Easing functions are used to interpolate between two values over time.
///
/// Custom easing functions can be implemented with cubic Bézier.
/// The control points are defined with `points` field by providing four numerical values: `x1`, `y1`, `x2` and `y2`. The `x1` and `x2` values have to be in the range `[0; 1]`. The cubic Bézier result is clamped to the range `[0; 1]`.
/// You can find example control point configurations [here](https://easings.net/).
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(tag = "function_name", rename_all = "snake_case")]
pub enum EasingFunction {
    Linear,
    Bounce,
    CubicBezier { points: [f64; 4] },
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HorizontalAlign {
    Left,
    Right,
    Justified,
    Center,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VerticalAlign {
    Top,
    Center,
    Bottom,
    Justified,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct AspectRatio(pub(super) String);

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct Degree(pub f64);

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(untagged)]
pub enum Framerate {
    String(String),
    U32(u32),
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, PartialEq)]
pub struct RGBAColor(pub String);

#[derive(Debug, PartialEq)]
pub struct TypeError(String);

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, PartialEq, Eq)]
#[serde(untagged)]
pub enum PortOrPortRange {
    String(String),
    U16(u16),
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TransportProtocol {
    /// UDP protocol.
    Udp,
    /// TCP protocol where Smelter is the server side of the connection.
    TcpServer,
}

impl Display for TypeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl Error for TypeError {}

impl TypeError {
    pub fn new<S: Into<String>>(msg: S) -> Self {
        Self(msg.into())
    }
}
