use std::time::Duration;

use crate::prelude::*;

/// Resolves the buffer options into concrete `(min, desired, max)` values.
pub(super) fn resolve_buffer_options(
    options: LiveInputBufferOptions,
) -> (Duration, Duration, Duration) {
    // minimal delta between bounds
    const D: Duration = Duration::from_millis(200);
    const DEFAULT: Duration = Duration::from_secs(2);

    // provided values below the floors are raised instead of rejected
    let options = LiveInputBufferOptions {
        min: options.min.map(|min| Duration::max(min, D)),
        desired: options.desired.map(|desired| Duration::max(desired, D * 2)),
        max: options.max.map(|max| Duration::max(max, D * 3)),
    };

    let desired = match options.desired {
        Some(desired) => desired,
        None => match (options.min, options.max) {
            (Some(min), Some(max)) => (min + max) / 2,
            (Some(min), None) => Duration::max(min + D, DEFAULT),
            (None, Some(max)) => Duration::min(max.saturating_sub(D), DEFAULT),
            (None, None) => DEFAULT,
        },
    };
    // default spread adds a chunk size to the upper limit; provided values are
    // respected, with small guardrail margins to stop oscillation
    let max = Duration::max(
        options.max.unwrap_or(Duration::from_secs(1) + desired * 2),
        desired + D,
    );
    let min = Duration::clamp(options.min.unwrap_or(desired / 2), D, desired - D);
    (min, desired, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn ms(ms: u64) -> Duration {
        Duration::from_millis(ms)
    }

    fn resolve(
        min: Option<u64>,
        desired: Option<u64>,
        max: Option<u64>,
    ) -> (Duration, Duration, Duration) {
        resolve_buffer_options(LiveInputBufferOptions {
            desired: desired.map(ms),
            min: min.map(ms),
            max: max.map(ms),
        })
    }

    #[test]
    fn defaults() {
        assert_eq!(resolve(None, None, None), (ms(1000), ms(2000), ms(5000)));
    }

    #[test]
    fn desired_only() {
        assert_eq!(
            resolve(None, Some(4000), None),
            (ms(2000), ms(4000), ms(9000))
        );
        assert_eq!(resolve(None, Some(500), None), (ms(250), ms(500), ms(2000)));
    }

    #[test]
    fn min_only() {
        // desired follows a larger min
        assert_eq!(
            resolve(Some(5000), None, None),
            (ms(5000), ms(5200), ms(11400))
        );
        assert_eq!(
            resolve(Some(500), None, None),
            (ms(500), ms(2000), ms(5000))
        );
    }

    #[test]
    fn max_only() {
        // desired follows a smaller max
        assert_eq!(
            resolve(None, None, Some(1000)),
            (ms(400), ms(800), ms(1000))
        );
        assert_eq!(
            resolve(None, None, Some(20000)),
            (ms(1000), ms(2000), ms(20000))
        );
    }

    #[test]
    fn min_and_max() {
        assert_eq!(
            resolve(Some(1000), None, Some(3000)),
            (ms(1000), ms(2000), ms(3000))
        );
        assert_eq!(
            resolve(Some(4000), None, Some(20000)),
            (ms(4000), ms(12000), ms(20000))
        );
    }

    #[test]
    fn all_provided() {
        assert_eq!(
            resolve(Some(1000), Some(4000), Some(20000)),
            (ms(1000), ms(4000), ms(20000))
        );
        // a tight band is respected as provided
        assert_eq!(
            resolve(Some(3500), Some(4000), Some(4500)),
            (ms(3500), ms(4000), ms(4500))
        );
    }

    #[test]
    fn degenerate_bands() {
        // min == desired == max keeps only the 200ms guardrail margins
        assert_eq!(
            resolve(Some(2000), Some(2000), Some(2000)),
            (ms(1800), ms(2000), ms(2200))
        );
        assert_eq!(
            resolve(Some(500), Some(500), Some(500)),
            (ms(300), ms(500), ms(700))
        );
        // min == max without desired
        assert_eq!(
            resolve(Some(3000), None, Some(3000)),
            (ms(2800), ms(3000), ms(3200))
        );
        // band narrower than the 200ms margin
        assert_eq!(
            resolve(Some(550), Some(600), Some(700)),
            (ms(400), ms(600), ms(800))
        );
    }

    #[test]
    fn values_below_floors_are_raised() {
        // min >= D, desired >= 2D, max >= 3D (reachable only when bypassing
        // the API validation of >= 500ms)
        assert_eq!(
            resolve(Some(0), Some(2), Some(4)),
            (ms(200), ms(400), ms(600))
        );
        assert_eq!(resolve(None, Some(100), None), (ms(200), ms(400), ms(1800)));
        assert_eq!(
            resolve(Some(50), Some(4000), None),
            (ms(200), ms(4000), ms(9000))
        );
        assert_eq!(resolve(None, None, Some(100)), (ms(200), ms(400), ms(600)));
    }
}
