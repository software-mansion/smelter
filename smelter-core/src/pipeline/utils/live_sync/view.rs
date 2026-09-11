use std::time::Duration;

use crate::{
    Timestamp,
    pipeline::utils::input_sync::TimestampAnchor,
    utils::{
        input_sync::TrackKind,
        live_sync::{LiveSyncOptions, edge_estimator::EdgeEstimate, state::MIN_QUEUE_HEADROOM},
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
    /// Oldest buffered pts, the next one to be released; `None` when empty.
    oldest_buffered_pts: Option<Timestamp>,
    /// Newest buffered pts; `None` when empty.
    newest_buffered_pts: Option<Timestamp>,
    /// Output pts the released content ends at.
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

    /// Mapping to flush the buffer with on reset: the one the track applies
    /// when it is started, otherwise a best effort one. `None` when there is
    /// nothing to build it from.
    fn flush_anchor(&self, kind: TrackKind) -> Option<TimestampAnchor> {
        let track = self.track(kind)?;
        if track.state == TrackState::Started {
            return self.mode.anchor(kind).map(|anchor| anchor.current);
        }

        // Try to maintain continuity if there is still time to reach queue:
        // the oldest buffered chunk picks the timeline up where the released
        // content ended.
        if let Some(last_pts) = track.last_released_pts
            && last_pts > self.now_pts + MIN_QUEUE_HEADROOM
        {
            return Some(TimestampAnchor {
                input_pts: track.oldest_buffered_pts?,
                output_pts: last_pts,
            });
        }

        // Nothing to continue from, so the newest buffered chunk stands in for the live edge.
        // As result effective buffer is exactly desired buffer.
        Some(TimestampAnchor {
            input_pts: track.newest_buffered_pts?,
            output_pts: self.now_pts + self.options.buffering_strategy.desired_buffer(),
        })
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
                let video_anchor = self
                    .video_anchor_following_audio_edge(other_track_anchor)
                    .unwrap_or_else(|| strategy.desired_anchor(&track_estimation, self.now_pts));
                Mode::Independent {
                    audio: other_track_anchor,
                    video: Some(Anchor::new(video_anchor)),
                }
            }
        })
    }

    /// Mode after this tick's corrections; the current mode when nothing
    /// changes.
    fn correct_decision(&self) -> Mode {
        let strategy = self.options.buffering_strategy;

        // split threshold is looser than the converge one, so the mode cannot flap
        let tracks_diverged = self.tracks_share_timeline(Duration::from_secs(10)) == Some(false);
        let tracks_converged = self.tracks_share_timeline(Duration::from_secs(3)) == Some(true);

        match self.mode {
            Mode::Undecided => Mode::Undecided,
            // Tracks turned out to be on different timelines. Every started track
            // keeps the anchor value as its own, so the switch does not affect output.
            Mode::Shared(anchor) if tracks_diverged => Mode::Independent {
                audio: match &self.audio {
                    Some(track) if track.state == TrackState::Started => Some(anchor),
                    _ => None,
                },
                video: match &self.video {
                    Some(track) if track.state == TrackState::Started => Some(anchor),
                    _ => None,
                },
            },
            // Tracks turned out to be on the same timeline.
            Mode::Independent {
                audio: Some(audio),
                video: Some(mut video),
            } if tracks_converged => {
                video.target = audio.current;
                match audio.current.distance_to(video.current) < Timestamp::from_millis(50) {
                    true => Mode::Shared(audio),
                    // keep independent mode, but slew towards new target when releasing chunks
                    false => Mode::Independent {
                        audio: Some(audio),
                        video: Some(video),
                    },
                }
            }
            Mode::Shared(mut anchor) => {
                if let Some(estimation) = self.shared_estimation
                    && !strategy.buffer_in_range(estimation, anchor.current, self.now_pts)
                {
                    anchor.target = strategy.desired_anchor(&estimation, self.now_pts);
                }
                let mut tracks = [&self.audio, &self.video].into_iter().flatten();
                if !tracks.any(|track| track.state == TrackState::Started) {
                    // It is safe to do because it's first packet, or after reset or stall, no
                    // continuity needs to be preserved.
                    //
                    // We need to do this, because nothing nudges it at this state, do different
                    // track can converge from Independent to Shared and hit outdated value.
                    anchor.current = anchor.target;
                }
                Mode::Shared(anchor)
            }
            Mode::Independent { audio, video } => self.correct_independent(audio, video),
        }
    }

    /// Corrections of the own anchors in independent mode.
    fn correct_independent(&self, mut audio: Option<Anchor>, mut video: Option<Anchor>) -> Mode {
        let strategy = self.options.buffering_strategy;
        let audio_estimation = self.audio.as_ref().and_then(|audio| audio.estimation);
        let video_estimation = self.video.as_ref().and_then(|video| video.estimation);

        // audio leads: its buffer is sized by the strategy
        if let (Some(anchor), Some(estimation)) = (audio.as_mut(), audio_estimation)
            && !strategy.buffer_in_range(estimation, anchor.current, self.now_pts)
        {
            anchor.target = strategy.desired_anchor(&estimation, self.now_pts);
        }

        // video follows audio's live edge; sizes its own buffer when audio is not
        // running or its edge is not stable yet
        if let (Some(anchor), Some(estimation)) = (video.as_mut(), video_estimation) {
            match self.video_anchor_following_audio_edge(audio) {
                Some(following) => {
                    let diff = following.distance_to(anchor.target);
                    if estimation.upper_bound.stable && diff > Timestamp::from_millis(50) {
                        anchor.target = following;
                    }
                }
                None => {
                    if !strategy.buffer_in_range(estimation, anchor.current, self.now_pts) {
                        anchor.target = strategy.desired_anchor(&estimation, self.now_pts);
                    }
                }
            }
        }

        Mode::Independent { audio, video }
    }

    /// Mapping that presents video's live edge at the output pts where `audio`
    /// presents audio's; `None` while audio is not running or its edge is
    /// not stable.
    fn video_anchor_following_audio_edge(&self, audio: Option<Anchor>) -> Option<TimestampAnchor> {
        let audio = audio?;
        let audio_edge = self.audio.as_ref()?.estimation?.upper_bound;
        let video_edge = self.video.as_ref()?.estimation?.upper_bound;
        if !audio_edge.stable {
            return None;
        }

        // This anchor will transform video pts, in a way that would transform
        // current video edge to the same output pts as current audio edge
        Some(TimestampAnchor {
            input_pts: video_edge.pts,
            output_pts: audio.target.to_output_pts(audio_edge.pts),
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
        if self.oldest_buffered_pts.is_some() {
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
