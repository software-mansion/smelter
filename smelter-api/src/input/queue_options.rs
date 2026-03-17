use std::time::Duration;

use crate::*;

pub(super) fn new_queue_options(
    required: Option<bool>,
    offset_ms: Option<f64>,
) -> Result<smelter_core::QueueInputOptions, TypeError> {
    Ok(smelter_core::QueueInputOptions {
        required: required.unwrap_or(false),
        offset: offset_ms
            .map(|offset_ms| Duration::try_from_secs_f64(offset_ms / 1000.0))
            .transpose()
            .map_err(|err| TypeError::new(format!("Invalid duration. {err}")))?,
    })
}
