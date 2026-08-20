use std::time::Duration;

/// Correspondence between the input and output timelines of a track: content
/// at `input_pts` is presented at `output_pts`, and every other timestamp
/// keeps its distance to the anchor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TimestampAnchor {
    /// Raw pts of the anchor: the estimated live edge when a live track
    /// starts, the first written pts for non-live inputs, or the oldest
    /// buffered pts when a reset has to build a mapping of its own.
    pub input_pts: Duration,
    /// Pts relative to the sync point at which content at `input_pts` is
    /// presented.
    pub output_pts: Duration,
}

impl TimestampAnchor {
    /// Maps a raw timestamp (pts or dts) onto the output timeline.
    /// Timestamps below `input_pts` (initial backlog) map before the start
    /// point, saturating at zero; such content plays late or is dropped by
    /// the consumer.
    pub(crate) fn to_output_pts(&self, pts: Duration) -> Duration {
        (self.output_pts + pts).saturating_sub(self.input_pts)
    }

    /// Output pts that `self` and `other` assign to the same input pts.
    /// Absolute value does not have any special meaning just the difference
    /// between them.
    fn common_pts(&self, other: TimestampAnchor) -> (Duration, Duration) {
        (
            self.output_pts + other.input_pts,
            other.output_pts + self.input_pts,
        )
    }

    /// How far apart the two mappings present the same input pts; zero when
    /// both describe the same mapping.
    pub(crate) fn distance_to(&self, other: TimestampAnchor) -> Duration {
        let (own, theirs) = self.common_pts(other);
        Duration::abs_diff(own, theirs)
    }

    /// Whether `self` presents the same input pts later than `other` does,
    /// i.e. holds content back for longer. `false` when both describe the
    /// same mapping.
    pub(crate) fn presents_later_than(&self, other: TimestampAnchor) -> bool {
        let (own, theirs) = self.common_pts(other);
        own > theirs
    }

    /// Moves the mapping at most `step` towards `target`, i.e. towards
    /// presenting the same input pts at the same output pts. A no-op once both
    /// describe the same mapping.
    pub(crate) fn nudge_towards(&mut self, target: TimestampAnchor, step: Duration) {
        let (own, wanted) = self.common_pts(target);
        match own > wanted {
            // presenting later than the target, so shift earlier
            true => self.input_pts += Duration::min(step, own - wanted),
            false => self.output_pts += Duration::min(step, wanted - own),
        }
    }
}
