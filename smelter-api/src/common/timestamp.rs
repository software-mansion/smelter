use std::time::Duration;

use crate::TypeError;

/// Converts time in milliseconds from a request into a `Duration`. Callers should wrap the error
/// with the name of the field.
pub fn duration_from_ms(ms: f64) -> Result<Duration, TypeError> {
    // Arbitrary upper bound, far from overflowing `Duration`, `Timestamp` and values derived
    // from them.
    const MAX_MS: f64 = 365.0 * 24.0 * 60.0 * 60.0 * 1000.0;

    if ms.is_nan() {
        return Err(TypeError::new("Value is not a number."));
    }
    if ms < 0.0 {
        return Err(TypeError::new("Value cannot be negative."));
    }
    if ms > MAX_MS {
        return Err(TypeError::new("Value is too large."));
    }
    Ok(Duration::from_secs_f64(ms / 1000.0))
}
