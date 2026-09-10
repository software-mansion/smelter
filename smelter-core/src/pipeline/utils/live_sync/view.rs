use std::time::Duration;

use crate::{
    Timestamp,
    pipeline::utils::input_sync::TimestampAnchor,
    utils::{
        input_sync::TrackKind,
        live_sync::{LiveSyncOptions, edge_estimator::EdgeEstimate},
    },
};

/// Which anchors the started tracks apply. Tracks on the same timeline
/// share one; tracks on unrelated timelines run their own, video aligned to
/// audio's live edge.
#[derive(Debug, Clone, Copy)]
pub(super) enum Mode {
    /// No track started yet.
    Undecided,
    /// Every started track applies this anchor, corrected against the
    /// shared estimator.
    Shared(Anchor),
    /// Each started track applies its own anchor (`None` while waiting),
    /// corrected against its own estimator.
    Independent {
        audio: Option<Anchor>,
        video: Option<Anchor>,
    },
}

/// Corrections move `target`; `current` slews towards it in small steps as
/// chunks are read.
#[derive(Debug, Clone, Copy)]
pub(super) struct Anchor {
    /// Mapping applied to every chunk read right now.
    pub current: TimestampAnchor,
    /// Mapping the corrections aim for.
    pub target: TimestampAnchor,
    /// Largest input pts released so far with this anchor; sizes the slew
    /// steps and keeps tracks sharing the anchor in pts order.
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TrackState {
    /// Written chunks are buffered and not released yet.
    Waiting,
    /// Chunks are released with the anchor `Mode` holds for the track.
    Started,
}

pub(super) struct StateView {
    options: LiveSyncOptions,
    now_pts: Timestamp,
    mode: Mode,
    shared_estimation: Option<EdgeEstimate>,
    audio: Option<TrackView>,
    video: Option<TrackView>,
}

pub(super) struct TrackView {
    state: TrackState,
    estimation: Option<EdgeEstimate>,
    buffer_empty: bool,
    last_released_pts: Option<Timestamp>,
}

impl StateView {
    fn track(&self, kind: TrackKind) -> Option<&TrackView> {
        match kind {
            TrackKind::Audio => self.audio.as_ref(),
            TrackKind::Video => self.video.as_ref(),
        }
    }

    fn should_reset(&self, kind: TrackKind) -> bool {
        match self.track(kind) {
            Some(track) => track.should_reset(self),
            None => false,
        }
    }

    /// Mode after a waiting track of `kind` starts; `None` while it should
    /// keep waiting.
    fn start_decision(&self, kind: TrackKind) -> Option<Mode> {
        let track = self.track(kind)?;
        if track.state != TrackState::Waiting {
            return None;
        }
        let track_estimation = track.estimation?;
        let shared_estimation = self.shared_estimation?;

        let both_stable =
            track_estimation.upper_bound.stable && shared_estimation.upper_bound.stable;
        let waiting_too_long = track_estimation.delivery.observed_for >= self.options.max_wait;
        if !both_stable && !waiting_too_long {
            return None;
        }

        let strategy = self.options.buffering_strategy;
        let shared_timeline = self.tracks_share_timeline(Duration::from_secs(10));

        // shared_timeline in line with existing mode, so keep shared mode
        if let (Mode::Shared(anchor), Some(true) | None) = (self.mode, shared_timeline) {
            return Some(Mode::Shared(anchor));
        }

        // Mode not established yet, calculate new shared anchor (if timeline is shared or unknown)
        if let (Mode::Undecided, Some(true) | None) = (self.mode, shared_timeline) {
            let anchor = strategy.desired_anchor(&shared_estimation, self.now_pts);
            return Some(Mode::Shared(Anchor::new(anchor)));
        }

        // At this point we know that tracks will be independent. This value represents
        // a new anchor of the other track.
        let other_track_anchor = match self.mode {
            Mode::Undecided => None,
            Mode::Shared(anchor) => Some(anchor),
            Mode::Independent { audio, video } => match kind {
                TrackKind::Audio => video,
                TrackKind::Video => audio,
            },
        };

        Some(match kind {
            TrackKind::Audio => {
                let anchor = strategy.desired_anchor(&track_estimation, self.now_pts);
                Mode::Independent {
                    audio: Some(Anchor::new(anchor)),
                    video: other_track_anchor,
                }
            }
            TrackKind::Video => {
                let audio_estimation = self.audio.as_ref().and_then(|audio| audio.estimation);
                let anchor = match (other_track_anchor, audio_estimation) {
                    (Some(audio_anchor), Some(estimation)) if estimation.upper_bound.stable => {
                        let audio_pts = estimation.upper_bound.pts;
                        let video_pts = track_estimation.upper_bound.pts;
                        // anchor that will translate video pts so the live edge of the
                        // video matches, produces the same output pts as live edge of the
                        // audio
                        TimestampAnchor {
                            input_pts: video_pts,
                            output_pts: audio_anchor.target.to_output_pts(audio_pts),
                        }
                    }
                    _ => strategy.desired_anchor(&track_estimation, self.now_pts),
                };
                Mode::Independent {
                    audio: other_track_anchor,
                    video: Some(Anchor::new(anchor)),
                }
            }
        })
    }

    /// Heuristic that decides if all tracks are on the same timeline. Live
    /// edges closer than `threshold` are treated as the same timeline. `None`
    /// when there is not enough information to decide either way.
    fn tracks_share_timeline(&self, threshold: Duration) -> Option<bool> {
        let (Some(audio), Some(video)) = (&self.audio, &self.video) else {
            return None;
        };
        let (Some(audio), Some(video)) = (audio.estimation, video.estimation) else {
            return None;
        };
        let (audio, video) = (audio.upper_bound, video.upper_bound);

        let diff = (audio.pts - video.pts).abs();
        // If diff is that large we ignore stability, timelines have to be diverged
        if diff >= Timestamp::from_secs(120) {
            return Some(false);
        }

        // If diff is over the threshold we check stability too before deciding
        if diff < Timestamp::from(threshold) {
            return match audio.stable && video.stable {
                true => Some(true),
                false => None,
            };
        }

        match (audio.stable, video.stable) {
            (true, true) => Some(false),
            // unstable track ahead of the stable one only diverges further;
            // behind it could be backlog
            (true, false) => match video.pts < audio.pts {
                true => None,
                false => Some(false),
            },
            (false, true) => match audio.pts < video.pts {
                true => None,
                false => Some(false),
            },
            (false, false) => None,
        }
    }
}

impl TrackView {
    fn should_reset(&self, shared: &StateView) -> bool {
        if self.state == TrackState::Waiting {
            return false;
        }
        if !self.buffer_empty {
            return false;
        }
        let Some(last_pts) = self.last_released_pts else {
            return false;
        };

        // Slightly late track can still recover; reset would cause a gap of at
        // least the stabilization period. 5s late is considered unrecoverable.
        last_pts + Timestamp::from_secs(5) <= shared.now_pts
    }
}
