use std::time::Duration;

use super::{
    LiveSyncOptions,
    edge_estimator::EdgeEstimate,
    mode::{Anchor, Mode, TrackState},
};
use crate::{
    Timestamp,
    pipeline::utils::input_sync::{TimestampAnchor, TrackKind},
};

/// Read-only snapshot of [`SharedState`](super::state::SharedState) the decisions are made
/// from. Valid until the state is mutated.
pub(super) struct StateView {
    pub options: LiveSyncOptions,
    pub now_pts: Timestamp,
    pub mode: Mode,
    pub shared_estimation: Option<EdgeEstimate>,
    pub audio: Option<TrackView>,
    pub video: Option<TrackView>,
}

pub(super) struct TrackView {
    pub state: TrackState,
    pub estimation: Option<EdgeEstimate>,
    pub buffer_empty: bool,
    /// Output pts the released content ends at.
    pub last_released_pts: Option<Timestamp>,
}

impl StateView {
    fn track(&self, kind: TrackKind) -> Option<&TrackView> {
        match kind {
            TrackKind::Audio => self.audio.as_ref(),
            TrackKind::Video => self.video.as_ref(),
        }
    }

    /// Track stalled long enough that it has to earn its start again.
    pub fn should_reset(&self, kind: TrackKind) -> bool {
        match self.track(kind) {
            Some(track) => track.should_reset(self),
            None => false,
        }
    }

    /// Mode after a waiting track of `kind` starts; `None` while it should keep waiting.
    pub fn start_decision(&self, kind: TrackKind) -> Option<Mode> {
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

        const SPLIT_THRESHOLD: Duration = Duration::from_secs(10);

        let strategy = self.options.buffering_strategy;
        let shared_timeline =
            TrackView::is_timeline_shared(&self.audio, &self.video, SPLIT_THRESHOLD);

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

    /// Mode after this tick's corrections; the current mode when nothing changes.
    pub fn correct_decision(&self) -> Mode {
        // split threshold is looser than the converge one, so the mode cannot flap
        const SPLIT_THRESHOLD: Duration = Duration::from_secs(5);
        const MERGE_THRESHOLD: Duration = Duration::from_secs(3);

        let strategy = self.options.buffering_strategy;
        let tracks_diverged =
            TrackView::is_timeline_shared(&self.audio, &self.video, SPLIT_THRESHOLD) == Some(false);
        let tracks_converged =
            TrackView::is_timeline_shared(&self.audio, &self.video, MERGE_THRESHOLD) == Some(true);

        match self.mode {
            Mode::Undecided => Mode::Undecided,
            // Tracks turned out to be on different timelines. Every started track keeps the anchor
            // value as its own, so the switch does not affect output.
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

        // video follows audio's live edge; sizes its own buffer when audio is not running or its
        // edge is not stable yet
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

    // Anchor that will transform video pts, in a way that would transform current video
    // edge to the same output pts as current audio edge.
    // If audio edge is unstable return None
    fn video_anchor_following_audio_edge(&self, audio: Option<Anchor>) -> Option<TimestampAnchor> {
        let audio = audio?;
        let audio_edge = self.audio.as_ref()?.estimation?.upper_bound;
        let video_edge = self.video.as_ref()?.estimation?.upper_bound;
        if !audio_edge.stable {
            return None;
        }

        Some(TimestampAnchor {
            input_pts: video_edge.pts,
            output_pts: audio.target.to_output_pts(audio_edge.pts),
        })
    }
}

impl TrackView {
    /// Heuristic that decides if all tracks are on the same timeline. Live
    /// edges closer than `threshold` are treated as the same timeline. `None`
    /// when there is not enough information to decide either way.
    fn is_timeline_shared(
        audio: &Option<TrackView>,
        video: &Option<TrackView>,
        threshold: Duration,
    ) -> Option<bool> {
        let (Some(audio), Some(video)) = (audio.as_ref()?.estimation, video.as_ref()?.estimation)
        else {
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
