use std::time::Duration;

use compositor_render::scene;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::*;

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct Transition {
    /// Duration of a transition in milliseconds.
    pub duration_ms: f64,
    /// (**default=`"linear"`**) Easing function to be used for the transition.
    pub easing_function: Option<EasingFunction>,
    /// (**default=`false`**) On scene update, if there is already a transition in progress,
    /// it will be interrupted and the new transition will start from the current state.
    pub should_interrupt: Option<bool>,
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

impl TryFrom<Transition> for scene::Transition {
    type Error = TypeError;

    fn try_from(transition: Transition) -> Result<Self, Self::Error> {
        let interpolation_kind = match transition.easing_function.unwrap_or(EasingFunction::Linear)
        {
            EasingFunction::Linear => scene::InterpolationKind::Linear,
            EasingFunction::Bounce => scene::InterpolationKind::Bounce,
            EasingFunction::CubicBezier { points } => {
                if points[0] < 0.0 || points[0] > 1.0 {
                    return Err(TypeError::new(
                        "Control point x1 has to be in the range [0, 1].",
                    ));
                }
                if points[2] < 0.0 || points[2] > 1.0 {
                    return Err(TypeError::new(
                        "Control point x2 has to be in the range [0, 1].",
                    ));
                }

                scene::InterpolationKind::CubicBezier {
                    x1: points[0],
                    y1: points[1],
                    x2: points[2],
                    y2: points[3],
                }
            }
        };

        Ok(Self {
            duration: Duration::from_secs_f64(transition.duration_ms / 1000.0),
            interpolation_kind,
            should_interrupt: transition.should_interrupt.unwrap_or(false),
        })
    }
}
