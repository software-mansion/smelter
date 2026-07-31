use std::time::Duration;

/// Correspondence between an input and an output timeline: content at
/// `input_pts` is presented at `output_pts`, and every other timestamp keeps
/// its distance to the anchor.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TimestampAnchor {
    /// Raw pts of the anchor.
    input_pts: Duration,
    /// Pts on the output timeline at which content at `input_pts` is
    /// presented.
    output_pts: Duration,
}

impl TimestampAnchor {
    pub fn new(input_pts: Duration, output_pts: Duration) -> Self {
        Self {
            input_pts,
            output_pts,
        }
    }

    /// Maps a raw timestamp (pts or dts) onto the output timeline.
    /// Timestamps below `input_pts` map before the anchor point, saturating
    /// at zero.
    pub fn to_output_pts(&self, pts: Duration) -> Duration {
        (self.output_pts + pts).saturating_sub(self.input_pts)
    }

    /// Moves this anchor towards `target` by at most `max_step`. Gradual
    /// convergence stretches or squashes the content mapped through the
    /// anchor instead of jumping its timestamps.
    pub fn converge_towards(&mut self, target: &TimestampAnchor, max_step: Duration) {
        let distance = target.offset_ns() - self.offset_ns();
        let step_ns = i128::min(max_step.as_nanos() as i128, distance.abs());
        let step = Duration::from_nanos(step_ns as u64);
        if distance > 0 {
            // present content later
            self.output_pts += step;
        } else {
            // present content earlier
            self.input_pts += step;
        }
    }

    /// Signed mapping offset in nanoseconds; two anchors with equal offsets
    /// define the same mapping.
    fn offset_ns(&self) -> i128 {
        self.output_pts.as_nanos() as i128 - self.input_pts.as_nanos() as i128
    }
}
