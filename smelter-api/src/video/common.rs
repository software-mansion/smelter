use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum HorizontalAlign {
    Left,
    Right,
    Justified,
    Center,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum VerticalAlign {
    Top,
    Center,
    Bottom,
    Justified,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
pub struct AspectRatio(pub(super) String);
