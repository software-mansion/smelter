use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema, PartialEq)]
pub struct ComponentId(Arc<str>);

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema, PartialEq)]
pub struct RendererId(Arc<str>);

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema, PartialEq)]
pub struct OutputId(Arc<str>);

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema, PartialEq)]
pub struct InputId(Arc<str>);

impl From<&str> for ComponentId {
    fn from(s: &str) -> Self {
        Self(s.into())
    }
}

impl From<&str> for RendererId {
    fn from(s: &str) -> Self {
        Self(s.into())
    }
}

impl From<&str> for InputId {
    fn from(s: &str) -> Self {
        Self(s.into())
    }
}

impl From<&str> for OutputId {
    fn from(s: &str) -> Self {
        Self(s.into())
    }
}

impl From<ComponentId> for smelter_render::scene::ComponentId {
    fn from(id: ComponentId) -> Self {
        Self(id.0)
    }
}

impl From<RendererId> for smelter_render::RendererId {
    fn from(id: RendererId) -> Self {
        Self(id.0)
    }
}

impl From<OutputId> for smelter_render::OutputId {
    fn from(id: OutputId) -> Self {
        id.0.into()
    }
}

impl From<InputId> for smelter_render::InputId {
    fn from(id: InputId) -> Self {
        id.0.into()
    }
}
