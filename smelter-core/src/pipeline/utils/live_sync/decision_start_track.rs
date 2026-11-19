use std::time::Instant;

use super::{
    buffer::LiveSyncBuffer,
    edge_estimator::EdgeEstimate,
    mode::{IndependentMode, Mode, SecondaryTrackOffset, SharedMode, SlewingAnchor},
    state::SharedState,
};
use crate::{InstantExt, Timestamp, pipeline::utils::input_sync::TrackKind};

/// Mode after a waiting track of `kind` starts; `None` while it should keep waiting.
pub(super) fn start_track_decision<B: LiveSyncBuffer>(
    state: &SharedState<B>,
    kind: TrackKind,
    now: Instant,
) -> Option<Mode> {
    if state.is_started(kind) {
        return None;
    }
    let audio_estimation = state
        .track(TrackKind::Audio)
        .and_then(|audio| audio.estimator.estimate(now));
    let video_estimation = state
        .track(TrackKind::Video)
        .and_then(|video| video.estimator.estimate(now));
    let track_estimation = match kind {
        TrackKind::Audio => audio_estimation,
        TrackKind::Video => video_estimation,
    }?;
    let shared_estimation =
        EdgeEstimate::combine(audio_estimation.as_ref(), video_estimation.as_ref())?;

    if state.track(kind)?.waits_for_gap_end && !track_estimation.lower_bound.stable {
        return None;
    }

    // every track that delivers has to be stable, this one included
    let both_stable = audio_estimation.is_none_or(|audio| audio.upper_bound.stable)
        && video_estimation.is_none_or(|video| video.upper_bound.stable);
    let waiting_too_long = track_estimation.delivery.observed_for >= state.options.max_wait;
    if !both_stable && !waiting_too_long {
        return None;
    }

    let now_pts = state.sync_point.timestamp_at(now);
    let strategy = state.options.buffering_strategy;

    let tracks_diverged =
        EdgeEstimate::timelines_diverged(audio_estimation.as_ref(), video_estimation.as_ref());
    let tracks_converged =
        EdgeEstimate::timelines_converged(audio_estimation.as_ref(), video_estimation.as_ref());

    // timelines not known to differ, so keep shared mode
    if let Some(Mode::Shared(shared)) = state.mode
        && !tracks_diverged
    {
        return Some(Mode::Shared(shared.with_started(kind)));
    }

    // Mode not established yet, calculate new shared anchor (if timeline is shared or unknown)
    if let None = state.mode
        && !tracks_diverged
    {
        let priority = state
            .options
            .track_priority
            .select(audio_estimation.as_ref(), video_estimation.as_ref());
        let anchor = strategy.desired_anchor(&shared_estimation, priority, now_pts);
        return Some(Mode::Shared(
            SharedMode::from_anchor(anchor).with_started(kind),
        ));
    }

    // Timelines turned out to be shared, so join the leader's anchor instead of aligning by edges;
    // the leader does not move.
    if let Some(Mode::Independent(independent)) = state.mode
        && tracks_converged
    {
        return Some(Mode::Shared(SharedMode {
            anchor: independent.leader_anchor,
            // the leader is the other track and this one starts now
            audio_started: true,
            video_started: true,
        }));
    }

    // At this point we decided that the tracks are going to be independent.

    // Audio leads whenever it runs; a track running alone gets its own anchor. A track that starts
    // while the other one plays joins the other's mapping shifted by the edge offset, so the
    // playing track does not move.
    Some(Mode::Independent(match kind {
        // Case: Video track already started and now audio is starting
        //
        // Select audio as a leader. Calculate anchor so it produces the same anchor as
        // old video track when shifted by an offset.
        TrackKind::Audio
            if let Some(mode) = state.mode
                && mode.is_started(TrackKind::Video) =>
        {
            // Always some, because we know that video already started
            let video_estimation = video_estimation?;
            let old_video_anchor = mode.anchor(TrackKind::Video)?;

            let video_offset = Timestamp::offset(
                video_estimation.upper_bound.pts,
                track_estimation.upper_bound.pts,
            );

            // Calculate audio anchor that will produce old video anchor when shifted
            // by the offset, so video does not move; the buffer is sized by audio's own
            // estimate from now on.
            let current = old_video_anchor - video_offset;
            let target = strategy.desired_anchor(&track_estimation, None, now_pts);

            IndependentMode {
                leader_kind: TrackKind::Audio,
                leader_anchor: SlewingAnchor {
                    current,
                    target,
                    last_released_pts: None,
                },
                secondary_track_offset: Some(SecondaryTrackOffset::new(video_offset)),
            }
        }

        // Case: No tracks started yet, audio is starting now
        //
        // Audio is selected as a leader with anchor calculated based on estimator results
        TrackKind::Audio => IndependentMode {
            leader_kind: TrackKind::Audio,
            leader_anchor: SlewingAnchor::new(strategy.desired_anchor(
                &track_estimation,
                None,
                now_pts,
            )),
            secondary_track_offset: None,
        },

        // Case: Audio track already started and now video is starting
        //
        // Leader anchor is inherited from previous state
        TrackKind::Video
            if let Some(mode) = state.mode
                && mode.is_started(TrackKind::Audio) =>
        {
            // Always some, because we know that audio already started
            let audio_estimation = audio_estimation?;

            let video_offset = Timestamp::offset(
                track_estimation.upper_bound.pts,
                audio_estimation.upper_bound.pts,
            );

            IndependentMode {
                leader_kind: TrackKind::Audio,
                leader_anchor: match mode {
                    // the shared target was sized from both tracks, including the diverged video,
                    // so recalculate it from audio's own estimate
                    Mode::Shared(shared) => SlewingAnchor {
                        target: strategy.desired_anchor(&audio_estimation, None, now_pts),
                        ..shared.anchor
                    },
                    Mode::Independent(independent) => independent.leader_anchor,
                },
                secondary_track_offset: Some(SecondaryTrackOffset::new(video_offset)),
            }
        }

        // Case: No tracks started yet, video is starting now
        //
        // Video is selected as a leader with anchor calculated based on estimator results
        TrackKind::Video => IndependentMode {
            leader_kind: TrackKind::Video,
            leader_anchor: SlewingAnchor::new(strategy.desired_anchor(
                &track_estimation,
                None,
                now_pts,
            )),
            secondary_track_offset: None,
        },
    }))
}
