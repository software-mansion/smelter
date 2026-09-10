use std::time::{Duration, Instant};

use tracing::{debug, trace};

use super::{
    LiveSyncOptions,
    buffer::LiveSyncBuffer,
    decision_correct_mode::correct_mode_decision,
    decision_start_track::start_track_decision,
    edge_estimator::{EdgeEstimate, LiveEdgeEstimator},
    mode::{IndependentMode, Mode},
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

/// The whole mutable state of an input, cross-track and per-track, kept behind one mutex;
/// [`LiveSync`] and [`LiveSyncTrack`] are thin handles to it. Decisions
/// ([`start_track_decision`], [`correct_mode_decision`]) read it and are applied here.
pub(super) struct SharedState<B: LiveSyncBuffer> {
    pub options: LiveSyncOptions,
    /// Instant that output timestamps are measured from.
    sync_point: Instant,
    /// Estimator observing chunks of all tracks; its edge is defined by the
    /// freshest track.
    pub shared_estimator: LiveEdgeEstimator,
    /// Anchors the started tracks apply; `None` until the first track starts.
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

    pub fn now_pts(&self, now: Instant) -> Timestamp {
        self.sync_point.timestamp_at(now)
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

    /// Track stalled long enough that it has to earn its start again.
    fn should_reset(&self, kind: TrackKind, now: Instant) -> bool {
        let Some(track) = self.track(kind) else {
            return false;
        };
        if !self.is_started(kind) || track.buffer.peek_pts().is_some() {
            return false;
        }
        let Some(last_pts) = track.last_released_pts else {
            return false;
        };

        // Slightly late track can still recover; reset would cause a gap of at
        // least the stabilization period.
        last_pts + Timestamp::from_secs(1) <= self.now_pts(now)
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
    pub(super) fn tick(&mut self, now: Instant) {
        self.drop_closed_tracks();

        for kind in [TrackKind::Audio, TrackKind::Video] {
            if self.should_reset(kind, now) {
                debug!(?kind, "Live sync: track stalled, resetting");
                self.reset_track(kind, now);
            }
        }

        // audio first, so video's decision can align to it
        for kind in [TrackKind::Audio, TrackKind::Video] {
            if let Some(mode) = start_track_decision(self, kind, now) {
                self.start_track(kind, mode, now);
            }
        }

        let corrected_mode = correct_mode_decision(self, now);
        if let Some(change) = Mode::diff(self.mode, corrected_mode) {
            debug!(?change, mode=?corrected_mode, "Live sync: mode corrected");
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
            offset = ?anchor.as_offset(),
            buffered = track.buffer.pts_values().count(),
            ?mode,
            "Live sync: track started"
        );

        // Not using regular try_release_chunk, because decoder should only drop late packets
        // during startup phase. It avoids visible video speedup when queue input drains decoder
        // to reach "on time" position
        while let Some(mut chunk) = track.buffer.try_read() {
            // The last video frame is always presented, so the output shows something right away
            let is_last_video_frame = kind == TrackKind::Video && track.buffer.peek_pts().is_none();
            let is_too_late = anchor.to_output_pts(chunk.pts()) <= now_pts;
            if !is_last_video_frame && is_too_late {
                chunk.mark_decode_only();
            }
            track.release_chunk(chunk, anchor);
        }

        self.mode = Some(mode);
    }

    /// Anchor to release the buffered chunks of `kind` with on reset; `None` when nothing was
    /// observed.
    fn flush_anchor(&self, kind: TrackKind, now: Instant) -> Option<TimestampAnchor> {
        let track = match kind {
            TrackKind::Audio => self.audio.as_ref()?,
            TrackKind::Video => self.video.as_ref()?,
        };
        if let Some(anchor) = self.mode.and_then(|mode| mode.anchor(kind)) {
            return Some(anchor);
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
            offset = ?flush_anchor.map(|anchor| anchor.as_offset()),
            buffered = track.buffer.pts_values().count(),
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
        let Some(chunk) = track.buffer.try_read() else {
            return false;
        };

        // nudge will be applied on the next chunk
        mode.nudge_anchor_toward_target(kind, chunk.pts(), self.options.buffering_strategy);
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
        let Some(pts) = track.buffer.peek_pts() else {
            return false;
        };

        let now_pts = self.sync_point.timestamp_at(now);
        if shared.anchor.current.to_output_pts(pts) <= now_pts + MIN_QUEUE_HEADROOM {
            // the chunk is about to miss the queue; release it now instead of
            // waiting for the other track
            return false;
        }
        match other.buffer.peek_pts() {
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

/// Secondary track offset that presents both live edges at once.
pub(super) fn edge_offset(
    audio: &EdgeEstimate,
    video: &EdgeEstimate,
    leader: TrackKind,
) -> Timestamp {
    let (audio, video) = (audio.upper_bound.pts, video.upper_bound.pts);
    match leader {
        // offset for video when audio is a leader
        TrackKind::Audio => audio - video,
        // offset for audio when video is a leader (video leads only while audio is not
        // started, so this is unused until audio takes over)
        TrackKind::Video => video - audio,
    }
}

/// State of a single track, owned by [`SharedState`].
pub(super) struct Track<B: LiveSyncBuffer> {
    kind: TrackKind,
    /// Instant that output timestamps are measured from.
    sync_point: Instant,
    /// Estimator observing only this track's chunks.
    pub estimator: LiveEdgeEstimator,
    buffer: B,
    /// Receives the chunks this track releases.
    sink: BoxedTrackSink<B::Chunk>,
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
