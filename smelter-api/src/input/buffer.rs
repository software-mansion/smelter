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
    /// Buffer the input starts with, in milliseconds, measured for the chunks that arrive the
    /// fastest. It is also the value the buffer is brought back to when `min_ms` or `max_ms` is
    /// not met.
    pub desired_ms: Option<f64>,
    /// Smallest buffer, in milliseconds, that chunks arriving late on a regular basis (network
    /// jitter, stalls that keep coming back) should be left with. When they have less, the
    /// buffer grows.
    pub min_ms: Option<f64>,
    /// Largest buffer, in milliseconds, measured for the chunks that arrive the fastest. When
    /// the buffer is larger, it shrinks back to `desired_ms`. The buffer still grows past this
    /// value when it is needed to keep `min_ms` for the chunks that are late on a regular basis.
    pub max_ms: Option<f64>,
}

/// Buffer a MoQ input keeps between the live edge of the stream and playback.
/// A larger buffer adds latency, but tolerates more delivery jitter and
/// network stalls.
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
#[serde(untagged)]
pub enum MoqInputBuffer {
    /// Desired buffer in milliseconds; the allowed range is derived from it.
    DesiredMs(f64),
    Options(MoqInputBufferOptions),
}

/// Values that are not provided are derived from the provided ones and the
/// protocol defaults.
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MoqInputBufferOptions {
    /// Buffer the input starts with, in milliseconds, measured for the chunks that arrive the
    /// fastest. It is also the value the buffer is brought back to when `min_ms` or `max_ms` is
    /// not met.
    pub desired_ms: Option<f64>,
    /// Smallest buffer, in milliseconds, that chunks arriving late on a regular basis (network
    /// jitter, stalls that keep coming back) should be left with. When they have less, the
    /// buffer grows.
    pub min_ms: Option<f64>,
    /// Largest buffer, in milliseconds, measured for the chunks that arrive the fastest. When
    /// the buffer is larger, it shrinks back to `desired_ms`.
    pub max_ms: Option<f64>,
    /// What happens when the stream can't be on time with a buffer of `max_ms`.
    ///
    /// Defaults to `enforce`.
    pub max_policy: Option<InputBufferMaxPolicy>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum InputBufferMaxPolicy {
    /// The buffer grows past `max_ms` as far as it is needed to keep `min_ms` for the chunks
    /// that are late on a regular basis. Nothing is lost.
    Extend,
    /// Playback is never more than `max_ms` behind the live edge of the stream, measured on
    /// audio when the input has it. Content that arrives too late is not played on time: late
    /// audio is dropped and late video is shown late.
    Enforce,
}

impl From<InputBufferMaxPolicy> for core::LiveInputBufferMaxPolicy {
    fn from(value: InputBufferMaxPolicy) -> Self {
        match value {
            InputBufferMaxPolicy::Extend => Self::Extend,
            InputBufferMaxPolicy::Enforce => Self::Enforce,
        }
    }
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
        let desired = options
            .desired_ms
            .map(duration_from_ms)
            .transpose()
            .map_err(|err| TypeError::new(format!("Invalid buffer.desired_ms. {err}")))?;
        let min = options
            .min_ms
            .map(duration_from_ms)
            .transpose()
            .map_err(|err| TypeError::new(format!("Invalid buffer.min_ms. {err}")))?;
        let max = options
            .max_ms
            .map(duration_from_ms)
            .transpose()
            .map_err(|err| TypeError::new(format!("Invalid buffer.max_ms. {err}")))?;

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
