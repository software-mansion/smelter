use std::time::{Duration, Instant};

use tracing::{debug, trace};

use super::{
    LiveSyncOptions,
    buffer::{BufferingStrategy, LiveSyncBuffer},
    decision_correct_mode::correct_mode_decision,
    decision_start_track::start_track_decision,
    edge_estimator::{EdgeEstimate, LiveEdgeEstimator},
    mode::{IndependentMode, Mode},
    stats::LiveSyncTrackStats,
    track::LiveSyncDeadline,
};
use crate::pipeline::utils::input_sync::{
    BoxedTrackSink, InputSyncItem, InputSyncStatsSender, TrackClosedError, TrackEvent, TrackKind,
};

use crate::prelude::*;

/// pts jump (in either direction) treated as a discontinuity of the input
/// timeline; the old edge estimate does not describe the new timeline.
const DISCONTINUITY_THRESHOLD: Duration = Duration::from_secs(10);

/// Lead over the playback position below which chunks are force-released.
const MIN_QUEUE_HEADROOM: Duration = Duration::from_millis(100);

/// The whole mutable state of an input, cross-track and per-track, kept behind one mutex;
/// [`LiveSync`] and [`LiveSyncTrack`] are thin handles to it. Decisions
/// ([`start_track_decision`], [`correct_mode_decision`]) read it and are applied here.
pub(super) struct SharedState<B: LiveSyncBuffer> {
    pub options: LiveSyncOptions,
    /// Instant that output timestamps are measured from.
    pub sync_point: Instant,
    /// Estimator observing chunks of all tracks; its edge is defined by the
    /// freshest track.
    pub shared_estimator: LiveEdgeEstimator,
    pub mode: Option<Mode>,
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
            mode: None,
            audio: None,
            video: None,
            stats_sender,
        }
    }

    pub fn track(&self, kind: TrackKind) -> Option<&Track<B>> {
        match kind {
            TrackKind::Audio => self.audio.as_ref(),
            TrackKind::Video => self.video.as_ref(),
        }
    }

    pub fn is_started(&self, kind: TrackKind) -> bool {
        self.mode.is_some_and(|mode| mode.is_started(kind))
    }

    pub(super) fn add_track(
        &mut self,
        kind: TrackKind,
        sink: BoxedTrackSink<B::Chunk>,
    ) -> LiveSyncDeadline {
        debug!(?kind, "Live sync: adding track");
        let deadline = LiveSyncDeadline::new();
        let track = Track {
            kind,
            sync_point: self.sync_point,
            estimator: LiveEdgeEstimator::new(
                self.sync_point,
                self.options.stabilization_tolerance,
                self.options.stabilization_period,
            ),
            buffer: B::default(),
            sink,
            deadline: deadline.clone(),
            last_released_pts: None,
            last_written: None,
            stats: LiveSyncTrackStats::new(&self.stats_sender, kind, self.sync_point),
        };
        match kind {
            TrackKind::Audio => self.audio = Some(track),
            TrackKind::Video => self.video = Some(track),
        }
        deadline
    }

    /// Runs every transition due at `now` (resets, start decisions, corrections) and releases
    /// every releasable chunk. Driven by writes and by the periodic ticker, so time-based
    /// transitions fire during delivery pauses too.
    pub(super) fn tick(&mut self, now: Instant) {
        self.drop_closed_tracks();

        if let Some(audio) = &self.audio
            && audio.is_stalled(self.mode, self.options.stale_estimate_threshold, now)
        {
            debug!("Live sync: audio track stalled, resetting");
            self.reset_track(TrackKind::Audio, now);
        }

        if let Some(video) = &self.video
            && video.is_stalled(self.mode, self.options.stale_estimate_threshold, now)
        {
            debug!("Live sync: video track stalled, resetting");
            self.reset_track(TrackKind::Video, now);
        }

        // audio first, so video's decision can align to it
        for kind in [TrackKind::Audio, TrackKind::Video] {
            if let Some(mode) = start_track_decision(self, kind, now) {
                self.start_track(kind, mode);
            }
        }

        let corrected_mode = correct_mode_decision(self, now);
        if let Some(change) = Mode::diff(self.mode, corrected_mode) {
            match change.is_minor() {
                true => trace!(?change, "Live sync: mode corrected"),
                false => debug!(?change, "Live sync: mode corrected"),
            }
            self.mode = corrected_mode;
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

        self.publish_deadlines(now);
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
        self.publish_deadlines(now);
        self.report_stats();
    }

    /// Oldest input pts each track could still release on time under its current anchor.
    fn publish_deadlines(&mut self, now: Instant) {
        let now_pts = self.sync_point.timestamp_at(now);
        for kind in [TrackKind::Audio, TrackKind::Video] {
            let anchor = self.mode.and_then(|mode| mode.anchor(kind));
            let track = match kind {
                TrackKind::Audio => self.audio.as_ref(),
                TrackKind::Video => self.video.as_ref(),
            };
            if let Some(track) = track {
                let deadline =
                    anchor.map(|anchor| anchor.to_input_pts(now_pts + MIN_QUEUE_HEADROOM));
                track.deadline.set(deadline);
            }
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
        track.stats.report_bytes_received(chunk.size());
        if let Some(anchor) = self.mode.and_then(|mode| mode.anchor(kind)) {
            let output_pts = anchor.to_output_pts(chunk.pts());
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
    /// backlog is released with it by the regular release in the same tick.
    fn start_track(&mut self, kind: TrackKind, mode: Mode) {
        let Some(anchor) = mode.anchor(kind) else {
            return;
        };
        let Some(track) = self.track(kind) else {
            return;
        };
        debug!(
            ?kind,
            ?anchor,
            buffer = ?track.buffer.stats(),
            ?mode,
            "Live sync: track started"
        );
        self.mode = Some(mode);
    }

    /// Gives up on the live edge of one track: releases everything it buffered and goes back to
    /// waiting for a start decision. Its own anchor is forgotten; a shared one stays for the
    /// other track.
    fn reset_track(&mut self, kind: TrackKind, now: Instant) {
        let flush_anchor = self
            .track(kind)
            .and_then(|track| track.flush_anchor(self.mode, self.options.buffering_strategy, now));
        self.mode = self.mode.and_then(|mode| mode.reset_track(kind));
        let track = match kind {
            TrackKind::Audio => self.audio.as_mut(),
            TrackKind::Video => self.video.as_mut(),
        };
        let Some(track) = track else {
            return;
        };
        debug!(
            ?kind,
            ?flush_anchor,
            buffer = ?track.buffer.stats(),
            "Live sync: resetting track"
        );
        track.estimator = LiveEdgeEstimator::new(
            track.sync_point,
            self.options.stabilization_tolerance,
            self.options.stabilization_period,
        );
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
        let Some(mode) = &mut self.mode else {
            return false;
        };
        let Some(anchor) = mode.anchor(kind) else {
            return false;
        };
        let Some(next_pts) = track.buffer.peek_next_pts() else {
            return false;
        };
        // a chunk about to miss the queue is released even if it is behind a gap that might
        // still be filled
        let now_pts = self.sync_point.timestamp_at(now);
        let is_about_to_miss = anchor.to_output_pts(next_pts) <= now_pts + MIN_QUEUE_HEADROOM;
        let chunk = match is_about_to_miss {
            true => track.buffer.read(),
            false => track.buffer.try_read(),
        };
        let Some(mut chunk) = chunk else {
            return false;
        };

        // Late content is only decoded, so the decoder catches up instead of the queue playing
        // it back sped up. The newest video frame is always presented, so the output shows
        // something right away.
        let is_last_video_frame =
            kind == TrackKind::Video && track.buffer.peek_next_pts().is_none();
        let decode_only = !is_last_video_frame && anchor.to_output_pts(chunk.pts()) <= now_pts;
        if decode_only {
            chunk.mark_decode_only();
        }

        mode.nudge_anchor_toward_target(kind, chunk.pts(), self.options.buffering_strategy);
        let anchor = mode.anchor(kind).unwrap_or(anchor);
        track.release_chunk(chunk, anchor);
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
                self.mode = self.mode.and_then(|mode| mode.reset_track(kind));
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
        let Some(Mode::Shared(shared)) = self.mode else {
            return false;
        };
        let (track, other, other_kind) = match kind {
            TrackKind::Audio => (self.audio.as_ref(), self.video.as_ref(), TrackKind::Video),
            TrackKind::Video => (self.video.as_ref(), self.audio.as_ref(), TrackKind::Audio),
        };
        let (Some(track), Some(other)) = (track, other) else {
            return false;
        };
        if !shared.is_started(kind) || !shared.is_started(other_kind) {
            return false;
        }
        let Some(pts) = track.buffer.peek_next_pts() else {
            return false;
        };

        let now_pts = self.sync_point.timestamp_at(now);
        if shared.anchor.current.to_output_pts(pts) <= now_pts + MIN_QUEUE_HEADROOM {
            // the chunk is about to miss the queue; release it now instead of
            // waiting for the other track
            return false;
        }
        match other.buffer.peek_next_pts() {
            Some(other_pts) => other_pts < pts,
            None => true,
        }
    }

    /// Drops the live edge state built for the old timeline when `pts` does not belong to it
    /// anymore. Only the track that jumped is reset; the other track keeps playing and, if it
    /// was sharing the anchor, leads on its own until the tracks converge again.
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

        self.reset_track(kind, now);
        // The shared estimate mixes both timelines from now on, so it must not size the other
        // track's anchor; the other track leads until the tracks converge again. A shared anchor
        // nobody applies belongs to the old timeline and would be reused by the next start.
        if let Some(Mode::Shared(shared)) = self.mode {
            let other = match kind {
                TrackKind::Audio => TrackKind::Video,
                TrackKind::Video => TrackKind::Audio,
            };
            self.mode = match shared.is_started(other) {
                true => Some(Mode::Independent(IndependentMode {
                    leader_kind: other,
                    leader_anchor: shared.anchor,
                    secondary_track_offset: None,
                })),
                false => None,
            };
        }
        self.shared_estimator = LiveEdgeEstimator::new(
            self.sync_point,
            self.options.stabilization_tolerance,
            self.options.stabilization_period,
        );

        // After the reset, so flushed old chunks precede the event in the sink.
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
                .and_then(|mode| Some((mode.anchor(kind)?, mode.target(kind)?)));
            track.stats.report_state_change(kind, self.mode);
            let estimator = match self.mode {
                Some(Mode::Shared(shared)) if shared.is_started(kind) => {
                    Some(&self.shared_estimator)
                }
                Some(Mode::Independent(independent)) if independent.anchor(kind).is_some() => {
                    Some(&track.estimator)
                }
                _ => None,
            };
            track
                .stats
                .report_state_snapshot(&track.buffer, anchors, estimator);
        }
    }
}

/// State of a single track, owned by [`SharedState`].
pub(super) struct Track<B: LiveSyncBuffer> {
    kind: TrackKind,
    /// Instant that output timestamps are measured from.
    sync_point: Instant,
    /// Estimator observing only this track's chunks. A track starts only once it has an
    /// estimate and the estimator is replaced only after the track left the mode
    /// (`reset_track`), so a started track always has an estimate.
    pub estimator: LiveEdgeEstimator,
    buffer: B,
    /// Receives the chunks this track releases.
    sink: BoxedTrackSink<B::Chunk>,
    /// Shared with the input, see [`LiveSyncDeadline`].
    deadline: LiveSyncDeadline,
    /// Output pts the released content ends at. Used to maintain continuity after
    /// reset so it has to survive the reset itself.
    pub last_released_pts: Option<Timestamp>,
    /// pts of the last written chunk and its arrival time. Deliberately not
    /// cleared on reset, so a discontinuity is detectable right after one.
    last_written: Option<(Timestamp, Instant)>,
    stats: LiveSyncTrackStats,
}

impl<B: LiveSyncBuffer> Track<B> {
    /// Pushes a chunk out, with its timestamps mapped onto the output
    /// timeline by `anchor`.
    fn release_chunk(&mut self, mut chunk: B::Chunk, anchor: TimestampOffset) {
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

    /// Anchor to release the buffered chunks with on reset; `None` when nothing was observed.
    fn flush_anchor(
        &self,
        mode: Option<Mode>,
        strategy: BufferingStrategy,
        now: Instant,
    ) -> Option<TimestampOffset> {
        if let Some(anchor) = mode.and_then(|mode| mode.anchor(self.kind)) {
            return Some(anchor);
        }

        let now_pts = self.sync_point.timestamp_at(now);
        // continue where the released content ended, if that is still ahead of the queue
        if let Some(last_pts) = self.last_released_pts
            && last_pts > now_pts + MIN_QUEUE_HEADROOM
        {
            return Some(Timestamp::offset(self.buffer.peek_next_pts()?, last_pts));
        }

        // otherwise like a fresh start: last observed chunk at the desired buffer
        let (last_written_pts, _) = self.last_written?;
        Some(Timestamp::offset(
            last_written_pts,
            now_pts + strategy.desired_buffer(),
        ))
    }

    /// Track stalled long enough that it has to earn its start again: it ran out of released
    /// content, or its timeline slipped against its own recent deliveries.
    fn is_stalled(
        &self,
        mode: Option<Mode>,
        stale_estimate_threshold: Duration,
        now: Instant,
    ) -> bool {
        let started = mode.is_some_and(|mode| mode.is_started(self.kind));
        if !started {
            return false;
        }

        const STALL_TIMEOUT: Duration = Duration::from_secs(1);
        let now_pts = self.sync_point.timestamp_at(now);

        // Chunks that keep arriving but map late are not a stall: the anchor is undersized for
        // this track, and a restart on a shared anchor would only rejoin it and be late again.
        let delivery_stopped = match self.last_written {
            Some((_, arrived_at)) => now.saturating_duration_since(arrived_at) >= STALL_TIMEOUT,
            None => true,
        };
        // Slightly late track can still recover; reset would cause a gap of at least the
        // stabilization period.
        let released_content_ran_out = match self.last_released_pts {
            Some(last_pts) => last_pts + STALL_TIMEOUT <= now_pts,
            None => false,
        };
        let ran_out_of_content =
            delivery_stopped && self.buffer.peek_next_pts().is_none() && released_content_ran_out;

        // The upper bound still describes an edge the stream is no longer at. The full window
        // would take its whole look-back to notice; a reset starts over on a fresh estimate.
        let estimate_is_stale = match self.estimator.estimate(now) {
            Some(EdgeEstimate {
                upper_bound,
                recent_upper_bound_pts: Some(recent_upper_bound_pts),
                ..
            }) => {
                upper_bound.pts - recent_upper_bound_pts
                    >= Timestamp::from(stale_estimate_threshold)
            }
            _ => false,
        };

        ran_out_of_content || estimate_is_stale
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
