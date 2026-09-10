use std::time::{Duration, Instant};

use crate::{
    Timestamp,
    utils::{
        input_sync::TrackKind,
        live_sync::{edge_estimator::EdgeEstimate, state::StartState},
    },
};

pub(super) struct StateView {
    now: Instant,
    now_pts: Timestamp,
    estimation: Option<EdgeEstimate>,
    audio: Option<TrackView>,
    video: Option<TrackView>,
}

pub(super) struct TrackView {
    estimation: Option<EdgeEstimate>,
    state: StartState,
    buffer_empty: bool,
    last_released_pts: Option<Timestamp>,
}

pub(super) struct SharedView {}

impl StateView {
    fn should_reset(&self, kind: TrackKind) -> bool {
        match kind {
            TrackKind::Audio if let Some(a) = &self.audio => a.should_reset(self),
            TrackKind::Video if let Some(v) = &self.video => v.should_reset(self),
            _ => false,
        }
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
        if matches!(self.state, StartState::WaitingForStart) {
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
        return last_pts + Timestamp::from_secs(5) < shared.now_pts;
    }
}
