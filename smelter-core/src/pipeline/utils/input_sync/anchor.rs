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
    /// The mapping as a single signed offset: what has to be added to an
    /// input pts to get its output pts.
    pub(crate) fn as_offset(&self) -> Timestamp {
        self.output_pts - self.input_pts
    }

    /// Maps a raw timestamp (pts or dts) onto the output timeline.
    /// Timestamps below `input_pts` (initial backlog) map before the start
    /// point, possibly below zero; such content plays late or is dropped by
    /// the consumer.
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
    pub(crate) fn nudge_toward(&mut self, target: TimestampAnchor, step: Timestamp) {
        let distance = target.as_offset() - self.as_offset();
        if distance == Timestamp::ZERO {
            return;
        }
        let before = self.as_offset();
        self.output_pts += Timestamp::clamp(distance, -step, step);
        tracing::trace!(
            before_offset = ?before,
            after_offset = ?self.as_offset(),
            target_offset = ?target.as_offset(),
            ?step,
            "Nudging anchor toward target"
        );
    }
}
