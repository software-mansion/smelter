use std::time::{Duration, Instant};

use super::{LiveSyncOptions, buffer::LiveSyncBuffer, edge_estimator::LiveEdgeEstimator};
use crate::pipeline::utils::input_sync::{
    BoxedTrackSink, InputSyncItem, TimestampAnchor, TrackClosedError, TrackEvent, TrackKind,
};

/// pts jump (in either direction) treated as a discontinuity of the input
/// timeline; the old edge estimate does not describe the new timeline.
const DISCONTINUITY_THRESHOLD: Duration = Duration::from_secs(10);

/// Lead over the playback position content needs to still reach the queue.
const MIN_QUEUE_HEADROOM: Duration = Duration::from_millis(80);

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
}

/// Anchor of the tracks aligned to the shared edge. Corrections move
/// `target`; `current` slews towards it in small steps as chunks are read.
#[derive(Debug, Clone, Copy)]
struct SharedAnchor {
    /// Mapping applied to every chunk read right now.
    current: TimestampAnchor,
    /// Mapping the corrections aim for.
    target: TimestampAnchor,
    /// Largest pts released so far by any track applying this anchor.
    ///
    /// It is used to ensure synchronize releasing chunks in PTS order.
    /// Value is reset (together with entire anchor) on discontinuity.
    last_released_pts: Option<Duration>,
}

impl<B: LiveSyncBuffer> SharedState<B> {
    pub(super) fn new(options: LiveSyncOptions, sync_point: Instant) -> Self {
        Self {
            options,
            sync_point,
            shared_estimator: LiveEdgeEstimator::new(sync_point, options.stabilization_tolerance),
            anchor: None,
            audio: None,
            video: None,
        }
    }

    pub(super) fn add_track(&mut self, kind: TrackKind, sink: BoxedTrackSink<B::Chunk>) {
        let track = TrackState {
            options: self.options,
            sync_point: self.sync_point,
            estimator: LiveEdgeEstimator::new(
                self.sync_point,
                self.options.stabilization_tolerance,
            ),
            start: StartState::WaitingForStart,
            buffer: B::default(),
            sink,
            last_released_pts: None,
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
        if let Some(track) = self.audio.as_mut() {
            track.maybe_reset(now, self.anchor);
            track.maybe_start(now, &self.shared_estimator, &mut self.anchor);
        }
        if let Some(track) = self.video.as_mut() {
            track.maybe_reset(now, self.anchor);
            track.maybe_start(now, &self.shared_estimator, &mut self.anchor);
        }
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
    }

    /// Give up on live edge detection; every track releases what it buffered
    /// through its callback. Shared live edge state stays intact.
    pub(super) fn flush(&mut self) {
        let now = Instant::now();
        if let Some(track) = self.audio.as_mut() {
            track.reset(now, self.anchor);
        }
        if let Some(track) = self.video.as_mut() {
            track.reset(now, self.anchor);
        }
    }

    /// Drops the tracks whose sink is gone, so nothing is released into the
    /// void and the next write to them fails.
    fn drop_closed_tracks(&mut self) {
        if self
            .audio
            .as_ref()
            .is_some_and(|track| track.sink.is_closed())
        {
            self.audio = None;
        }
        if self
            .video
            .as_ref()
            .is_some_and(|track| track.sink.is_closed())
        {
            self.video = None;
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

        // both estimators observe for the whole lifetime of the input
        track.estimator.observe(now, chunk.pts());
        self.shared_estimator.observe(now, chunk.pts());
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
                anchor.last_released_pts = Some(Duration::max(last_pts, chunk.pts()));

                let max_shift = self.options.buffering_strategy.max_shift(
                    anchor.current,
                    anchor.target,
                    chunk.pts().saturating_sub(last_pts),
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
                *last_released_pts = Some(Duration::max(last_pts, chunk.pts()));

                let max_shift = self.options.buffering_strategy.max_shift(
                    *current_anchor,
                    *target_anchor,
                    chunk.pts().saturating_sub(last_pts),
                );
                current_anchor.nudge_towards(*target_anchor, max_shift);

                let anchor = *current_anchor;
                track.release_chunk(chunk, anchor);
                true
            }
        }
    }

    fn maybe_correct(&mut self, now: Instant) {
        let now_pts = now.saturating_duration_since(self.sync_point);

        if let (Some(anchor), Some(estimation)) =
            (self.anchor.as_mut(), self.shared_estimator.estimate(now))
        {
            let strategy = self.options.buffering_strategy;
            if strategy.needs_reanchor(estimation, anchor.current, now_pts) {
                anchor.target = strategy.desired_anchor(estimation, now_pts);
            }
        }

        if !self.any_track_on_shared_anchor()
            && let Some(anchor) = self.anchor.as_mut()
        {
            anchor.current = anchor.target
        }

        if let Some(track) = self.audio.as_mut() {
            track.maybe_correct(now, &self.shared_estimator, &mut self.anchor);
        }
        if let Some(track) = self.video.as_mut() {
            track.maybe_correct(now, &self.shared_estimator, &mut self.anchor);
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

    // Releasing chunks from buffer needs to be synchronized between tracks, so we can
    // use single anchor to slightly shift the chunks. Otherwise it requires far more complex
    // setup, or it would cause a/v desync while 2 tracks converge on target anchor
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

        let now_pts = now.saturating_duration_since(self.sync_point);
        if anchor.current.to_output_pts(pts) <= now_pts + MIN_QUEUE_HEADROOM {
            // check if with current anchor we have still time to reach queue. If yes, then
            // we can still wait a bit
            return false;
        }
        match other.buffer.peek_pts() {
            Some(other_pts) => other_pts < pts,
            None => true,
        }
    }

    /// Drops the live edge state built for the old timeline when `pts` does
    /// not belong to it anymore.
    fn reset_on_discontinuity(&mut self, kind: TrackKind, now: Instant, pts: Duration) {
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

        if let Some(track) = self.audio.as_mut() {
            track.reset(now, self.anchor);
        }
        if let Some(track) = self.video.as_mut() {
            track.reset(now, self.anchor);
        }

        // Even though only one track had a gap we restart both of them, but
        // only one should produce discontinuity event.
        //
        // Especially, important for HLS where discontinuity on both tracks
        // can be slightly of (e.g. by a packet) which will cause 2 resets,
        // but only one discontinuity should be send downstream (e.g. to decoder)
        let track = match kind {
            TrackKind::Audio => self.audio.as_mut(),
            TrackKind::Video => self.video.as_mut(),
        };
        if let Some(track) = track {
            track.sink.on_event(TrackEvent::Discontinuity);
        }

        self.shared_estimator =
            LiveEdgeEstimator::new(self.sync_point, self.options.stabilization_tolerance);
        self.anchor = None;
    }
}

/// State of a single track, owned by [`SharedState`].
struct TrackState<B: LiveSyncBuffer> {
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
    last_released_pts: Option<Duration>,
}

impl<B: LiveSyncBuffer> TrackState<B> {
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

        // If it is slightly late it can still recover, and unnecessary reset
        // would cause gap of at least stabilization period. If it is 5s late we
        // consider that unrecoverable.
        //
        // We could pick lower number, but the only consequence of not resetting it
        // earlier is that second track will be producing chunks in the last moment.
        let now_pts = now.saturating_duration_since(self.sync_point);
        if last_pts + Duration::from_secs(5) > now_pts {
            return;
        }

        self.reset(now, shared_anchor);
    }

    /// Runs the start decision; called without new chunks too, so time-based
    /// conditions can trigger the start when delivery pauses.
    fn maybe_start(
        &mut self,
        now: Instant,
        shared_estimator: &LiveEdgeEstimator,
        shared_anchor: &mut Option<SharedAnchor>,
    ) {
        if !matches!(self.start, StartState::WaitingForStart) {
            return;
        }
        let now_pts = now.saturating_duration_since(self.sync_point);

        let Some(source) = self.resolve_should_start(now, shared_estimator) else {
            return;
        };

        match source {
            EdgeSource::Shared => {
                if shared_anchor.is_some() {
                    self.start = StartState::StartedShared;
                    return;
                }
                let Some(estimation) = shared_estimator.estimate(now) else {
                    return;
                };
                let anchor = self
                    .options
                    .buffering_strategy
                    .desired_anchor(estimation, now_pts);
                *shared_anchor = Some(SharedAnchor {
                    current: anchor,
                    target: anchor,
                    last_released_pts: None,
                });
                self.start = StartState::StartedShared;
            }
            EdgeSource::Track => {
                let Some(estimation) = self.estimator.estimate(now) else {
                    return;
                };
                let anchor = self
                    .options
                    .buffering_strategy
                    .desired_anchor(estimation, now_pts);
                self.start = StartState::StartedTrack {
                    target_anchor: anchor,
                    current_anchor: anchor,
                    last_released_pts: None,
                };
            }
        }
    }

    fn maybe_correct(
        &mut self,
        now: Instant,
        shared_estimator: &LiveEdgeEstimator,
        shared_anchor: &mut Option<SharedAnchor>,
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
        // If both estimators start to be relatively close then try to converge on shared
        // target.
        if let (Some(shared_anchor), Some(shared_estimation)) =
            (*shared_anchor, shared_estimator.estimate(now))
        {
            let track_diff = Duration::abs_diff(
                track_estimation.upper_bound.pts,
                shared_estimation.upper_bound.pts,
            );

            // stricter than the difference that splits the tracks, so they
            // cannot flap between sharing an anchor and running their own
            if track_diff < Duration::from_secs(3) {
                // We no longer update target anchor based on estimator, but track
                // estimator can still break this cycle if it diverges.
                *target_anchor = shared_anchor.current;
                let anchor_distance = shared_anchor.current.distance_to(*current_anchor);
                if anchor_distance < Duration::from_millis(50) {
                    self.start = StartState::StartedShared
                }
                return;
            }
        }

        let strategy = self.options.buffering_strategy;
        let now_pts = now.saturating_duration_since(self.sync_point);
        if strategy.needs_reanchor(track_estimation, *current_anchor, now_pts) {
            *target_anchor = strategy.desired_anchor(track_estimation, now_pts);
        }
    }

    /// Pushes a chunk out, with its timestamps mapped onto the output
    /// timeline by `anchor`.
    fn release_chunk(&mut self, mut chunk: B::Chunk, anchor: TimestampAnchor) {
        chunk.apply_anchor(anchor);
        self.last_released_pts = Some(match self.last_released_pts {
            Some(previous) => Duration::max(previous, chunk.pts()),
            None => chunk.pts(),
        });
        self.sink.on_event(TrackEvent::Chunk(chunk));
    }

    /// Gives up on the live edge: releases everything buffered with the
    /// mapping in use and goes back to waiting for a start decision.
    fn reset(&mut self, now: Instant, shared_anchor: Option<SharedAnchor>) {
        let anchor = self.best_effort_anchor(now, shared_anchor);

        self.estimator =
            LiveEdgeEstimator::new(self.sync_point, self.options.stabilization_tolerance);
        self.start = StartState::WaitingForStart;

        if let Some(anchor) = anchor {
            // release everything buffered with the old mapping
            while let Some(chunk) = self.buffer.read() {
                self.release_chunk(chunk, anchor);
            }
        }
    }

    /// Whether `pts` belongs to a different timeline than the one this track
    /// has been observing.
    fn is_discontinuity(&self, now: Instant, pts: Duration) -> bool {
        let Some(estimation) = self.estimator.estimate(now) else {
            return false;
        };
        let delivery = estimation.delivery;
        // pts expected if the stream kept producing in real time since the
        // newest received chunk
        let expected_pts = delivery.last_pts + delivery.since_last_arrival;
        let forward_jump = pts > expected_pts + DISCONTINUITY_THRESHOLD;
        let backward_jump = pts + DISCONTINUITY_THRESHOLD < delivery.last_pts;
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

        let now_pts = now.saturating_duration_since(self.sync_point);

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

    /// Which live edge estimate this track should start with, or `None` if it
    /// should keep waiting.
    fn resolve_should_start(
        &self,
        now: Instant,
        shared_estimator: &LiveEdgeEstimator,
    ) -> Option<EdgeSource> {
        let shared = shared_estimator.estimate(now)?;
        let track = self.estimator.estimate(now)?;

        let track_stable = track.upper_bound.stable_for > self.options.stabilization_period;
        let shared_stable = shared.upper_bound.stable_for > self.options.stabilization_period;
        // measured on this track, so a track that starts (or resumes)
        // delivering later still gets the full stabilization window
        let waiting_too_long = track.delivery.observed_for >= self.options.max_wait;

        if !(track_stable && shared_stable) && !waiting_too_long {
            return None;
        }

        // a difference this large cannot come from sender misalignment, chunk
        // sizes or delivery lag, so the timestamps are counted from another origin
        let tracks_diff = Duration::abs_diff(track.upper_bound.pts, shared.upper_bound.pts);
        match tracks_diff < Duration::from_secs(10) {
            true => Some(EdgeSource::Shared),
            false => Some(EdgeSource::Track),
        }
    }
}

#[derive(Debug, Clone)]
enum StartState {
    /// Written chunks are buffered and never returned. On each write and on
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
        last_released_pts: Option<Duration>,
    },
}

/// Which live edge estimate a track aligns to when it starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EdgeSource {
    Shared,
    Track,
}
