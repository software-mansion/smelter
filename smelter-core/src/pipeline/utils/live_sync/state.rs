use std::time::{Duration, Instant};

use tracing::{debug, trace};

use super::{
    LiveSyncOptions, buffer::LiveSyncBuffer, edge_estimator::LiveEdgeEstimator,
    stats::LiveSyncTrackStats,
};
use crate::pipeline::utils::input_sync::{
    BoxedTrackSink, InputSyncItem, InputSyncStatsSender, TimestampAnchor, TrackClosedError,
    TrackEvent, TrackKind,
};

use crate::prelude::*;

/// pts jump (in either direction) treated as a discontinuity of the input
/// timeline; the old edge estimate does not describe the new timeline.
const DISCONTINUITY_THRESHOLD: Duration = Duration::from_secs(10);

/// Lead over the playback position below which chunks are force-released.
const MIN_QUEUE_HEADROOM: Duration = Duration::from_millis(100);

/// Smaller changes of the edge-aligned target are ignored, so estimator
/// jitter does not keep nudging a following track.
const FOLLOW_TOLERANCE: Timestamp = Timestamp::from_millis(50);

/// The whole mutable state of an input, cross-track and per-track, kept
/// behind one mutex; [`LiveSync`] and [`LiveSyncTrack`] are thin handles to
/// it.
pub(super) struct SharedState<B: LiveSyncBuffer> {
    options: LiveSyncOptions,
    /// Instant that output timestamps are measured from.
    sync_point: Instant,
    /// Estimator observing chunks of all tracks; its edge is defined by the
    /// freshest track.
    shared_estimator: LiveEdgeEstimator,
    /// The one mapping every track aligned to the shared edge applies;
    /// established by the first track whose start decision fires, adopted by
    /// the other.
    anchor: Option<SharedAnchor>,
    audio: Option<TrackState<B>>,
    video: Option<TrackState<B>>,
    stats_sender: InputSyncStatsSender,
}

/// Anchor of the tracks aligned to the shared edge. Corrections move
/// `target`; `current` slews towards it in small steps as chunks are read.
#[derive(Debug, Clone, Copy)]
pub(super) struct SharedAnchor {
    /// Mapping applied to every chunk read right now.
    pub current: TimestampAnchor,
    /// Mapping the corrections aim for.
    pub target: TimestampAnchor,
    /// Largest input pts released so far by any track applying this anchor.
    ///
    /// Used to maintain interleaved (by pts) order on sync output between tracks.
    /// Reset (together with the entire anchor) on discontinuity.
    last_released_pts: Option<Timestamp>,
}

/// Where a started track presents its live edge. A track running its own
/// anchor aligns to it, so both tracks present their live edges at the same
/// output pts even when their input timelines are unrelated.
#[derive(Debug, Clone, Copy)]
pub(super) struct EdgeReference {
    pub anchor: TimestampAnchor,
    /// Live edge (stable upper bound) of the track, in its input pts.
    pub edge_pts: Timestamp,
}

impl EdgeReference {
    /// Anchor presenting `edge_pts` of another track at the same output pts
    /// as this reference presents its own edge.
    pub fn aligned_anchor(&self, edge_pts: Timestamp) -> TimestampAnchor {
        TimestampAnchor {
            input_pts: edge_pts,
            output_pts: self.anchor.to_output_pts(self.edge_pts),
        }
    }
}

impl<B: LiveSyncBuffer> SharedState<B> {
    pub(super) fn new(
        options: LiveSyncOptions,
        sync_point: Instant,
        stats_sender: InputSyncStatsSender,
    ) -> Self {
        Self {
            options,
            sync_point,
            shared_estimator: LiveEdgeEstimator::new(
                sync_point,
                options.stabilization_tolerance,
                options.stabilization_period,
            ),
            anchor: None,
            audio: None,
            video: None,
            stats_sender,
        }
    }

    pub(super) fn add_track(&mut self, kind: TrackKind, sink: BoxedTrackSink<B::Chunk>) {
        debug!(?kind, "Live sync: adding track");
        let track = TrackState {
            kind,
            options: self.options,
            sync_point: self.sync_point,
            estimator: LiveEdgeEstimator::new(
                self.sync_point,
                self.options.stabilization_tolerance,
                self.options.stabilization_period,
            ),
            start: StartState::WaitingForStart,
            buffer: B::default(),
            sink,
            last_released_pts: None,
            last_written: None,
            stats: LiveSyncTrackStats::new(&self.stats_sender, kind, self.sync_point),
        };
        match kind {
            TrackKind::Audio => self.audio = Some(track),
            TrackKind::Video => self.video = Some(track),
        }
    }

    /// Runs every transition due at `now` (resets, start decisions,
    /// corrections) and releases every releasable chunk. Driven by writes and
    /// by the periodic ticker, so time-based transitions fire during delivery
    /// pauses too.
    pub(super) fn tick(&mut self, now: Instant) {
        self.drop_closed_tracks();
        self.maybe_reset(now);
        self.maybe_start(now);
        self.maybe_correct(now);

        // push every releasable chunk to the track callbacks, in pts order
        // across the tracks sharing the anchor
        loop {
            let released_audio = self.try_release_chunk(TrackKind::Audio, now);
            let released_video = self.try_release_chunk(TrackKind::Video, now);
            if !released_audio && !released_video {
                break;
            }
        }

        let shared_anchor = self.anchor;
        if let Some(track) = self.audio.as_mut() {
            track.report_stats_track_snapshot(shared_anchor, &self.shared_estimator);
        }
        if let Some(track) = self.video.as_mut() {
            track.report_stats_track_snapshot(shared_anchor, &self.shared_estimator);
        }
    }

    // Same phases as `tick`, but every decision is read from a `StateView`
    // and this function only applies it. The view is an owned snapshot,
    // rebuilt only after something was applied, since that is the only way
    // the state it was built from changes (a reset before a start, audio's
    // start before video's).
    //
    // pub(super) fn tick(&mut self, now: Instant) {
    //     self.drop_closed_tracks();
    //     let mut view = self.view(now);
    //
    //     for kind in [TrackKind::Audio, TrackKind::Video] {
    //         if view.should_reset(kind) {
    //             self.reset_track(kind, view.flush_anchor(kind));
    //             view = self.view(now);
    //         }
    //     }
    //
    //     // audio first, so video's decision can align to it
    //     for kind in [TrackKind::Audio, TrackKind::Video] {
    //         if let Some(decision) = view.start_decision(kind) {
    //             self.start_track(kind, decision, now);
    //             view = self.view(now);
    //         }
    //     }
    //
    //     self.apply_correction(view.correction());
    //
    //     // unchanged: release loop and stats snapshots
    //     loop {
    //         let released_audio = self.try_release_chunk(TrackKind::Audio, now);
    //         let released_video = self.try_release_chunk(TrackKind::Video, now);
    //         if !released_audio && !released_video {
    //             break;
    //         }
    //     }
    //     self.report_stats_snapshots();
    // }
    //
    // fn view(&self, now: Instant) -> StateView {
    //     StateView {
    //         options: self.options,
    //         now,
    //         now_pts: self.sync_point.timestamp_at(now),
    //         estimation: self.shared_estimator.estimate(now),
    //         shared_anchor: self.anchor,
    //         audio: self.audio.as_ref().map(|track| track.view(now)),
    //         video: self.video.as_ref().map(|track| track.view(now)),
    //     }
    // }
    //
    // fn start_track(&mut self, kind: TrackKind, decision: StartDecision, now: Instant) {
    //     let anchor = match decision {
    //         StartDecision::Shared(anchor) => {
    //             // adopt when it exists, establish otherwise
    //             self.anchor.get_or_insert(SharedAnchor::new(anchor));
    //             track.set_start(StartState::StartedShared);
    //             anchor
    //         }
    //         StartDecision::Track(anchor) => {
    //             track.set_start(StartState::StartedTrack { target: anchor, current: anchor, .. });
    //             anchor
    //         }
    //     };
    //     debug!(?kind, ?decision, "Live sync: track started");
    //     track.release_backlog(anchor, now);   // today's tail of maybe_start
    // }

    /// Give up on live edge detection; every track releases what it buffered
    /// through its callback. Shared live edge state stays intact.
    pub(super) fn flush(&mut self) {
        debug!("Live sync: flush");
        let now = Instant::now();
        if let Some(track) = self.audio.as_mut() {
            track.reset(now, self.anchor);
        }
        if let Some(track) = self.video.as_mut() {
            track.reset(now, self.anchor);
        }
    }

    /// Fails once the sink of the track is gone, so the input can stop
    /// producing for it.
    pub(super) fn write_chunk(
        &mut self,
        kind: TrackKind,
        chunk: B::Chunk,
    ) -> Result<(), TrackClosedError> {
        let now = Instant::now();
        self.reset_on_discontinuity(kind, now, chunk.pts());

        let track = match kind {
            TrackKind::Audio => self.audio.as_mut(),
            TrackKind::Video => self.video.as_mut(),
        };
        let Some(track) = track else {
            return Err(TrackClosedError);
        };

        track.last_written = Some((chunk.pts(), now));
        track.report_stats_chunk_received(self.anchor, &chunk);

        // both estimators observe for the whole lifetime of the input
        track.estimator.observe(now, chunk.pts());
        self.shared_estimator.observe(now, chunk.pts());
        trace!(
            ?kind,
            pts=?chunk.pts(),
            now_pts=?self.sync_point.timestamp_at(now),
            "Live sync: observed chunk"
        );
        track.buffer.write(chunk);

        self.tick(now);
        Ok(())
    }

    fn try_release_chunk(&mut self, kind: TrackKind, now: Instant) -> bool {
        if self.should_wait_for_other_track(kind, now) {
            return false;
        }

        let track = match kind {
            TrackKind::Audio => self.audio.as_mut(),
            TrackKind::Video => self.video.as_mut(),
        };
        let Some(track) = track else {
            return false;
        };

        match &mut track.start {
            StartState::WaitingForStart => false,
            StartState::StartedShared => {
                let Some(anchor) = self.anchor.as_mut() else {
                    return false;
                };
                let Some(chunk) = track.buffer.try_read() else {
                    return false;
                };

                let last_pts = anchor.last_released_pts.unwrap_or(chunk.pts());
                anchor.last_released_pts = Some(Timestamp::max(last_pts, chunk.pts()));

                let max_shift = self.options.buffering_strategy.max_shift(
                    anchor.current,
                    anchor.target,
                    Timestamp::max(Timestamp::ZERO, chunk.pts() - last_pts),
                );
                anchor.current.nudge_towards(anchor.target, max_shift);

                track.release_chunk(chunk, anchor.current);
                true
            }
            StartState::StartedTrack {
                target_anchor,
                current_anchor,
                last_released_pts,
            } => {
                let Some(chunk) = track.buffer.try_read() else {
                    return false;
                };

                let last_pts = last_released_pts.unwrap_or(chunk.pts());
                *last_released_pts = Some(Timestamp::max(last_pts, chunk.pts()));

                let max_shift = self.options.buffering_strategy.max_shift(
                    *current_anchor,
                    *target_anchor,
                    Timestamp::max(Timestamp::ZERO, chunk.pts() - last_pts),
                );
                current_anchor.nudge_towards(*target_anchor, max_shift);

                let anchor = *current_anchor;
                track.release_chunk(chunk, anchor);
                true
            }
        }
    }

    fn drop_closed_tracks(&mut self) {
        if let Some(audio) = &self.audio
            && audio.sink.is_closed()
        {
            debug!("Live sync: audio track sink closed, dropping track");
            self.audio = None;
        }
        if let Some(video) = &self.video
            && video.sink.is_closed()
        {
            debug!("Live sync: video track sink closed, dropping track");
            self.video = None;
        }
    }

    fn maybe_reset(&mut self, now: Instant) {
        if let Some(track) = self.audio.as_mut() {
            track.maybe_reset(now, self.anchor);
        }
        if let Some(track) = self.video.as_mut() {
            track.maybe_reset(now, self.anchor);
        }
    }

    fn maybe_start(&mut self, now: Instant) {
        let shared_timeline = self.tracks_share_timeline(now, Duration::from_secs(10));

        if let Some(track) = self.audio.as_mut() {
            track.maybe_start(
                now,
                &self.shared_estimator,
                &mut self.anchor,
                shared_timeline,
                None,
            );
        }
        // video follows audio, so the audio reference is taken after audio
        // had a chance to start in this tick
        let leader = self.audio_edge_reference(now);
        if let Some(track) = self.video.as_mut() {
            track.maybe_start(
                now,
                &self.shared_estimator,
                &mut self.anchor,
                shared_timeline,
                leader,
            );
        }
    }

    /// Live edge of the started audio track and the anchor it presents it
    /// with; `None` while audio is missing, waiting or its edge is not
    /// stable yet.
    fn audio_edge_reference(&self, now: Instant) -> Option<EdgeReference> {
        let audio = self.audio.as_ref()?;
        let anchor = match audio.start {
            StartState::WaitingForStart => return None,
            StartState::StartedShared => self.anchor?.target,
            StartState::StartedTrack { target_anchor, .. } => target_anchor,
        };
        let upper_bound = audio.estimator.estimate(now)?.upper_bound;
        match upper_bound.stable {
            true => Some(EdgeReference {
                anchor,
                edge_pts: upper_bound.pts,
            }),
            false => None,
        }
    }

    /// Heuristic that decides if all tracks are on the same timeline. Live
    /// edges closer than `threshold` are treated as the same timeline. `None`
    /// when there is not enough information to decide either way.
    fn tracks_share_timeline(&self, now: Instant, threshold: Duration) -> Option<bool> {
        let audio = self.audio.as_ref().and_then(|a| a.estimator.estimate(now));
        let video = self.video.as_ref().and_then(|v| v.estimator.estimate(now));
        let (Some(audio), Some(video)) = (audio, video) else {
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

    fn maybe_correct(&mut self, now: Instant) {
        let now_pts = self.sync_point.timestamp_at(now);

        if let (Some(anchor), Some(estimation)) =
            (self.anchor.as_mut(), self.shared_estimator.estimate(now))
        {
            let strategy = self.options.buffering_strategy;
            if !strategy.buffer_in_range(estimation, anchor.current, now_pts) {
                anchor.target = strategy.desired_anchor(&estimation, now_pts);
                trace!(
                    target_offset = anchor.target.offset_string(),
                    "Live sync: shared anchor out of range, correcting target"
                );
            }
        }

        if !self.any_track_on_shared_anchor()
            && let Some(anchor) = self.anchor.as_mut()
        {
            anchor.current = anchor.target
        }

        // stricter than the difference that splits the tracks on start, so
        // they cannot flap between sharing an anchor and running their own
        let shared_timeline = self.tracks_share_timeline(now, Duration::from_secs(3));

        if let Some(track) = self.audio.as_mut() {
            track.maybe_correct(now, &mut self.anchor, shared_timeline, None);
        }
        let leader = self.audio_edge_reference(now);
        if let Some(track) = self.video.as_mut() {
            track.maybe_correct(now, &mut self.anchor, shared_timeline, leader);
        }
    }

    /// Whether any track is applying the shared anchor.
    fn any_track_on_shared_anchor(&self) -> bool {
        let audio_shared = match self.audio.as_ref() {
            Some(track) => matches!(track.start, StartState::StartedShared),
            None => false,
        };
        let video_shared = match self.video.as_ref() {
            Some(track) => matches!(track.start, StartState::StartedShared),
            None => false,
        };
        audio_shared || video_shared
    }

    // Tracks sharing the anchor have to release their chunks in pts order, so
    // a single anchor can be slewed as chunks are released. Without that
    // ordering it would require a far more complex setup, or the tracks would
    // desync while the anchor converges on its target.
    fn should_wait_for_other_track(&self, kind: TrackKind, now: Instant) -> bool {
        let (track, other) = match kind {
            TrackKind::Audio => (self.audio.as_ref(), self.video.as_ref()),
            TrackKind::Video => (self.video.as_ref(), self.audio.as_ref()),
        };
        let (Some(track), Some(other)) = (track, other) else {
            return false;
        };
        if !matches!(track.start, StartState::StartedShared)
            || !matches!(other.start, StartState::StartedShared)
        {
            return false;
        }
        let (Some(anchor), Some(pts)) = (self.anchor, track.buffer.peek_pts()) else {
            return false;
        };

        let now_pts = self.sync_point.timestamp_at(now);
        if anchor.current.to_output_pts(pts) <= now_pts + MIN_QUEUE_HEADROOM {
            // the chunk is about to miss the queue; release it now instead of
            // waiting for the other track
            return false;
        }
        match other.buffer.peek_pts() {
            Some(other_pts) => other_pts < pts,
            None => true,
        }
    }

    /// Drops the live edge state built for the old timeline when `pts` does
    /// not belong to it anymore.
    fn reset_on_discontinuity(&mut self, kind: TrackKind, now: Instant, pts: Timestamp) {
        let track = match kind {
            TrackKind::Audio => self.audio.as_ref(),
            TrackKind::Video => self.video.as_ref(),
        };
        let Some(track) = track else {
            return;
        };
        if !track.is_discontinuity(now, pts) {
            return;
        }
        debug!(?kind, ?pts, "Live sync: discontinuity detected");

        // Only reset if track had any data since last reset
        if track.estimator.estimate(now).is_some() {
            if let Some(track) = self.audio.as_mut() {
                track.reset(now, self.anchor);
            }
            if let Some(track) = self.video.as_mut() {
                track.reset(now, self.anchor);
            }
            self.shared_estimator = LiveEdgeEstimator::new(
                self.sync_point,
                self.options.stabilization_tolerance,
                self.options.stabilization_period,
            );
            self.anchor = None;
        }

        // After the resets, so flushed old chunks precede the event in the sink.
        let track = match kind {
            TrackKind::Audio => self.audio.as_mut(),
            TrackKind::Video => self.video.as_mut(),
        };
        if let Some(track) = track {
            track.sink.on_event(TrackEvent::Discontinuity);
            track.stats.report_discontinuity();
        }
    }
}

/// State of a single track, owned by [`SharedState`].
struct TrackState<B: LiveSyncBuffer> {
    kind: TrackKind,
    /// Input-wide config, copied so a track can run its own transitions.
    options: LiveSyncOptions,
    /// Instant that output timestamps are measured from.
    sync_point: Instant,
    /// Estimator observing only this track's chunks.
    estimator: LiveEdgeEstimator,
    start: StartState,
    buffer: B,
    /// Receives the chunks this track releases.
    sink: BoxedTrackSink<B::Chunk>,
    /// Output pts the released content ends at. Used to maintain continuity after
    /// reset so it has to survive the reset itself.
    last_released_pts: Option<Timestamp>,
    /// pts of the last written chunk and its arrival time. Deliberately not
    /// cleared on reset, so a discontinuity is detectable right after one.
    last_written: Option<(Timestamp, Instant)>,
    stats: LiveSyncTrackStats,
}

impl<B: LiveSyncBuffer> TrackState<B> {
    fn set_start(&mut self, start: StartState) {
        self.start = start;
        self.stats.report_state_change(&self.start);
    }

    fn report_stats_chunk_received(&self, shared_anchor: Option<SharedAnchor>, chunk: &B::Chunk) {
        self.stats.report_bytes_received(chunk.size());
        let current_anchor = match self.start {
            StartState::WaitingForStart => None,
            StartState::StartedShared => shared_anchor.map(|a| a.current),
            StartState::StartedTrack { current_anchor, .. } => Some(current_anchor),
        };
        if let Some(current) = current_anchor {
            self.stats
                .report_chunk_received(current.to_output_pts(chunk.pts()));
        }
    }

    fn report_stats_track_snapshot(
        &mut self,
        shared_anchor: Option<SharedAnchor>,
        shared_estimator: &LiveEdgeEstimator,
    ) {
        let (anchors, estimator) = match self.start {
            StartState::WaitingForStart => (None, None),
            StartState::StartedShared => (
                shared_anchor.map(|a| (a.current, a.target)),
                Some(shared_estimator),
            ),
            StartState::StartedTrack {
                target_anchor,
                current_anchor,
                ..
            } => (Some((current_anchor, target_anchor)), Some(&self.estimator)),
        };
        self.stats
            .report_state_snapshot(&self.buffer, anchors, estimator);
    }

    /// Mainly detects tracks that stopped sending data, but it can also trigger
    /// on significant network problems.
    fn maybe_reset(&mut self, now: Instant, shared_anchor: Option<SharedAnchor>) {
        if matches!(self.start, StartState::WaitingForStart) {
            return;
        }
        if self.buffer.peek_pts().is_some() {
            return;
        }
        let Some(last_pts) = self.last_released_pts else {
            return;
        };

        // Slightly late track can still recover; reset would cause a gap of at
        // least the stabilization period. 5s late is considered unrecoverable.
        let now_pts = self.sync_point.timestamp_at(now);
        if last_pts + Duration::from_secs(5) > now_pts {
            return;
        }

        debug!(
            kind=?self.kind,
            last_released_pts=?last_pts,
            ?now_pts,
            "Live sync: track stalled, resetting"
        );
        self.reset(now, shared_anchor);
    }

    /// Runs the start decision; called without new chunks too, so time-based
    /// conditions can trigger the start when delivery pauses.
    fn maybe_start(
        &mut self,
        now: Instant,
        shared_estimator: &LiveEdgeEstimator,
        shared_anchor: &mut Option<SharedAnchor>,
        shared_timeline: Option<bool>,
        leader: Option<EdgeReference>,
    ) {
        if !matches!(self.start, StartState::WaitingForStart) {
            return;
        }

        let now_pts = self.sync_point.timestamp_at(now);
        let Some(shared_estimation) = shared_estimator.estimate(now) else {
            return;
        };
        let Some(track_estimation) = self.estimator.estimate(now) else {
            return;
        };

        let both_stable =
            track_estimation.upper_bound.stable && shared_estimation.upper_bound.stable;
        let waiting_too_long = track_estimation.delivery.observed_for >= self.options.max_wait;
        if !both_stable && !waiting_too_long {
            return;
        }

        // undecided tracks start on the shared timeline
        let anchor = match shared_timeline.unwrap_or(true) {
            true => match shared_anchor {
                Some(anchor) => {
                    debug!(
                        kind=?self.kind,
                        offset=anchor.current.offset_string(),
                        "Live sync: track started, adopting shared anchor"
                    );
                    self.set_start(StartState::StartedShared);
                    anchor.current
                }
                None => {
                    let anchor = self
                        .options
                        .buffering_strategy
                        .desired_anchor(&shared_estimation, now_pts);
                    debug!(
                        kind=?self.kind,
                        offset=anchor.offset_string(),
                        buffered=?self.buffered_duration(),
                        ?shared_estimation,
                        "Live sync: track started, establishing shared anchor"
                    );
                    *shared_anchor = Some(SharedAnchor {
                        current: anchor,
                        target: anchor,
                        last_released_pts: None,
                    });
                    self.set_start(StartState::StartedShared);
                    anchor
                }
            },
            false => {
                // aligned to the leader when there is one; converging on it
                // later by slewing would take far longer
                let anchor = match leader {
                    Some(leader) => leader.aligned_anchor(track_estimation.upper_bound.pts),
                    None => self
                        .options
                        .buffering_strategy
                        .desired_anchor(&track_estimation, now_pts),
                };
                debug!(
                    kind=?self.kind,
                    offset=anchor.offset_string(),
                    buffered=?self.buffered_duration(),
                    following=leader.is_some(),
                    ?track_estimation,
                    "Live sync: track started with its own anchor"
                );
                self.set_start(StartState::StartedTrack {
                    target_anchor: anchor,
                    current_anchor: anchor,
                    last_released_pts: None,
                });
                anchor
            }
        };

        while let Some(mut chunk) = self.buffer.try_read() {
            if anchor.to_output_pts(chunk.pts()) <= now_pts {
                // Decoder should only drop late packets during startup phase.
                // It avoids visible video speedup when queue input drains
                // decoder to reach "on time" position
                chunk.mark_decode_only();
            }
            self.release_chunk(chunk, anchor);
        }
    }

    fn maybe_correct(
        &mut self,
        now: Instant,
        shared_anchor: &mut Option<SharedAnchor>,
        shared_timeline: Option<bool>,
        leader: Option<EdgeReference>,
    ) {
        let StartState::StartedTrack {
            target_anchor,
            current_anchor,
            ..
        } = &mut self.start
        else {
            return;
        };

        let Some(track_estimation) = self.estimator.estimate(now) else {
            return;
        };

        // The verdict that this track runs its own timeline can turn out to be wrong.
        // If the tracks turn out to be close then try to converge on shared target;
        // undecided tracks stay where they are.
        if shared_timeline == Some(true) {
            let Some(shared_anchor) = shared_anchor else {
                debug!(
                    kind=?self.kind,
                    offset=current_anchor.offset_string(),
                    "Live sync: track anchor promoted to shared anchor"
                );
                *shared_anchor = Some(SharedAnchor {
                    current: *current_anchor,
                    target: *current_anchor,
                    last_released_pts: None,
                });
                self.start = StartState::StartedShared;
                return;
            };
            // We no longer update target anchor based on estimator, but track
            // estimator can still break this cycle if it diverges.
            *target_anchor = shared_anchor.current;
            let anchor_distance = shared_anchor.current.distance_to(*current_anchor);
            if anchor_distance < Timestamp::from_millis(50) {
                debug!(
                    kind=?self.kind,
                    offset=shared_anchor.current.offset_string(),
                    "Live sync: track converged, switching to shared anchor"
                );
                self.set_start(StartState::StartedShared);
            }
            return;
        }

        // With a leader the buffer size is not this track's decision; its
        // live edge is kept where the leader presents its own.
        if let Some(leader) = leader {
            if !track_estimation.upper_bound.stable {
                return;
            }
            let aligned = leader.aligned_anchor(track_estimation.upper_bound.pts);
            if aligned.distance_to(*target_anchor) > FOLLOW_TOLERANCE {
                *target_anchor = aligned;
                trace!(
                    kind=?self.kind,
                    target_offset=target_anchor.offset_string(),
                    "Live sync: track anchor drifted from the leader, correcting target"
                );
            }
            return;
        }

        let strategy = self.options.buffering_strategy;
        let now_pts = self.sync_point.timestamp_at(now);
        if !strategy.buffer_in_range(track_estimation, *current_anchor, now_pts) {
            *target_anchor = strategy.desired_anchor(&track_estimation, now_pts);
            trace!(
                kind=?self.kind,
                target_offset=target_anchor.offset_string(),
                "Live sync: track anchor out of range, correcting target"
            );
        }
    }

    /// pts span of the buffered content.
    fn buffered_duration(&self) -> Timestamp {
        let min = self.buffer.pts_values().min();
        let max = self.buffer.pts_values().max();
        match (min, max) {
            (Some(min), Some(max)) => max - min,
            _ => Timestamp::ZERO,
        }
    }

    /// Pushes a chunk out, with its timestamps mapped onto the output
    /// timeline by `anchor`.
    fn release_chunk(&mut self, mut chunk: B::Chunk, anchor: TimestampAnchor) {
        let input_pts = chunk.pts();
        chunk.apply_anchor(anchor);
        let output_pts = chunk.pts();

        trace!(
            kind=?self.kind,
            ?input_pts,
            ?output_pts,
            lead=?(output_pts - self.sync_point.timestamp_now()),
            "Live sync: releasing chunk"
        );
        self.stats.report_chunk_released(output_pts);
        self.last_released_pts = Some(match self.last_released_pts {
            Some(previous) => Timestamp::max(previous, chunk.pts()),
            None => chunk.pts(),
        });
        self.sink.on_event(TrackEvent::Chunk(chunk));
    }

    /// Gives up on the live edge: releases everything buffered with the
    /// mapping in use and goes back to waiting for a start decision.
    fn reset(&mut self, now: Instant, shared_anchor: Option<SharedAnchor>) {
        let anchor = self.best_effort_anchor(now, shared_anchor);
        debug!(
            kind=?self.kind,
            start=?self.start,
            offset=anchor.map(|anchor| anchor.offset_string()),
            buffered=self.buffer.pts_values().count(),
            "Live sync: resetting track"
        );

        self.estimator = LiveEdgeEstimator::new(
            self.sync_point,
            self.options.stabilization_tolerance,
            self.options.stabilization_period,
        );
        self.set_start(StartState::WaitingForStart);

        if let Some(anchor) = anchor {
            // release everything buffered with the old mapping
            while let Some(chunk) = self.buffer.read() {
                self.release_chunk(chunk, anchor);
            }
        }
    }

    /// Whether `pts` belongs to a different timeline than the one this track
    /// has been observing.
    fn is_discontinuity(&self, now: Instant, pts: Timestamp) -> bool {
        let Some((last_pts, arrived_at)) = self.last_written else {
            return false;
        };
        // pts expected if the stream kept producing in real time since the
        // newest received chunk
        let expected_pts = last_pts + now.saturating_duration_since(arrived_at);
        let forward_jump = pts > expected_pts + DISCONTINUITY_THRESHOLD;
        let backward_jump = pts + DISCONTINUITY_THRESHOLD < last_pts;
        forward_jump || backward_jump
    }

    /// Mapping the buffered content can be released with: the one the track
    /// is applying when it started, otherwise a best effort one. `None` when
    /// there is nothing to build it from.
    fn best_effort_anchor(
        &self,
        now: Instant,
        shared_anchor: Option<SharedAnchor>,
    ) -> Option<TimestampAnchor> {
        let started_anchor = match self.start {
            StartState::WaitingForStart => None,
            StartState::StartedShared => shared_anchor.map(|anchor| anchor.current),
            StartState::StartedTrack { current_anchor, .. } => Some(current_anchor),
        };
        if let Some(anchor) = started_anchor {
            return Some(anchor);
        }

        let now_pts = self.sync_point.timestamp_at(now);

        // Try to maintain continuity if there is still time to reach queue:
        // the oldest buffered chunk picks the timeline up where the released
        // content ended.
        if let Some(last_pts) = self.last_released_pts
            && last_pts > now_pts + MIN_QUEUE_HEADROOM
        {
            return Some(TimestampAnchor {
                input_pts: self.buffer.peek_pts()?,
                output_pts: last_pts,
            });
        }

        // Nothing to continue from, so the newest buffered chunk stands in for the live edge.
        // As result effective buffer is exactly desired buffer.
        Some(TimestampAnchor {
            input_pts: self.buffer.pts_values().max()?, // most recently observed
            output_pts: now_pts + self.options.buffering_strategy.desired_buffer(),
        })
    }
}

#[derive(Debug, Clone)]
pub(super) enum StartState {
    /// Written chunks are buffered and not released yet. On each write and on
    /// each tick we are checking if both edge estimators are ready.
    ///
    /// If shared and track estimator diverge too much the track starts with
    /// its own mapping ([`StartedTrack`](Self::StartedTrack)), otherwise it
    /// aligns to the shared anchor ([`StartedShared`](Self::StartedShared)).
    WaitingForStart,
    /// Aligned to the shared live edge; chunks are mapped with the input-wide
    /// [`SharedAnchor`].
    StartedShared,
    /// The track's timestamps are unrelated to the other track, so it keeps a
    /// private mapping derived from its own estimator.
    StartedTrack {
        target_anchor: TimestampAnchor,
        current_anchor: TimestampAnchor,
        /// Largest pts released so far; sizes the slew steps.
        last_released_pts: Option<Timestamp>,
    },
}
