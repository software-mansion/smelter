use std::time::Duration;

/// Correspondence between an input and an output timeline: content at
/// `input_pts` is presented at `output_pts`, and every other timestamp keeps
/// its distance to the anchor.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TimestampAnchor {
    input_pts: Duration,
    output_pts: Duration,
}

impl TimestampAnchor {
    pub fn new(input_pts: Duration, output_pts: Duration) -> Self {
        Self {
            input_pts,
            output_pts,
        }
    }

    /// Maps a raw timestamp (pts or dts) onto the output timeline. Timestamps
    /// before `input_pts` map before the anchor point, saturating at zero.
    pub fn output_pts_of(self, pts: Duration) -> Duration {
        (self.output_pts + pts).saturating_sub(self.input_pts)
    }

    /// How far apart the mappings are; zero when both present content
    /// identically.
    pub fn distance(self, other: Self) -> Duration {
        duration_from_nanos((self.offset_ns() - other.offset_ns()).unsigned_abs())
    }

    /// The mapping moved towards `target` by at most `max_step`. Moving in
    /// steps stretches or squashes the content mapped through the anchor
    /// instead of jumping its timestamps.
    pub fn converged_towards(mut self, target: Self, max_step: Duration) -> Self {
        let distance = target.offset_ns() - self.offset_ns();
        let step = duration_from_nanos(u128::min(max_step.as_nanos(), distance.unsigned_abs()));
        match distance > 0 {
            // present content later
            true => self.output_pts += step,
            // present content earlier
            false => self.input_pts += step,
        }
        self
    }

    /// Signed mapping offset in nanoseconds; anchors with equal offsets define
    /// the same mapping.
    fn offset_ns(self) -> i128 {
        self.output_pts.as_nanos() as i128 - self.input_pts.as_nanos() as i128
    }
}

fn duration_from_nanos(nanos: u128) -> Duration {
    Duration::from_nanos(u64::try_from(nanos).unwrap_or(u64::MAX))
}
