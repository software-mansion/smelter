use std::time::Duration;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{TypeError, duration_from_ms};

#[derive(Debug, Serialize, Deserialize, Clone, Default, JsonSchema, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SideChannel {
    /// Enable side channel for video track.
    pub video: Option<bool>,
    /// Enable side channel for audio track.
    pub audio: Option<bool>,
    /// Side channel delay in milliseconds. Frames are buffered for this duration ahead of
    /// when the queue consumes them, so the side-channel subscriber receives them early
    /// and has roughly this much time to process before the frame is due.
    pub delay_ms: Option<f64>,
}

impl SideChannel {
    pub(super) fn delay(&self) -> Result<Duration, TypeError> {
        let Some(delay_ms) = self.delay_ms else {
            return Ok(Duration::ZERO);
        };
        duration_from_ms(delay_ms)
            .map_err(|err| TypeError::new(format!("Invalid side_channel.delay_ms. {err}")))
    }
}
