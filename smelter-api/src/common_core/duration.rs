use std::time::Duration;

use crate::TypeError;

pub(crate) fn try_duration_from_f64(
    keyframe_interval: &Option<f64>,
) -> Result<Duration, TypeError> {
    const DEFAULT_KEYFRAME_INTERVAL: Duration = Duration::from_millis(5000);

    match keyframe_interval {
        Some(ki) if *ki < 0.0 => Err(TypeError::new("Time cannot be negative.")),
        Some(ki) => {
            let ki = ki.round() as u64;
            Ok(Duration::from_millis(ki))
        }
        None => Ok(DEFAULT_KEYFRAME_INTERVAL),
    }
}
