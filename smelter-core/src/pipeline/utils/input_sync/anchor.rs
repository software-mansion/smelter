use tracing::trace;

use crate::Timestamp;

/// Correspondence between the input and output timelines of a track: content
/// at `input_pts` is presented at `output_pts`, and every other timestamp
/// keeps its distance to the anchor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TimestampAnchor {
    /// Raw pts of the anchor: the estimated live edge when a live track
    /// starts, the first written pts for non-live inputs, or the oldest
    /// buffered pts when a reset has to build a mapping of its own.
    pub input_pts: Timestamp,
    /// Pts relative to the sync point at which content at `input_pts` is
    /// presented.
    pub output_pts: Timestamp,
}

impl TimestampAnchor {
    /// The offset that produces output PTS values when added to input PTS
    pub(crate) fn as_offset(&self) -> Timestamp {
        self.output_pts - self.input_pts
    }

    /// Maps a raw timestamp (pts or dts) onto the output timeline.
    pub(crate) fn to_output_pts(self, pts: Timestamp) -> Timestamp {
        pts + self.as_offset()
    }

    /// Mapping that presents every input pts `offset` later than this one.
    pub(crate) fn offset_by(self, offset: Timestamp) -> Self {
        Self {
            input_pts: self.input_pts,
            output_pts: self.output_pts + offset,
        }
    }

    /// How far apart the two mappings present the same input pts; zero when
    /// both describe the same mapping.
    pub(crate) fn distance_to(&self, other: TimestampAnchor) -> Timestamp {
        (self.as_offset() - other.as_offset()).abs()
    }

    /// Whether `self` presents the same input pts later than `other` does,
    /// i.e. holds content back for longer. `false` when both describe the
    /// same mapping.
    pub(crate) fn presents_later_than(&self, other: TimestampAnchor) -> bool {
        self.as_offset() > other.as_offset()
    }

    /// Moves the mapping at most `step` toward `target`, i.e. toward
    /// presenting the same input pts at the same output pts. A no-op once both
    /// describe the same mapping.
    pub(crate) fn nudge_toward(&mut self, target: TimestampAnchor, max_step: Timestamp) {
        let offset_to_target = target.as_offset() - self.as_offset();
        if offset_to_target == Timestamp::ZERO {
            return;
        }
        let change = Timestamp::clamp(offset_to_target, -max_step, max_step);
        self.output_pts += change;
        trace!(?change, anchor=?self.as_offset(), "Nudging anchor toward target");
    }
}
