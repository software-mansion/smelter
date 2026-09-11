use std::time::{Duration, Instant};

use tracing::{debug, trace};

use super::{
    LiveSyncOptions,
    buffer::LiveSyncBuffer,
    edge_estimator::LiveEdgeEstimator,
    mode::{Mode, TrackState},
    stats::LiveSyncTrackStats,
    view::{StateView, TrackView},
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

/// The whole mutable state of an input, cross-track and per-track, kept behind one mutex;
/// [`LiveSync`] and [`LiveSyncTrack`] are thin handles to it. Decisions are read from a
/// [`StateView`] and applied here.
pub(super) struct SharedState<B: LiveSyncBuffer> {
    options: LiveSyncOptions,
    /// Instant that output timestamps are measured from.
    sync_point: Instant,
    /// Estimator observing chunks of all tracks; its edge is defined by the
    /// freshest track.
    shared_estimator: LiveEdgeEstimator,
    /// Anchors the started tracks apply.
    mode: Mode,
    audio: Option<Track<B>>,
    video: Option<Track<B>>,
    stats_sender: InputSyncStatsSender,
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
            mode: Mode::Undecided,
            audio: None,
            video: None,
            stats_sender,
        }
    }

    fn view(&self, now: Instant) -> StateView {
        StateView {
            options: self.options,
            now_pts: self.sync_point.timestamp_at(now),
            mode: self.mode,
            shared_estimation: self.shared_estimator.estimate(now),
            audio: self.audio.as_ref().map(|track| track.view(now)),
            video: self.video.as_ref().map(|track| track.view(now)),
        }
    }

    pub(super) fn add_track(&mut self, kind: TrackKind, sink: BoxedTrackSink<B::Chunk>) {
        debug!(?kind, "Live sync: adding track");
        let track = Track {
            kind,
            sync_point: self.sync_point,
            estimator: LiveEdgeEstimator::new(
                self.sync_point,
                self.options.stabilization_tolerance,
                self.options.stabilization_period,
            ),
            state: TrackState::Waiting,
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

    /// Runs every transition due at `now` (resets, start decisions, corrections) and releases
    /// every releasable chunk. Driven by writes and by the periodic ticker, so time-based
    /// transitions fire during delivery pauses too.
    ///
    /// Every decision is read from a [`StateView`]; the view is rebuilt after something was
    /// applied, since that is the only way the state it was built from changes (a reset before a
    /// start, audio's start before video's).
    pub(super) fn tick(&mut self, now: Instant) {
        self.drop_closed_tracks();
        let mut view = self.view(now);

        for kind in [TrackKind::Audio, TrackKind::Video] {
            if view.should_reset(kind) {
                debug!(?kind, "Live sync: track stalled, resetting");
                self.reset_track(kind, now);
                view = self.view(now);
            }
        }

        // audio first, so video's decision can align to it
        for kind in [TrackKind::Audio, TrackKind::Video] {
            if let Some(mode) = view.start_decision(kind) {
                self.start_track(kind, mode, now);
                view = self.view(now);
            }
        }

        let corrected = view.correct_decision();
        if let Some(change) = self.mode.diff(&corrected) {
            debug!(?change, mode=?corrected, "Live sync: mode corrected");
            self.mode = corrected;
        }

        // push every releasable chunk to the track callbacks, in pts order
        // across the tracks sharing the anchor
        loop {
            let released_audio = self.try_release_chunk(TrackKind::Audio, now);
            let released_video = self.try_release_chunk(TrackKind::Video, now);
            if !released_audio && !released_video {
                break;
            }
        }

        self.report_stats();
    }

    /// Give up on live edge detection; every track releases what it buffered
    /// through its callback. Shared live edge state stays intact.
    pub(super) fn flush(&mut self) {
        debug!("Live sync: flush");
        let now = Instant::now();
        for kind in [TrackKind::Audio, TrackKind::Video] {
            self.reset_track(kind, now);
        }
        self.report_stats();
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
        track.stats.report_bytes_received(chunk.size());
        if let Some(anchor) = self.mode.anchor(kind) {
            let output_pts = anchor.current.to_output_pts(chunk.pts());
            track.stats.report_chunk_received(output_pts);
        }

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

    /// Applies a start decision: `mode` holds the anchor the track starts with; the buffered
    /// backlog is released with it.
    fn start_track(&mut self, kind: TrackKind, mode: Mode, now: Instant) {
        let now_pts = self.sync_point.timestamp_at(now);
        let Some(anchor) = mode.anchor(kind) else {
            return;
        };
        let track = match kind {
            TrackKind::Audio => self.audio.as_mut(),
            TrackKind::Video => self.video.as_mut(),
        };
        let Some(track) = track else {
            return;
        };
        debug!(
            ?kind,
            offset = anchor.current.offset_string(),
            buffered = track.buffer.pts_values().count(),
            ?mode,
            "Live sync: track started"
        );

        track.state = TrackState::Started;

        // Not using regular try_release_chunk, because decoder should only drop late packets
        // during startup phase. It avoids visible video speedup when queue input drains decoder
        // to reach "on time" position
        while let Some(mut chunk) = track.buffer.try_read() {
            // The last video frame is always presented, so the output shows something right away
            let is_last_video_frame = kind == TrackKind::Video && track.buffer.peek_pts().is_none();
            let is_too_late = anchor.current.to_output_pts(chunk.pts()) <= now_pts;
            if !is_last_video_frame && is_too_late {
                chunk.mark_decode_only();
            }
            track.release_chunk(chunk, anchor.current);
        }

        self.mode = mode;
    }

    /// Anchor to release the buffered chunks of `kind` with on reset; `None` when nothing was
    /// observed.
    fn flush_anchor(&self, kind: TrackKind, now: Instant) -> Option<TimestampAnchor> {
        let track = match kind {
            TrackKind::Audio => self.audio.as_ref()?,
            TrackKind::Video => self.video.as_ref()?,
        };
        if track.state == TrackState::Started
            && let Some(anchor) = self.mode.anchor(kind)
        {
            return Some(anchor.current);
        }

        let now_pts = self.sync_point.timestamp_at(now);
        // continue where the released content ended, if that is still ahead of the queue
        if let Some(last_pts) = track.last_released_pts
            && last_pts > now_pts + MIN_QUEUE_HEADROOM
        {
            return Some(TimestampAnchor {
                input_pts: track.buffer.peek_pts()?,
                output_pts: last_pts,
            });
        }

        // otherwise like a fresh start: last observed chunk at the desired buffer
        let (last_written_pts, _) = track.last_written?;
        Some(TimestampAnchor {
            input_pts: last_written_pts,
            output_pts: now_pts + self.options.buffering_strategy.desired_buffer(),
        })
    }

    /// Gives up on the live edge of one track: releases everything it buffered and goes back to
    /// waiting for a start decision. Its own anchor is forgotten; a shared one stays for the
    /// other track.
    fn reset_track(&mut self, kind: TrackKind, now: Instant) {
        let flush_anchor = self.flush_anchor(kind, now);
        self.mode.forget_independent_anchor(kind);
        let track = match kind {
            TrackKind::Audio => self.audio.as_mut(),
            TrackKind::Video => self.video.as_mut(),
        };
        let Some(track) = track else {
            return;
        };
        debug!(
            ?kind,
            state=?track.state,
            offset=flush_anchor.map(|anchor| anchor.offset_string()),
            buffered=track.buffer.pts_values().count(),
            "Live sync: resetting track"
        );
        track.estimator = LiveEdgeEstimator::new(
            track.sync_point,
            self.options.stabilization_tolerance,
            self.options.stabilization_period,
        );
        track.state = TrackState::Waiting;
        if let Some(anchor) = flush_anchor {
            while let Some(chunk) = track.buffer.read() {
                track.release_chunk(chunk, anchor);
            }
        }
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
        if track.state != TrackState::Started {
            return false;
        }
        let Some(anchor) = self.mode.anchor_mut(kind) else {
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

    fn drop_closed_tracks(&mut self) {
        for kind in [TrackKind::Audio, TrackKind::Video] {
            let track = match kind {
                TrackKind::Audio => self.audio.as_ref(),
                TrackKind::Video => self.video.as_ref(),
            };
            if track.is_some_and(|track| track.sink.is_closed()) {
                debug!(?kind, "Live sync: track sink closed, dropping track");
                self.mode.forget_independent_anchor(kind);
                match kind {
                    TrackKind::Audio => self.audio = None,
                    TrackKind::Video => self.video = None,
                }
            }
        }
    }

    // Tracks sharing the anchor have to release their chunks in pts order, so
    // a single anchor can be slewed as chunks are released. Without that
    // ordering it would require a far more complex setup, or the tracks would
    // desync while the anchor converges on its target.
    fn should_wait_for_other_track(&self, kind: TrackKind, now: Instant) -> bool {
        let Mode::Shared(anchor) = self.mode else {
            return false;
        };
        let (track, other) = match kind {
            TrackKind::Audio => (self.audio.as_ref(), self.video.as_ref()),
            TrackKind::Video => (self.video.as_ref(), self.audio.as_ref()),
        };
        let (Some(track), Some(other)) = (track, other) else {
            return false;
        };
        if track.state != TrackState::Started || other.state != TrackState::Started {
            return false;
        }
        let Some(pts) = track.buffer.peek_pts() else {
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
            for kind in [TrackKind::Audio, TrackKind::Video] {
                self.reset_track(kind, now);
            }
            self.shared_estimator = LiveEdgeEstimator::new(
                self.sync_point,
                self.options.stabilization_tolerance,
                self.options.stabilization_period,
            );
            self.mode = Mode::Undecided;
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

    fn report_stats(&mut self) {
        for kind in [TrackKind::Audio, TrackKind::Video] {
            let track = match kind {
                TrackKind::Audio => self.audio.as_mut(),
                TrackKind::Video => self.video.as_mut(),
            };
            let Some(track) = track else {
                continue;
            };
            let anchors = self
                .mode
                .anchor(kind)
                .map(|anchor| (anchor.current, anchor.target));
            track.stats.report_state_change(track.state, self.mode);
            let estimator = match (track.state, self.mode) {
                (TrackState::Waiting, _) => None,
                (TrackState::Started, Mode::Shared(_)) => Some(&self.shared_estimator),
                (TrackState::Started, _) => Some(&track.estimator),
            };
            track
                .stats
                .report_state_snapshot(&track.buffer, anchors, estimator);
        }
    }
}

/// State of a single track, owned by [`SharedState`].
struct Track<B: LiveSyncBuffer> {
    kind: TrackKind,
    /// Instant that output timestamps are measured from.
    sync_point: Instant,
    /// Estimator observing only this track's chunks.
    estimator: LiveEdgeEstimator,
    state: TrackState,
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

impl<B: LiveSyncBuffer> Track<B> {
    fn view(&self, now: Instant) -> TrackView {
        TrackView {
            state: self.state,
            estimation: self.estimator.estimate(now),
            buffer_empty: self.buffer.peek_pts().is_none(),
            last_released_pts: self.last_released_pts,
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
}
