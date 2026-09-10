use std::time::{Duration, Instant};

use super::{
    buffer::LiveSyncBuffer,
    edge_estimator::EdgeEstimate,
    mode::{IndependentMode, Mode, SecondaryTrackOffset, SharedMode, SlewingAnchor},
    state::{SharedState, edge_offset},
};
use crate::{Timestamp, pipeline::utils::input_sync::TrackKind};

/// Mode after this tick's corrections; the current mode when nothing changes.
pub(super) fn correct_mode_decision<B: LiveSyncBuffer>(
    state: &SharedState<B>,
    now: Instant,
) -> Option<Mode> {
    let now_pts = state.now_pts(now);
    let strategy = state.options.buffering_strategy;

    let audio = state.track(TrackKind::Audio);
    let video = state.track(TrackKind::Video);

    let audio_estimation = audio.and_then(|a| a.estimator.estimate(now));
    let video_estimation = video.and_then(|a| a.estimator.estimate(now));

    let tracks_diverged =
        EdgeEstimate::timelines_diverged(audio_estimation.as_ref(), video_estimation.as_ref());
    let tracks_converged =
        EdgeEstimate::timelines_converged(audio_estimation.as_ref(), video_estimation.as_ref());

    let audio_started = state.is_started(TrackKind::Audio);
    let video_started = state.is_started(TrackKind::Video);

    let Some(mode) = state.mode else {
        // Mode is not defined only when nothing is started, so
        // by definition nothing to correct
        return None;
    };

    Some(match mode {
        // Tracks turned out to be on different timelines. The started track keeps the anchor
        // and leads (audio if both); the other is aligned to it from the edges once started.
        Mode::Shared(mode) if tracks_diverged && (audio_started || video_started) => {
            let (leader_kind, secondary_track_offset) = match (audio_started, video_started) {
                (true, true) => {
                    // If both started then estimation has to be Some()
                    let audio = audio_estimation?.upper_bound.pts;
                    let video = video_estimation?.upper_bound.pts;
                    // the shared anchor is an offset of zero; slew from there
                    (
                        TrackKind::Audio,
                        Some(SecondaryTrackOffset {
                            current: Timestamp::ZERO,
                            target: audio - video,
                            last_released_pts: None,
                        }),
                    )
                }
                (true, false) => (TrackKind::Audio, None),
                (false, true) => (TrackKind::Video, None),
                (false, false) => unreachable!(),
            };
            // The shared target was sized by an estimate the other track polluted; the leader's
            // own estimate sizes it from now on.
            let leader_estimation = match leader_kind {
                TrackKind::Audio => audio_estimation?,
                TrackKind::Video => video_estimation?,
            };
            Mode::Independent(IndependentMode {
                leader_kind,
                leader_anchor: SlewingAnchor {
                    target: strategy.desired_anchor(&leader_estimation, now_pts),
                    ..mode.anchor
                },
                secondary_track_offset,
            })
        }
        // Tracks turned out to be on the same timeline; the secondary track jumps by the
        // offset.
        Mode::Independent(mode) if tracks_converged => Mode::Shared(SharedMode {
            anchor: mode.leader_anchor,
            audio_started,
            video_started,
        }),
        Mode::Shared(mut mode) => {
            if let Some(estimation) = state.shared_estimator.estimate(now)
                && !strategy.buffer_in_range(&estimation, mode.anchor.current, now_pts)
            {
                mode.anchor.target = strategy.desired_anchor(&estimation, now_pts);
            }
            if !audio_started && !video_started {
                // Nothing releases chunks, so nothing would nudge the anchor; and no
                // continuity has to be preserved before the first chunk or after a reset.
                mode.anchor.current = mode.anchor.target;
            }
            Mode::Shared(mode)
        }
        Mode::Independent(mut mode) => {
            let leader_kind = mode.leader_kind;
            let leader_estimation = match mode.leader_kind {
                TrackKind::Audio => audio_estimation,
                TrackKind::Video => video_estimation,
            };
            // the leader's buffer is sized by the strategy
            if let Some(estimation) = leader_estimation
                && !strategy.buffer_in_range(&estimation, mode.leader_anchor.current, now_pts)
            {
                mode.leader_anchor.target = strategy.desired_anchor(&estimation, now_pts);
            }

            // the secondary track is re-aligned once the stable edges moved
            const OFFSET_TOLERANCE: Duration = Duration::from_millis(50);
            if let (Some(offset), Some(audio), Some(video)) = (
                &mut mode.secondary_track_offset,
                audio_estimation,
                video_estimation,
            ) && audio.upper_bound.stable
                && video.upper_bound.stable
            {
                let edges_offset = edge_offset(&audio, &video, leader_kind);
                if (edges_offset - offset.target).abs() > Timestamp::from(OFFSET_TOLERANCE) {
                    offset.target = edges_offset;
                }
            }

            Mode::Independent(mode)
        }
    })
}
