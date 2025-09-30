use smelter_render::scene;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VideoScene {
    pub root: Component,
}

impl TryFrom<VideoScene> for scene::Component {
    type Error = TypeError;

    fn try_from(value: VideoScene) -> Result<Self, Self::Error> {
        value.root.try_into()
    }
}
