use std::{fmt, time::Duration};

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
    /// The secondary track's anchor is the leader's current anchor applied after this offset.
    /// The offset makes up for what the leader still has to slew, so the secondary track stays on
    /// its own final position while the leader slews. `None` while the secondary track has not
    /// started, so it has no anchor yet.
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

/// Offset added to a secondary track pts to get the leader pts presented at the same time once
/// the leader reaches its target. Corrections move `target`; `current` slews toward it as
/// secondary chunks are read, faster than the leader's anchor since the secondary track is always
/// video, where a slew costs dropped or repeated frames and no pitch change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SecondaryTrackOffset {
    pub current: Timestamp,
    /// Includes what the leader still has to slew, so reaching it puts the secondary track on
    /// its final anchor even while the leader is still on its way.
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

    /// Anchor the track of `kind` is slewing toward. The secondary track's is the leader's
    /// current anchor applied after the target offset, since that offset already accounts for
    /// what the leader still has to slew.
    pub fn target(&self, kind: TrackKind) -> Option<TimestampAnchor> {
        match self.leader_kind == kind {
            true => Some(self.leader_anchor.target),
            false => {
                let offset = self.secondary_track_offset?.target;
                Some(self.leader_anchor.current.offset_by(offset))
            }
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

    /// What `new` changes relative to `old`; `None` only when they are equal.
    pub fn diff(old: Option<Mode>, new: Option<Mode>) -> Option<ModeChange> {
        if old == new {
            return None;
        }
        let anchor_change = |old: TimestampAnchor, new: TimestampAnchor| {
            let change = new.as_offset() - old.as_offset();
            (change != Timestamp::ZERO).then_some(change)
        };
        let offset_change = |old: Timestamp, new: Timestamp| (old != new).then(|| new - old);

        Some(match (old, new) {
            (Some(Mode::Shared(old)), Some(Mode::Shared(new)))
                if (old.audio_started, old.video_started)
                    == (new.audio_started, new.video_started) =>
            {
                ModeChange::Shared {
                    target_change: anchor_change(old.anchor.target, new.anchor.target),
                    current_change: anchor_change(old.anchor.current, new.anchor.current),
                }
            }
            (Some(Mode::Independent(old_mode)), Some(Mode::Independent(new_mode)))
                if old_mode.leader_kind == new_mode.leader_kind =>
            {
                let (offset_target_change, offset_current_change) = match (
                    old_mode.secondary_track_offset,
                    new_mode.secondary_track_offset,
                ) {
                    (Some(old), Some(new)) => (
                        offset_change(old.target, new.target),
                        offset_change(old.current, new.current),
                    ),
                    (None, None) => (None, None),
                    _ => return Some(ModeChange::Mode(new)),
                };
                let (old, new) = (old_mode.leader_anchor, new_mode.leader_anchor);
                ModeChange::Independent {
                    target_change: anchor_change(old.target, new.target),
                    current_change: anchor_change(old.current, new.current),
                    offset_target_change,
                    offset_current_change,
                }
            }
            _ => ModeChange::Mode(new),
        })
    }
}

/// Difference between two modes, see [`Mode::diff`]. Anchor moves are positive when the same
/// input pts is now presented later (buffer grew); a positive offset change presents the
/// secondary track later relative to the leader. Fields are `None` for what did not move and
/// are left out of the debug output.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ModeChange {
    /// Different mode, a different leader, or a track started or stopped; holds the new mode.
    Mode(Option<Mode>),
    /// Same mode, the shared anchor moved.
    Shared {
        target_change: Option<Timestamp>,
        current_change: Option<Timestamp>,
    },
    /// Same mode; the leader's anchor and/or the secondary track offset moved.
    Independent {
        target_change: Option<Timestamp>,
        current_change: Option<Timestamp>,
        offset_target_change: Option<Timestamp>,
        offset_current_change: Option<Timestamp>,
    },
}

impl ModeChange {
    /// Only the secondary offset target moved, and by too little to be worth a debug line; it
    /// follows the leader's slew in steps this small.
    pub fn is_minor(&self) -> bool {
        const MINOR_CHANGE: Duration = Duration::from_millis(10);
        matches!(
            *self,
            ModeChange::Independent {
                target_change: None,
                current_change: None,
                offset_current_change: None,
                offset_target_change: Some(change),
            } if change.abs() < Timestamp::from(MINOR_CHANGE)
        )
    }
}

impl fmt::Debug for ModeChange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (name, fields) = match *self {
            ModeChange::Mode(mode) => return f.debug_tuple("Mode").field(&mode).finish(),
            ModeChange::Shared {
                target_change,
                current_change,
            } => (
                "Shared",
                vec![
                    ("target_change", target_change),
                    ("current_change", current_change),
                ],
            ),
            ModeChange::Independent {
                target_change,
                current_change,
                offset_target_change,
                offset_current_change,
            } => (
                "Independent",
                vec![
                    ("target_change", target_change),
                    ("current_change", current_change),
                    ("offset_target_change", offset_target_change),
                    ("offset_current_change", offset_current_change),
                ],
            ),
        };
        let mut output = f.debug_struct(name);
        for (field, change) in fields {
            if let Some(change) = change {
                output.field(field, &change);
            }
        }
        output.finish()
    }
}
