use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Serialize, Deserialize, Clone, Default, JsonSchema, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SideChannel {
    /// Enable side channel for video track.
    pub video: Option<bool>,
    /// Enable side channel for audio track.
    pub audio: Option<bool>,
}
