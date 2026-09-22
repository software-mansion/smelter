use std::time::{Duration, Instant};

use super::{
    buffer::{BufferingStrategy, LiveSyncBuffer},
    edge_estimator::EdgeEstimate,
    mode::{IndependentMode, Mode, SecondaryTrackOffset, SharedMode, SlewingAnchor},
    state::SharedState,
};
use crate::{InstantExt, Timestamp, TimestampOffset, pipeline::utils::input_sync::TrackKind};

/// Mode after this tick's corrections; the current mode when nothing changes.
pub(super) fn correct_mode_decision<B: LiveSyncBuffer>(
    state: &SharedState<B>,
    now: Instant,
) -> Option<Mode> {
    // Mode is not defined only when nothing is started, so by definition nothing to correct
    let mode = state.mode?;
    let ctx = Context::new(state, now);

    Some(match mode {
        Mode::Shared(mode) if ctx.tracks_diverged() && (ctx.audio_started || ctx.video_started) => {
            let (Some(audio), Some(video)) = (&ctx.audio_estimation, &ctx.video_estimation) else {
                // tracks are diverged only when both have an estimate
                return Some(Mode::Shared(mode));
            };
            split_shared(mode, audio, video, &ctx)
        }
        Mode::Shared(mode) => correct_shared(mode, state.shared_estimator.estimate(now), &ctx),
        Mode::Independent(mode) => {
            let leader_estimation = match mode.leader_kind {
                TrackKind::Audio => ctx.audio_estimation,
                TrackKind::Video => ctx.video_estimation,
            };
            let Some(leader_estimation) = leader_estimation else {
                // leader is started, so it has an estimate (see `Track::estimator`)
                return Some(Mode::Independent(mode));
            };
            correct_independent(mode, &leader_estimation, &ctx)
        }
    })
}

/// What the corrections read from the state, the same for every case.
struct Context {
    now_pts: Timestamp,
    strategy: BufferingStrategy,
    audio_started: bool,
    video_started: bool,
    audio_estimation: Option<EdgeEstimate>,
    video_estimation: Option<EdgeEstimate>,
}

impl Context {
    fn new<B: LiveSyncBuffer>(state: &SharedState<B>, now: Instant) -> Self {
        let estimation = |kind| {
            state
                .track(kind)
                .and_then(|track| track.estimator.estimate(now))
        };
        Self {
            now_pts: state.sync_point.timestamp_at(now),
            strategy: state.options.buffering_strategy,
            audio_started: state.is_started(TrackKind::Audio),
            video_started: state.is_started(TrackKind::Video),
            audio_estimation: estimation(TrackKind::Audio),
            video_estimation: estimation(TrackKind::Video),
        }
    }

    fn tracks_diverged(&self) -> bool {
        EdgeEstimate::timelines_diverged(
            self.audio_estimation.as_ref(),
            self.video_estimation.as_ref(),
        )
    }

    fn tracks_converged(&self) -> bool {
        EdgeEstimate::timelines_converged(
            self.audio_estimation.as_ref(),
            self.video_estimation.as_ref(),
        )
    }
}

/// Tracks turned out to be on different timelines. The started track keeps the anchor and leads
/// (audio if both); the other is aligned to it from the edges once started.
fn split_shared(
    shared: SharedMode,
    audio: &EdgeEstimate,
    video: &EdgeEstimate,
    ctx: &Context,
) -> Mode {
    // tracks diverged, so shared estimator might be polluted, so use estimator from the new
    // leader
    let (leader_kind, leader_estimation) = match ctx.audio_started {
        true => (TrackKind::Audio, audio),
        false => (TrackKind::Video, video),
    };

    let secondary_track_offset = match ctx.audio_started && ctx.video_started {
        // the shared anchor is an offset of zero; slew from there
        true => Some(SecondaryTrackOffset {
            current: TimestampOffset::ZERO,
            target: Timestamp::offset(video.upper_bound.pts, audio.upper_bound.pts),
            last_released_pts: None,
        }),
        false => None,
    };

    Mode::Independent(IndependentMode {
        leader_kind,
        leader_anchor: SlewingAnchor {
            current: shared.anchor.current,
            target: ctx.strategy.desired_anchor(leader_estimation, ctx.now_pts),
            // the shared value may come from the other track, which would stall the leader's
            // slew until its own pts pass it
            last_released_pts: None,
        },
        secondary_track_offset,
    })
}

/// The shared buffer is sized by the strategy from the shared estimate.
fn correct_shared(
    mut shared: SharedMode,
    shared_estimation: Option<EdgeEstimate>,
    ctx: &Context,
) -> Mode {
    if let Some(estimation) = shared_estimation
        && !ctx
            .strategy
            .buffer_in_range(&estimation, shared.anchor.current, ctx.now_pts)
    {
        shared.anchor.target = ctx.strategy.desired_anchor(&estimation, ctx.now_pts);
    }
    if !ctx.audio_started && !ctx.video_started {
        // We need to do it because current will never converge on target (nothing is actively
        // nudging it)
        // We can do it because no track is started, so no continuity to preserve
        shared.anchor.current = shared.anchor.target;
    }
    Mode::Shared(shared)
}

/// The leader's buffer is sized by the strategy from its own estimate; the secondary track is
/// re-aligned to it, or the tracks merge back once their timelines converge.
fn correct_independent(
    mut independent: IndependentMode,
    leader_estimation: &EdgeEstimate,
    ctx: &Context,
) -> Mode {
    let leader_anchor = &mut independent.leader_anchor;
    if !ctx
        .strategy
        .buffer_in_range(leader_estimation, leader_anchor.current, ctx.now_pts)
    {
        leader_anchor.target = ctx.strategy.desired_anchor(leader_estimation, ctx.now_pts);
    }

    if ctx.tracks_converged() {
        return match &mut independent.secondary_track_offset {
            // Stay on Mode::Independent, but start converging on shared
            Some(offset) if offset.current.abs_duration() > Duration::from_millis(200) => {
                offset.target = TimestampOffset::ZERO;
                Mode::Independent(independent)
            }
            // If only track or already converged, switch to shared
            _ => Mode::Shared(SharedMode {
                anchor: independent.leader_anchor,
                audio_started: ctx.audio_started,
                video_started: ctx.video_started,
            }),
        };
    }

    // The secondary track is re-aligned according to distance between live edges. It happens
    // only when we are sure that `tracks_diverged` and not just `!tracks_converged`.
    const OFFSET_TOLERANCE: Duration = Duration::from_millis(100);
    if let (Some(offset), Some(audio), Some(video)) = (
        &mut independent.secondary_track_offset,
        ctx.audio_estimation,
        ctx.video_estimation,
    ) && ctx.tracks_diverged()
        && audio.upper_bound.stable
        && video.upper_bound.stable
    {
        // offset that presents both live edges at once; the secondary is always video, since
        // audio leads whenever it runs
        let edge_offset = Timestamp::offset(video.upper_bound.pts, audio.upper_bound.pts);

        // what the leader still has to slew; added so the secondary track goes straight to its
        // final position instead of following the leader there
        let leader = independent.leader_anchor;
        let leader_remaining_slew = leader.target - leader.current;

        let new_offset = edge_offset + leader_remaining_slew;

        // Video lagging behind audio is noticed far sooner than video running ahead, so only a
        // move that presents video later waits for the tolerance
        if new_offset < offset.target || new_offset > offset.target + OFFSET_TOLERANCE {
            offset.target = new_offset;
        }
    }

    Mode::Independent(independent)
}
