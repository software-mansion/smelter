use std::{collections::VecDeque, time::Duration, time::Instant};

use tracing::{debug, trace};

use super::{
    AtUpperLimit, LiveSyncOptions,
    buffer::LiveSyncBuffer,
    decide::{Playback, REACHED, Reanchor, ReanchorKind, ReanchorReason, TrackView},
    measure::Measure,
    stats::LiveSyncTrackStats,
    stitch::Stitch,
    timeline::Timeline,
    track::LiveSyncDeadline,
};
use crate::{
    pipeline::utils::input_sync::{
        BoxedTrackSink, InputSyncItem, InputSyncStatsSender, TrackClosedError, TrackKind,
    },
    stats::LiveSyncTrackState,
};

use crate::prelude::*;

/// TR6: tracks with edges at least this far apart are on unrelated timelines.
const APART: Duration = Duration::from_secs(5);

/// TR6: tracks with edges closer than this share a timeline again.
const TOGETHER: Duration = Duration::from_secs(3);

/// The whole mutable state of an input, cross-track and per-track, kept behind one mutex;
/// [`LiveSync`](super::LiveSync) and [`LiveSyncTrack`](super::track::LiveSyncTrack) are thin
/// handles to it. Every write and every tick runs the stages in order: stitch and measure the
/// written chunk, decide per playback timeline, release per track.
pub(super) struct SharedState<B: LiveSyncBuffer> {
    pub options: LiveSyncOptions,
    /// Instant that output timestamps are measured from.
    sync_point: Instant,
    audio: Option<Track<B>>,
    video: Option<Track<B>>,
    timelines: Timelines,
    /// Stage 0, when it is on.
    stitch: Option<Stitch>,
    stats_sender: InputSyncStatsSender,
}

/// Playback timelines of an input.
#[derive(Default)]
struct Timelines {
    /// The timeline of the input, from the start on (DC1).
    main: Option<Playback>,
    /// TR6: the timeline of video while the tracks are apart.
    video_apart: Option<Playback>,
}

impl Timelines {
    fn of(&mut self, kind: TrackKind) -> Option<&mut Playback> {
        match kind {
            TrackKind::Video if self.video_apart.is_some() => self.video_apart.as_mut(),
            _ => self.main.as_mut(),
        }
    }
}

/// State of a single track, owned by [`SharedState`].
pub(super) struct Track<B: LiveSyncBuffer> {
    pub kind: TrackKind,
    /// Stage 1, from the first chunk on.
    pub measure: Option<Measure>,
    pub buffer: B,
    /// Receives what this track releases.
    pub sink: BoxedTrackSink<B::Chunk>,
    /// Shared with the input, see [`LiveSyncDeadline`].
    deadline: LiveSyncDeadline,
    /// TR3, TR6: its chunks are scheduled on a playback timeline.
    placed: bool,
    /// DC6: newest presentation time among the chunks that were released to be shown.
    pub released_until: Option<Timestamp>,
    /// Content presented before it is late: where the released content ended at a jump.
    pub floor: Option<Timestamp>,
    /// RL4: chunks before the next keyframe can't be decoded.
    pub needs_keyframe: bool,
    /// Chunks written to the buffer and taken from it. A FIFO returns them in write order.
    written: u64,
    pub taken: u64,
    /// ST2: `written` of the chunks that the sink gets a discontinuity in front of.
    pub discontinuities: VecDeque<u64>,
    pub stats: LiveSyncTrackStats,
}

impl<B: LiveSyncBuffer> Track<B> {
    fn view(&mut self, now: Timestamp) -> Option<TrackView> {
        let measure = self.measure.as_mut()?;
        Some(TrackView {
            kind: self.kind,
            edge: measure.edge(),
            lateness: measure.regular_lateness(now),
            candidate: measure.candidate(now),
            counts: measure.counts(),
            idle: measure.is_idle(now),
            playing: self.released_until.is_some_and(|until| until > now),
        })
    }

    /// The timeline the track is on was re-anchored.
    fn reanchored(&mut self, reanchor: Reanchor) {
        self.stats.report_reanchor(reanchor);
        if reanchor.kind == ReanchorKind::Jump {
            self.floor = self.released_until;
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
            audio: None,
            video: None,
            timelines: Timelines::default(),
            stitch: options.tuning.stitch.then(Stitch::default),
            stats_sender,
        }
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
            measure: None,
            buffer: B::default(),
            sink,
            deadline: deadline.clone(),
            placed: false,
            released_until: None,
            floor: None,
            needs_keyframe: true,
            written: 0,
            taken: 0,
            discontinuities: VecDeque::new(),
            stats: LiveSyncTrackStats::new(&self.stats_sender, kind),
        };
        match kind {
            TrackKind::Audio => self.audio = Some(track),
            TrackKind::Video => self.video = Some(track),
        }
        deadline
    }

    /// Releases everything the track buffered and drops it together with its sink.
    pub(super) fn remove_track(&mut self, kind: TrackKind) {
        debug!(?kind, "Live sync: removing track");
        let now = self.sync_point.timestamp_now();
        self.flush_track(kind, now);
        self.forget_track(kind);
        self.publish_deadlines(now);
        self.report_stats(now);
    }

    /// RL6: the stream ended, so every track releases everything it buffered.
    pub(super) fn flush(&mut self) {
        debug!("Live sync: flush");
        let now = self.sync_point.timestamp_now();
        self.start(now, true);
        self.place_tracks(None);
        for kind in [TrackKind::Audio, TrackKind::Video] {
            self.flush_track(kind, now);
        }
        self.publish_deadlines(now);
        self.report_stats(now);
    }

    /// Driven by the periodic ticker, so decisions and releases happen while delivery pauses.
    pub(super) fn tick(&mut self) {
        self.run(self.sync_point.timestamp_now());
    }

    /// Fails once the sink of the track is gone, so the input can stop
    /// producing for it.
    pub(super) fn write_chunk(
        &mut self,
        kind: TrackKind,
        mut chunk: B::Chunk,
    ) -> Result<(), TrackClosedError> {
        let now = self.sync_point.timestamp_now();
        let has_other = match kind {
            TrackKind::Audio => self.video.is_some(),
            TrackKind::Video => self.audio.is_some(),
        };
        let track = match kind {
            TrackKind::Audio => self.audio.as_mut(),
            TrackKind::Video => self.video.as_mut(),
        };
        let Some(track) = track else {
            return Err(TrackClosedError);
        };
        track.stats.report_bytes_received(chunk.size());

        // stage 0
        if let Some(stitch) = &mut self.stitch {
            let stitched = stitch.observe(kind, chunk.pts(), now, has_other);
            if stitched.discontinuity {
                debug!(
                    ?kind,
                    pts=?chunk.pts(),
                    offset=?stitched.offset,
                    "Live sync: discontinuity"
                );
                track.discontinuities.push_back(track.written);
            }
            if !stitched.offset.is_zero() {
                chunk.apply_anchor(Timestamp::offset(Timestamp::ZERO, stitched.offset));
            }
        }

        // stage 1
        let pts = chunk.pts();
        let delay = now - pts;
        trace!(?kind, ?pts, ?delay, "Live sync: chunk written");
        let playback = match track.placed {
            true => self.timelines.of(kind),
            false => None,
        };
        match &mut track.measure {
            Some(measure) => {
                let taken_back = measure.observe(now, delay);
                if let Some(playback) = playback
                    && measure.counts()
                    && taken_back.is_positive()
                {
                    playback.rise_taken_back(taken_back);
                }
            }
            None => track.measure = Some(Measure::new(&self.options, now, delay)),
        }
        if let Some(playback) = self.timelines.of(kind)
            && track.placed
        {
            let presented = pts + playback.timeline.at(pts);
            track.stats.report_chunk_received(presented - now);
        }

        track.buffer.write(chunk);
        track.written += 1;
        track.enforce_capacity(pts);

        self.run(now);
        Ok(())
    }

    fn run(&mut self, now: Timestamp) {
        self.drop_closed_tracks();

        let started = self.timelines.main.is_some();
        for track in [&mut self.audio, &mut self.video].into_iter().flatten() {
            if let Some(measure) = &mut track.measure {
                measure.tick(now, started);
            }
        }

        self.start(now, false);
        self.decide(now);
        self.release(now);

        self.publish_deadlines(now);
        self.report_stats(now);
    }

    /// DC1: playback starts when the first burst of every track that has delivered is over.
    /// `force` starts it on whatever the tracks have delivered.
    fn start(&mut self, now: Timestamp, force: bool) {
        if self.timelines.main.is_some() {
            return;
        }
        let edge_of = |track: &Option<Track<B>>| {
            let measure = track.as_ref()?.measure.as_ref()?;
            Some(measure.edge())
        };
        let (audio_edge, video_edge) = (edge_of(&self.audio), edge_of(&self.video));

        let delivering = || {
            [&self.audio, &self.video]
                .into_iter()
                .flatten()
                .filter_map(|track| track.measure.as_ref())
        };
        let Some(first_arrival) = delivering().map(|measure| measure.first_arrival()).min() else {
            return;
        };
        let settled = delivering().all(|measure| measure.settled(now));
        let timed_out = first_arrival + self.options.tuning.start_timeout <= now;
        if !(settled || timed_out || force) {
            return;
        }

        let LiveSyncOptions { start, upper, .. } = self.options;
        match (audio_edge, video_edge) {
            // TR6
            (Some(audio_edge), Some(video_edge))
                if !self.options.shared_timeline
                    && (audio_edge - video_edge).abs_duration() >= APART =>
            {
                self.timelines.main = Some(Playback::start(audio_edge + start));
                self.timelines.video_apart = Some(Playback::start(video_edge + start));
            }
            _ => {
                let Some(mut edge) = Option::max(audio_edge, video_edge) else {
                    return;
                };
                // 2.4: the cut-off, on audio when the input has it
                let reference_edge = match self.audio {
                    Some(_) => audio_edge,
                    None => video_edge,
                };
                if self.options.at_upper == AtUpperLimit::Enforce
                    && let Some(reference_edge) = reference_edge
                {
                    edge = Timestamp::min(edge, reference_edge + upper - start);
                }
                self.timelines.main = Some(Playback::start(edge + start));
            }
        }
        debug!(
            ?audio_edge,
            ?video_edge,
            settled,
            apart = self.timelines.video_apart.is_some(),
            "Live sync: playback started"
        );

        for track in [&mut self.audio, &mut self.video].into_iter().flatten() {
            if let Some(measure) = &mut track.measure {
                measure.start_counting();
                track.placed = true;
            }
        }
    }

    /// TR3, TR6: puts tracks on a playback timeline. `distance`: how far apart the edges of the
    /// tracks are, when both count.
    fn place_tracks(&mut self, distance: Option<Duration>) {
        if self.timelines.main.is_none() {
            return;
        }
        if self.options.shared_timeline {
            for track in [&mut self.audio, &mut self.video].into_iter().flatten() {
                track.placed = true;
            }
            if self.timelines.video_apart.take().is_some()
                && let Some(video) = &mut self.video
            {
                debug!("Live sync: tracks share a timeline, video jumps");
                video.reanchored(Reanchor {
                    kind: ReanchorKind::Jump,
                    reason: ReanchorReason::TracksJoined,
                });
            }
            return;
        }

        // Without the hint a track is scheduled once it counts: its first burst has ended, so
        // its edge says whether it is on the timeline of the other track.
        let start = self.options.start;
        let apart = distance.is_some_and(|distance| distance >= APART);
        let placed_edge = |track: &mut Option<Track<B>>| {
            let track = track.as_mut()?;
            let measure = track.measure.as_ref()?;
            let newly_placed = !track.placed && measure.counts();
            track.placed |= newly_placed;
            newly_placed.then_some(measure.edge())
        };
        let audio_was_placed = self.audio.as_ref().is_some_and(|track| track.placed);
        let video_was_placed = self.video.as_ref().is_some_and(|track| track.placed);
        if let Some(edge) = placed_edge(&mut self.audio)
            && video_was_placed
            && apart
        {
            // audio gets the timeline of the input, and video keeps the one it has
            let own = Playback::start(edge + start);
            self.timelines.video_apart = self.timelines.main.replace(own);
        }
        if let Some(edge) = placed_edge(&mut self.video)
            && audio_was_placed
            && apart
        {
            self.timelines.video_apart = Some(Playback::start(edge + start));
        }
        if audio_was_placed && video_was_placed && apart && self.timelines.video_apart.is_none() {
            // taken apart, both keep the `T` they have
            debug!(
                ?distance,
                "Live sync: tracks are apart, video gets its own timeline"
            );
            self.timelines.video_apart = self.timelines.main.clone();
        }
    }

    /// Stage 2, for each playback timeline with the tracks that are on it (TR2, TR6).
    fn decide(&mut self, now: Timestamp) {
        let audio = self.audio.as_mut().and_then(|track| track.view(now));
        let video = self.video.as_mut().and_then(|track| track.view(now));
        let distance = match (audio, video) {
            (Some(audio), Some(video)) if audio.counts && video.counts => {
                Some((audio.edge - video.edge).abs_duration())
            }
            _ => None,
        };
        self.place_tracks(distance);

        let placed = |track: &Option<Track<B>>| track.as_ref().is_some_and(|track| track.placed);
        let audio = audio.filter(|_| placed(&self.audio));
        let video = video.filter(|_| placed(&self.video));
        let Some(main) = &mut self.timelines.main else {
            return;
        };

        let Some(apart) = &mut self.timelines.video_apart else {
            if let Some(reanchor) = main.decide(&self.options, [audio, video]) {
                for track in [&mut self.audio, &mut self.video].into_iter().flatten() {
                    if track.placed {
                        track.reanchored(reanchor);
                    }
                }
            }
            return;
        };

        if let Some(reanchor) = main.decide(&self.options, [audio, None])
            && let Some(audio) = &mut self.audio
        {
            audio.reanchored(reanchor);
        }
        let Some(video_track) = &mut self.video else {
            return;
        };
        if distance.is_none_or(|distance| distance >= TOGETHER) {
            if let Some(reanchor) = apart.decide(&self.options, [video, None]) {
                video_track.reanchored(reanchor);
            }
            return;
        }

        // TR6: the tracks are joined again, so the timeline of video moves to the one of audio
        let gap = main.timeline.value() - apart.timeline.value();
        let playing = video.is_some_and(|video| video.playing);
        if gap.abs() <= REACHED {
            debug!("Live sync: tracks are together again");
            self.timelines.video_apart = None;
        } else if gap.abs_duration() <= self.options.tuning.max_slew && playing {
            if let Some(reanchor) = apart.join(main.timeline.value()) {
                debug!(?gap, "Live sync: tracks are together again, video slews");
                video_track.reanchored(reanchor);
            }
        } else {
            debug!(?gap, "Live sync: tracks are together again, video jumps");
            self.timelines.video_apart = None;
            video_track.reanchored(Reanchor {
                kind: ReanchorKind::Jump,
                reason: ReanchorReason::TracksJoined,
            });
        }
    }

    /// Stage 3
    fn release(&mut self, now: Timestamp) {
        for kind in [TrackKind::Audio, TrackKind::Video] {
            let track = match kind {
                TrackKind::Audio => self.audio.as_mut(),
                TrackKind::Video => self.video.as_mut(),
            };
            if let Some(track) = track
                && track.placed
                && let Some(playback) = self.timelines.of(kind)
            {
                track.release(playback, now);
            }
        }
    }

    /// RL6 for one track. A track that is not scheduled yet is released the way a start by
    /// timeout would schedule it, on its edge of now.
    fn flush_track(&mut self, kind: TrackKind, now: Timestamp) {
        let track = match kind {
            TrackKind::Audio => self.audio.as_mut(),
            TrackKind::Video => self.video.as_mut(),
        };
        let Some(track) = track else {
            return;
        };
        let Some(measure) = &track.measure else {
            return;
        };
        let mut own = Timeline::new(measure.edge() + self.options.start);
        let timeline = match self.timelines.of(kind) {
            Some(playback) if track.placed => &mut playback.timeline,
            _ => &mut own,
        };
        debug!(?kind, buffer=?track.buffer.stats(), "Live sync: releasing everything");
        track.flush(timeline, now);
    }

    fn drop_closed_tracks(&mut self) {
        for kind in [TrackKind::Audio, TrackKind::Video] {
            let track = match kind {
                TrackKind::Audio => self.audio.as_ref(),
                TrackKind::Video => self.video.as_ref(),
            };
            if track.is_some_and(|track| track.sink.is_closed()) {
                debug!(?kind, "Live sync: track sink closed, dropping track");
                self.forget_track(kind);
            }
        }
    }

    /// TR1: a removed track stops counting at once.
    fn forget_track(&mut self, kind: TrackKind) {
        match kind {
            TrackKind::Audio => {
                self.audio = None;
                // video is the only track left, so its timeline is the one of the input
                if let Some(apart) = self.timelines.video_apart.take() {
                    self.timelines.main = Some(apart);
                }
            }
            TrackKind::Video => {
                self.video = None;
                self.timelines.video_apart = None;
            }
        }
        if let Some(stitch) = &mut self.stitch {
            stitch.remove_track(kind);
        }
    }

    /// RL7: the oldest pts each track would still take.
    fn publish_deadlines(&mut self, now: Timestamp) {
        for kind in [TrackKind::Audio, TrackKind::Video] {
            let track = match kind {
                TrackKind::Audio => self.audio.as_ref(),
                TrackKind::Video => self.video.as_ref(),
            };
            let Some(track) = track else {
                continue;
            };
            let playback = match track.placed {
                true => self.timelines.of(kind),
                false => None,
            };
            let deadline = playback.and_then(|playback| {
                // TR5: for the best-effort track lateness is no reason to stop reading
                let best_effort = self.options.at_upper == AtUpperLimit::Enforce
                    && playback
                        .status
                        .is_some_and(|status| status.reference != kind);
                if best_effort {
                    return None;
                }
                let value = playback.timeline.value();
                // RL4: while nothing plays, content that a restart would still play is kept
                let oldest_kept = Timestamp::max(value, playback.restart.unwrap_or(value));
                Some(now - oldest_kept)
            });
            track.deadline.set(deadline);
        }
    }

    fn report_stats(&mut self, now: Timestamp) {
        for kind in [TrackKind::Audio, TrackKind::Video] {
            let track = match kind {
                TrackKind::Audio => self.audio.as_mut(),
                TrackKind::Video => self.video.as_mut(),
            };
            let Some(track) = track else {
                continue;
            };
            let playback = match track.placed {
                true => self.timelines.of(kind),
                false => None,
            };
            let playback = playback.as_deref();
            let state = match (playback, track.released_until) {
                (None, _) | (_, None) => LiveSyncTrackState::WaitingForStart,
                (_, Some(until)) if until <= now => LiveSyncTrackState::Interrupted,
                (Some(playback), _) if playback.is_reanchoring() => LiveSyncTrackState::Reanchoring,
                _ => LiveSyncTrackState::Playing,
            };
            track
                .stats
                .report_state(now, state, &track.buffer, playback);
        }
    }
}
