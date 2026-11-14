use std::sync::Arc;

use smelter_render::scene;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

mod color;
mod common;
mod common_into;
mod component;
mod component_into;
mod transition;

pub use color::*;
pub use common::*;
pub use component::*;
pub use transition::*;

use crate::*;

/// Defines what is rendered for the output. Exactly one of `root` and `script` has to be
/// defined.
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct VideoScene {
    /// Root of a component tree.
    pub root: Option<Component>,
    /// Script that produces the component tree. It runs inside Smelter, so it can react to
    /// input state changes or timers without any delay.
    pub script: Option<SceneScript>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SceneScript {
    /// Source code of a Lua script (Luau dialect).
    ///
    /// A script that is already running for this output is not restarted when an update
    /// request provides exactly the same source, only `props` are replaced.
    pub source: Arc<str>,
    /// Any JSON value, available in the script as `smelter.props()`. Defaults to `null`.
    #[schema(value_type = Option<serde_json::Value>)]
    pub props: Option<serde_json::Value>,
}

const ROOT_OR_SCRIPT_REQUIRED_MSG: &str =
    "Exactly one of \"root\" and \"script\" fields has to be defined.";

impl TryFrom<VideoScene> for scene::Component {
    type Error = TypeError;

    fn try_from(value: VideoScene) -> Result<Self, Self::Error> {
        match (value.root, value.script) {
            (Some(root), None) => root.try_into(),
            (None, Some(_)) => Err(TypeError::new(
                "Scenes defined with \"script\" are not supported here.",
            )),
            _ => Err(TypeError::new(ROOT_OR_SCRIPT_REQUIRED_MSG)),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl TryFrom<VideoScene> for smelter_core::OutputScene {
    type Error = TypeError;

    fn try_from(value: VideoScene) -> Result<Self, Self::Error> {
        match (value.root, value.script) {
            (Some(root), None) => Ok(Self::Static(scene::Scene::ComponentTree(root.try_into()?))),
            (None, Some(script)) => Ok(Self::Script(smelter_core::ScriptSpec {
                source: script.source,
                props: script.props.unwrap_or_default(),
            })),
            _ => Err(TypeError::new(ROOT_OR_SCRIPT_REQUIRED_MSG)),
        }
    }
}
