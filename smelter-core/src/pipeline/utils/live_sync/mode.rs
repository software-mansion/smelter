use super::buffer::BufferingStrategy;
use crate::{
    Timestamp,
    pipeline::utils::input_sync::{TimestampAnchor, TrackKind},
};

/// Anchors the started tracks apply; a track without an anchor holds its chunks back.
/// Corrections move `target`; `current` slews toward it in small steps as chunks are read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Mode {
    Shared(SharedMode),
    Independent(IndependentMode),
}

/// Every started track applies the same anchor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SharedMode {
    /// Slewed by every started track; its last released pts also keeps the tracks in pts order.
    pub anchor: SlewingAnchor,
    pub audio_started: bool,
    pub video_started: bool,
}

/// The tracks are on unrelated timelines. The leader is started, applies its anchor and is the
/// only track that slews it; the secondary track's anchor is derived from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct IndependentMode {
    /// Audio when started, otherwise video.
    pub leader_kind: TrackKind,
    pub leader_anchor: SlewingAnchor,
    /// The secondary track's anchor is the leader's applied after this offset, so it follows the
    /// leader's slew and moves on its own only when the offset does. `None` while the secondary
    /// track has not started, so it has no anchor yet.
    pub secondary_track_offset: Option<SecondaryTrackOffset>,
}

/// Anchor that corrections move by setting `target`; `current` slews toward it at the rate the
/// buffering strategy allows as chunks are read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SlewingAnchor {
    /// Applied to every chunk read right now.
    pub current: TimestampAnchor,
    /// What the corrections aim for.
    pub target: TimestampAnchor,
    /// Largest input pts released so far with this anchor; sizes the slew steps.
    pub last_released_pts: Option<Timestamp>,
}

impl SlewingAnchor {
    /// Anchor that is not slewing.
    pub fn new(anchor: TimestampAnchor) -> Self {
        Self {
            current: anchor,
            target: anchor,
            last_released_pts: None,
        }
    }

    fn nudge_toward_target(&mut self, pts: Timestamp, strategy: BufferingStrategy) {
        let last_pts = self.last_released_pts.unwrap_or(pts);
        self.last_released_pts = Some(Timestamp::max(last_pts, pts));

        let max_shift = strategy.max_shift(
            self.current,
            self.target,
            Timestamp::max(Timestamp::ZERO, pts - last_pts),
        );
        self.current.nudge_toward(self.target, max_shift);
    }
}

/// Offset added to a secondary track pts to get the leader pts presented at the same time.
/// Corrections move `target`; `current` slews toward it as secondary chunks are read, faster
/// than the leader's anchor since only the alignment moves, not the buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SecondaryTrackOffset {
    pub current: Timestamp,
    pub target: Timestamp,
    /// Largest input pts of the secondary track released so far; sizes the slew steps.
    pub last_released_pts: Option<Timestamp>,
}

impl SecondaryTrackOffset {
    /// Offset that is not slewing.
    pub fn new(offset: Timestamp) -> Self {
        Self {
            current: offset,
            target: offset,
            last_released_pts: None,
        }
    }
}

impl SharedMode {
    /// Shared anchor that is not slewing, applied by the track of `started` only.
    pub fn from_anchor(anchor: TimestampAnchor) -> Self {
        Self {
            anchor: SlewingAnchor::new(anchor),
            audio_started: false,
            video_started: false,
        }
    }

    pub fn is_started(&self, kind: TrackKind) -> bool {
        match kind {
            TrackKind::Audio => self.audio_started,
            TrackKind::Video => self.video_started,
        }
    }

    /// Same anchor, also applied by the track of `kind`.
    pub fn with_started(&self, kind: TrackKind) -> Self {
        match kind {
            TrackKind::Audio => Self {
                audio_started: true,
                ..*self
            },
            TrackKind::Video => Self {
                video_started: true,
                ..*self
            },
        }
    }
}

impl IndependentMode {
    /// Anchor a track of `kind` applies right now; the secondary track's is the leader's applied
    /// after the offset.
    pub fn anchor(&self, kind: TrackKind) -> Option<TimestampAnchor> {
        let leader = self.leader_anchor.current;
        match self.leader_kind == kind {
            true => Some(leader),
            false => Some(leader.offset_by(self.secondary_track_offset?.current)),
        }
    }

    /// Anchor the track of `kind` is slewing toward; the secondary track's is the leader's
    /// applied after the offset.
    pub fn target(&self, kind: TrackKind) -> Option<TimestampAnchor> {
        let leader = self.leader_anchor.target;
        match self.leader_kind == kind {
            true => Some(leader),
            false => Some(leader.offset_by(self.secondary_track_offset?.target)),
        }
    }
}

impl Mode {
    /// Anchor a track of `kind` applies right now; `None` while the track has not started.
    pub fn anchor(&self, kind: TrackKind) -> Option<TimestampAnchor> {
        match self {
            Mode::Shared(shared) => shared.is_started(kind).then_some(shared.anchor.current),
            Mode::Independent(independent) => independent.anchor(kind),
        }
    }

    /// Anchor the track of `kind` is slewing toward; `None` while the track has not started.
    pub fn target(&self, kind: TrackKind) -> Option<TimestampAnchor> {
        match self {
            Mode::Shared(shared) => shared.is_started(kind).then_some(shared.anchor.target),
            Mode::Independent(independent) => independent.target(kind),
        }
    }

    pub fn is_started(&self, kind: TrackKind) -> bool {
        self.anchor(kind).is_some()
    }

    /// Slews the anchor for a release of `kind` at `pts`. A secondary track's release slews
    /// its offset instead.
    pub fn nudge_anchor_toward_target(
        &mut self,
        kind: TrackKind,
        pts: Timestamp,
        strategy: BufferingStrategy,
    ) {
        match self {
            Mode::Shared(shared) => shared.anchor.nudge_toward_target(pts, strategy),
            Mode::Independent(independent) if independent.leader_kind == kind => {
                independent.leader_anchor.nudge_toward_target(pts, strategy)
            }
            Mode::Independent(independent) => {
                // Fraction of the released input step the offset moves by while slewing.
                const SLEW_RATE: f64 = 0.25;
                let Some(offset) = &mut independent.secondary_track_offset else {
                    return;
                };
                let last_pts = offset.last_released_pts.unwrap_or(pts);
                offset.last_released_pts = Some(Timestamp::max(last_pts, pts));

                let max_step = Timestamp::max(Timestamp::ZERO, pts - last_pts).mul_f64(SLEW_RATE);
                let distance = offset.target - offset.current;
                offset.current += Timestamp::clamp(distance, -max_step, max_step);
            }
        }
    }

    /// Mode after the track of `kind` stops applying its anchor. A shared anchor stays for the
    /// other track; a secondary track loses its offset; a leader hands the anchor the secondary
    /// track has been applying over to it, or leaves nothing if it had none.
    pub fn reset_track(mut self, kind: TrackKind) -> Option<Mode> {
        match &mut self {
            Mode::Shared(shared) => match kind {
                TrackKind::Audio => shared.audio_started = false,
                TrackKind::Video => shared.video_started = false,
            },
            Mode::Independent(independent) if independent.leader_kind != kind => {
                independent.secondary_track_offset = None;
            }
            Mode::Independent(independent) => {
                let secondary = match kind {
                    TrackKind::Audio => TrackKind::Video,
                    TrackKind::Video => TrackKind::Audio,
                };
                *independent = IndependentMode {
                    leader_kind: secondary,
                    leader_anchor: SlewingAnchor {
                        current: independent.anchor(secondary)?,
                        target: independent.target(secondary)?,
                        last_released_pts: None,
                    },
                    secondary_track_offset: None,
                };
            }
        }
        Some(self)
    }

    /// What `new` changes relative to `old`; `None` when nothing. Corrections only move the
    /// target, so that is the anchor move reported.
    pub fn diff(old: Option<Mode>, new: Option<Mode>) -> Option<ModeChange> {
        let target_change = |old: TimestampAnchor, new: TimestampAnchor| {
            (old != new).then(|| new.as_offset() - old.as_offset())
        };
        match (old, new) {
            (None, None) => None,
            (Some(Mode::Shared(old)), Some(Mode::Shared(new)))
                if (old.audio_started, old.video_started)
                    == (new.audio_started, new.video_started) =>
            {
                target_change(old.anchor.target, new.anchor.target)
                    .map(|target_change| ModeChange::Shared { target_change })
            }
            (Some(Mode::Independent(old)), Some(Mode::Independent(new)))
                if old.leader_kind == new.leader_kind =>
            {
                let offset_change = match (old.secondary_track_offset, new.secondary_track_offset) {
                    (Some(old), Some(new)) => {
                        (old.target != new.target).then(|| new.target - old.target)
                    }
                    (None, None) => None,
                    _ => return Some(ModeChange::Mode),
                };
                match (
                    target_change(old.leader_anchor.target, new.leader_anchor.target),
                    offset_change,
                ) {
                    (None, None) => None,
                    (target_change, offset_change) => Some(ModeChange::Independent {
                        target_change,
                        offset_change,
                    }),
                }
            }
            _ => Some(ModeChange::Mode),
        }
    }
}

/// Difference between two modes, see [`Mode::diff`]. Anchor moves are positive when the same
/// input pts is now presented later (buffer grew).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ModeChange {
    /// Different mode, a different leader, or a track started or stopped.
    Mode,
    /// Same mode, the shared target moved.
    Shared { target_change: Timestamp },
    /// Same mode; the leader's target and/or the secondary track offset moved (`None` for one
    /// that did not). A positive offset change presents the secondary track later relative to
    /// the leader.
    Independent {
        target_change: Option<Timestamp>,
        offset_change: Option<Timestamp>,
    },
}
