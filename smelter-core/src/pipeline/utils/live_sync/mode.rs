use crate::{
    Timestamp,
    pipeline::utils::input_sync::{TimestampAnchor, TrackKind},
};

/// Which anchors the started tracks apply. Tracks on the same timeline share one; tracks on
/// unrelated timelines run their own, video aligned to audio's live edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Mode {
    /// No track started yet.
    Undecided,
    /// Every started track applies this anchor, corrected against the shared estimator.
    Shared(Anchor),
    /// Each started track applies its own anchor (`None` while waiting), corrected against its
    /// own estimator.
    Independent {
        audio: Option<Anchor>,
        video: Option<Anchor>,
    },
}

/// Corrections move `target`; `current` slews towards it in small steps as
/// chunks are read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Anchor {
    /// Mapping applied to every chunk read right now.
    pub current: TimestampAnchor,
    /// Mapping the corrections aim for.
    pub target: TimestampAnchor,
    /// Largest input pts released so far with this anchor; sizes the slew steps and keeps tracks
    /// sharing the anchor in pts order.
    pub last_released_pts: Option<Timestamp>,
}

impl Anchor {
    pub fn new(anchor: TimestampAnchor) -> Self {
        Self {
            current: anchor,
            target: anchor,
            last_released_pts: None,
        }
    }
}

impl Mode {
    /// Anchor a started track of `kind` applies right now.
    pub fn anchor(&self, kind: TrackKind) -> Option<Anchor> {
        match (self, kind) {
            (Mode::Undecided, _) => None,
            (Mode::Shared(anchor), _) => Some(*anchor),
            (Mode::Independent { audio, .. }, TrackKind::Audio) => *audio,
            (Mode::Independent { video, .. }, TrackKind::Video) => *video,
        }
    }

    pub fn anchor_mut(&mut self, kind: TrackKind) -> Option<&mut Anchor> {
        match (self, kind) {
            (Mode::Undecided, _) => None,
            (Mode::Shared(anchor), _) => Some(anchor),
            (Mode::Independent { audio, .. }, TrackKind::Audio) => audio.as_mut(),
            (Mode::Independent { video, .. }, TrackKind::Video) => video.as_mut(),
        }
    }

    /// What `other` changes relative to `self`; `None` when nothing.
    pub fn diff(&self, other: &Mode) -> Option<ModeChange> {
        match (self, other) {
            (Mode::Undecided, Mode::Undecided) => None,
            (Mode::Shared(old), Mode::Shared(new)) => old.diff(new).map(ModeChange::Shared),
            (
                Mode::Independent {
                    audio: old_audio,
                    video: old_video,
                },
                Mode::Independent {
                    audio: new_audio,
                    video: new_video,
                },
            ) => {
                let audio = match (old_audio, new_audio) {
                    (Some(old), Some(new)) => old.diff(new),
                    (None, None) => None,
                    _ => return Some(ModeChange::Mode),
                };
                let video = match (old_video, new_video) {
                    (Some(old), Some(new)) => old.diff(new),
                    (None, None) => None,
                    _ => return Some(ModeChange::Mode),
                };
                match (audio, video) {
                    (None, None) => None,
                    _ => Some(ModeChange::Independent { audio, video }),
                }
            }
            _ => Some(ModeChange::Mode),
        }
    }

    /// Forgets the own anchor of `kind`; no-op unless independent.
    pub fn forget_independent_anchor(&mut self, kind: TrackKind) {
        match (self, kind) {
            (Mode::Independent { audio, .. }, TrackKind::Audio) => *audio = None,
            (Mode::Independent { video, .. }, TrackKind::Video) => *video = None,
            _ => {}
        }
    }
}

/// Difference between two modes, see [`Mode::diff`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ModeChange {
    /// Different mode, or an own anchor appeared or disappeared.
    Mode,
    /// Same mode, the shared anchor moved.
    Shared(AnchorChange),
    /// Same mode, own anchors moved (`None` for one that did not).
    Independent {
        audio: Option<AnchorChange>,
        video: Option<AnchorChange>,
    },
}

/// How far the mappings of an anchor moved: positive when the same input pts is now presented
/// later (buffer grew).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct AnchorChange {
    pub current: Timestamp,
    pub target: Timestamp,
}

impl Anchor {
    /// What `other` changes relative to `self`; `None` when nothing moved.
    fn diff(&self, other: &Anchor) -> Option<AnchorChange> {
        let change = AnchorChange {
            current: other.current.as_offset() - self.current.as_offset(),
            target: other.target.as_offset() - self.target.as_offset(),
        };
        match change.current != Timestamp::ZERO || change.target != Timestamp::ZERO {
            true => Some(change),
            false => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TrackState {
    /// Written chunks are buffered and not released yet.
    Waiting,
    /// Chunks are released with the anchor `Mode` holds for the track.
    Started,
}
