use std::time::Duration;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::common_core::prelude as core;
use crate::*;

/// Buffer a live input keeps between the live edge of the stream and playback.
/// A larger buffer adds latency, but tolerates more delivery jitter and
/// network stalls.
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
#[serde(untagged)]
pub enum InputBuffer {
    /// Desired buffer in milliseconds; the allowed range is derived from it.
    DesiredMs(f64),
    Options(InputBufferOptions),
}

/// Values that are not provided are derived from the provided ones and the
/// protocol defaults.
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct InputBufferOptions {
    /// Buffer the input aims to keep, in milliseconds. At the start it should
    /// buffer at least that much media before producing first chunk.
    pub desired_ms: Option<f64>,
    /// Lower range of what is considered stable state. If buffer is smaller than this
    /// value then media will be slightly "stretched" so the buffer converges on desired value.
    pub min_ms: Option<f64>,
    /// Upper range of what is considered stable state. If buffer is larger than this
    /// value then media will be slightly "squashed" so the buffer converges on desired value.
    pub max_ms: Option<f64>,
}

impl TryFrom<InputBuffer> for core::LiveInputBufferOptions {
    type Error = TypeError;

    fn try_from(value: InputBuffer) -> Result<Self, Self::Error> {
        let options = match value {
            InputBuffer::DesiredMs(desired_ms) => InputBufferOptions {
                desired_ms: Some(desired_ms),
                min_ms: None,
                max_ms: None,
            },
            InputBuffer::Options(options) => options,
        };
        let desired = parse_buffer_ms("buffer.desired_ms", options.desired_ms)?;
        let min = parse_buffer_ms("buffer.min_ms", options.min_ms)?;
        let max = parse_buffer_ms("buffer.max_ms", options.max_ms)?;

        if let (Some(min), Some(desired)) = (min, desired)
            && min > desired
        {
            return Err(TypeError::new(
                "buffer.min_ms cannot be greater than buffer.desired_ms.",
            ));
        }
        if let (Some(desired), Some(max)) = (desired, max)
            && desired > max
        {
            return Err(TypeError::new(
                "buffer.desired_ms cannot be greater than buffer.max_ms.",
            ));
        }
        if let (Some(min), Some(max)) = (min, max)
            && min > max
        {
            return Err(TypeError::new(
                "buffer.min_ms cannot be greater than buffer.max_ms.",
            ));
        }

        Ok(Self { desired, min, max })
    }
}

fn parse_buffer_ms(name: &str, value: Option<f64>) -> Result<Option<Duration>, TypeError> {
    let Some(ms) = value else {
        return Ok(None);
    };
    match Duration::try_from_secs_f64(ms / 1000.0) {
        Ok(duration) => Ok(Some(duration)),
        Err(err) => Err(TypeError::new(format!("Invalid {name}. {err}"))),
    }
}
