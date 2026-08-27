use std::time::{Duration, Instant};

use tracing::{debug, trace};

use super::{LiveSyncOptions, buffer::LiveSyncBuffer, edge_estimator::LiveEdgeEstimator};
use crate::{
    pipeline::utils::input_sync::{
        BoxedTrackSink, InputSyncItem, TimestampAnchor, TrackClosedError, TrackEvent, TrackKind,
    },
    utils::live_sync::edge_estimator::EdgeEstimate,
};

/// pts jump (in either direction) treated as a discontinuity of the input
/// timeline; the old edge estimate does not describe the new timeline.
const DISCONTINUITY_THRESHOLD: Duration = Duration::from_secs(10);

/// Lead over the playback position content needs to still reach the queue.
const MIN_QUEUE_HEADROOM: Duration = Duration::from_millis(100);

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
        debug!(?kind, "Live sync: adding track");
        let track = TrackState {
            kind,
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
    }

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

        // both estimators observe for the whole lifetime of the input
        track.estimator.observe(now, chunk.pts());
        self.shared_estimator.observe(now, chunk.pts());
        trace!(
            ?kind,
            pts=?chunk.pts(),
            now_pts=?now.saturating_duration_since(self.sync_point),
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
        let shared_timeline = self.tracks_share_timeline(now);

        if let Some(track) = self.audio.as_mut() {
            track.maybe_start(
                now,
                &self.shared_estimator,
                &mut self.anchor,
                shared_timeline,
            );
        }
        if let Some(track) = self.video.as_mut() {
            track.maybe_start(
                now,
                &self.shared_estimator,
                &mut self.anchor,
                shared_timeline,
            );
        }
    }

    /// Heuristic that decides if all tracks are on the same timeline
    fn tracks_share_timeline(&self, now: Instant) -> bool {
        let audio = self.audio.as_ref().and_then(|a| a.estimator.estimate(now));
        let video = self.video.as_ref().and_then(|v| v.estimator.estimate(now));
        let (Some(audio), Some(video)) = (audio, video) else {
            return true;
        };
        let (audio, video) = (audio.upper_bound, video.upper_bound);

        let diff = Duration::abs_diff(audio.pts, video.pts);
        // If diff is that large we ignore stability, timelines have to
        // be diverged
        if diff >= Duration::from_secs(120) {
            return false;
        }

        // If diff is over 10 second we check stability too before deciding
        if diff < Duration::from_secs(10) {
            return true;
        }

        let stabilization_period = self.options.stabilization_period;
        let audio_stable = audio.stable_for > stabilization_period;
        let video_stable = video.stable_for > stabilization_period;
        match (audio_stable, video_stable) {
            (true, true) => false,
            // unstable track behind the stable one: could be backlog
            (true, false) => video.pts < audio.pts,
            (false, true) => audio.pts < video.pts,
            (false, false) => true,
        }
    }

    fn maybe_correct(&mut self, now: Instant) {
        let now_pts = now.saturating_duration_since(self.sync_point);

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
        debug!(
            ?kind,
            ?pts,
            "Live sync: discontinuity detected, resetting input"
        );

        if let Some(track) = self.audio.as_mut() {
            track.reset(now, self.anchor);
        }
        if let Some(track) = self.video.as_mut() {
            track.reset(now, self.anchor);
        }

        // Both tracks are reset, but only one should emit a discontinuity event.
        // In HLS both tracks can detect the gap slightly off (e.g. by a packet).
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

        // Slightly late track can still recover; reset would cause a gap of at
        // least the stabilization period. 5s late is considered unrecoverable.
        let now_pts = now.saturating_duration_since(self.sync_point);
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
        shared_timeline: bool,
    ) {
        if !matches!(self.start, StartState::WaitingForStart) {
            return;
        }

        let now_pts = now.saturating_duration_since(self.sync_point);
        let Some(shared_estimation) = shared_estimator.estimate(now) else {
            return;
        };
        if !self.resolve_should_start(now, &shared_estimation) {
            return;
        }

        match shared_timeline {
            true => {
                if let Some(anchor) = shared_anchor {
                    debug!(
                        kind=?self.kind,
                        offset=anchor.current.offset_string(),
                        "Live sync: track started, adopting shared anchor"
                    );
                    self.start = StartState::StartedShared;
                    return;
                }
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
                self.start = StartState::StartedShared;
            }
            false => {
                let Some(estimation) = self.estimator.estimate(now) else {
                    return;
                };
                let anchor = self
                    .options
                    .buffering_strategy
                    .desired_anchor(&estimation, now_pts);
                debug!(
                    kind=?self.kind,
                    offset=anchor.offset_string(),
                    buffered=?self.buffered_duration(),
                    ?estimation,
                    "Live sync: track started with its own anchor"
                );
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
        if let Some(shared_estimation) = shared_estimator.estimate(now) {
            let upper_diff = Duration::abs_diff(
                track_estimation.upper_bound.pts,
                shared_estimation.upper_bound.pts,
            );

            let lower_diff = Duration::abs_diff(
                track_estimation.lower_bound.pts,
                shared_estimation.lower_bound.pts,
            );

            // stricter than the difference that splits the tracks, so they
            // cannot flap between sharing an anchor and running their own
            if upper_diff < Duration::from_secs(3) && lower_diff < Duration::from_secs(3) {
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
                if anchor_distance < Duration::from_millis(50) {
                    debug!(
                        kind=?self.kind,
                        offset=shared_anchor.current.offset_string(),
                        "Live sync: track converged, switching to shared anchor"
                    );
                    self.start = StartState::StartedShared
                }
                return;
            }
        }

        let strategy = self.options.buffering_strategy;
        let now_pts = now.saturating_duration_since(self.sync_point);
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
    fn buffered_duration(&self) -> Duration {
        let min = self.buffer.pts_values().min();
        let max = self.buffer.pts_values().max();
        match (min, max) {
            (Some(min), Some(max)) => max - min,
            _ => Duration::ZERO,
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
            lead=?chunk.pts().checked_sub(self.sync_point.elapsed()),
            "Live sync: releasing chunk"
        );
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
        debug!(
            kind=?self.kind,
            start=?self.start,
            offset=anchor.map(|anchor| anchor.offset_string()),
            buffered=self.buffer.pts_values().count(),
            "Live sync: resetting track"
        );

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
    fn resolve_should_start(&self, now: Instant, shared: &EdgeEstimate) -> bool {
        let Some(track) = self.estimator.estimate(now) else {
            return false;
        };

        let track_stable = track.upper_bound.stable_for > self.options.stabilization_period;
        let shared_stable = shared.upper_bound.stable_for > self.options.stabilization_period;
        let both_stable = track_stable && shared_stable;
        // measured on this track, so a track that starts (or resumes)
        // delivering later still gets the full stabilization window
        let waiting_too_long = track.delivery.observed_for >= self.options.max_wait;

        if !both_stable && !waiting_too_long {
            return false;
        }

        return true;
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
