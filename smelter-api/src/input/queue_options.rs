use std::time::Duration;

use crate::*;

pub(super) fn new_queue_options(
    required: Option<bool>,
    offset_ms: Option<f64>,
) -> Result<(bool, Option<Duration>), TypeError> {
    let required = required.unwrap_or(false);
    let offset = offset_ms
        .map(duration_from_ms)
        .transpose()
        .map_err(|err| TypeError::new(format!("Invalid offset_ms. {err}")))?;
    Ok((required, offset))
}
