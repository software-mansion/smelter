#![allow(dead_code)]

use std::{collections::VecDeque, time::Duration};

use crate::MediaKind;

/// Synchronizes a live input to its live edge before any media is produced.
///
/// For every incoming chunk the controller samples `offset = arrival - pts`
/// (arrival measured as duration since the queue sync point). While the input
/// delivers faster than real time (e.g. HLS downloading backlog segments,
/// RTMP reconnect burst) the minimum of that offset keeps decreasing. Once it
/// stops improving for `stabilization_period`, the input delivers at real
/// time, and the minimum approximates the live edge: media with PTS `p` can be
/// available at `min_offset + p` at the earliest.
///
/// Until that happens chunks are held inside this struct and no queue track
/// should exist. On stabilization [`LiveEdgeSyncStart`] is returned:
/// - Register a queue track with `QueueTrackOffset::Pts(start.queue_offset)`.
/// - Use `start.first_pts` as the normalization base, subtract it from PTS/DTS
///   of every chunk (both `start.items` and chunks received later).
/// - Chunks older than `start.cutoff_pts` are scheduled in the past. Video is
///   trimmed to the last keyframe covering the cutoff, so those chunks are
///   needed for decoding, but resulting frames can be marked as not present.
///
/// Playback position is placed `buffer` behind the live edge, where `buffer`
/// is the held media duration clamped to `[min_buffer, max_buffer]`. Held
/// media older than that is dropped (keyframe-aligned for video).
///
/// Fallbacks:
/// - If the rate does not stabilize within `stabilization_timeout` (source
///   delivers faster than real time indefinitely, e.g. VOD content pushed over
///   a live protocol), the stream starts with all held chunks preserved and
///   playback scheduled from the oldest one (buffered playback).
/// - If held media exceeds `max_hold`, the stream starts at the current live
///   edge estimate to bound memory usage and latency.
///
/// `push` only reacts to incoming chunks. Detecting stabilization during
/// silence (e.g. HLS waiting for the next segment after backlog is drained)
/// requires calling `poll`; `deadline` returns when the next call can trigger.
#[derive(Debug)]
pub(crate) struct LiveEdgeSync<T> {
    config: LiveEdgeSyncConfig,
    held: VecDeque<HeldChunk<T>>,
    started: bool,

    first_sample_at: Option<Duration>,
    /// Monotonized newest PTS (running max, B-frame reordering ignored)
    max_pts: Option<Duration>,
    min_held_pts: Option<Duration>,
    /// Live edge estimate, `min(arrival - pts)` over all samples (signed nanoseconds,
    /// PTS can be larger than arrival e.g. for epoch based timestamps)
    min_offset: Option<i128>,
    /// Value of the live edge estimate when the current plateau started
    plateau_anchor: i128,
    /// Last time the estimate improved by more than `plateau_threshold`
    plateau_since: Duration,
}

#[derive(Debug, Clone)]
pub(crate) struct LiveEdgeSyncConfig {
    /// Jitter allowance when detecting that the live edge estimate stopped improving
    pub plateau_threshold: Duration,
    /// How long the live edge estimate has to stay flat (within `plateau_threshold`)
    /// before the input is considered stabilized
    pub stabilization_period: Duration,
    /// Time since the first chunk after which waiting for stabilization is abandoned
    /// and the stream starts with all held chunks preserved
    pub stabilization_timeout: Duration,
    /// Minimal duration between the live edge and the playback position
    pub min_buffer: Duration,
    /// Maximal duration between the live edge and the playback position, held media
    /// older than that is dropped on start
    pub max_buffer: Duration,
    /// Cap on media held during stabilization, when exceeded the stream starts
    /// immediately at the current live edge estimate
    pub max_hold: Duration,
}

impl Default for LiveEdgeSyncConfig {
    fn default() -> Self {
        Self {
            plateau_threshold: Duration::from_millis(200),
            stabilization_period: Duration::from_secs(2),
            stabilization_timeout: Duration::from_secs(10),
            min_buffer: Duration::from_secs(1),
            max_buffer: Duration::from_secs(4),
            max_hold: Duration::from_secs(90),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct LiveEdgeChunkMeta {
    /// Raw PTS, before any normalization
    pub pts: Duration,
    /// Whether decoding can start from this chunk, should be `true` for audio
    pub keyframe: bool,
    pub kind: MediaKind,
}

#[derive(Debug)]
pub(crate) enum LiveEdgeSyncEvent<T> {
    /// Stream did not start yet, the chunk was buffered inside
    Hold,
    /// Stream starts with this chunk, register a queue track and flush `items`
    Start(LiveEdgeSyncStart<T>),
    /// Stream already started, forward the chunk
    Forward(T),
}

#[derive(Debug)]
pub(crate) struct LiveEdgeSyncStart<T> {
    /// Value for `QueueTrackOffset::Pts` when `first_pts` is used as the
    /// normalization base
    pub queue_offset: Duration,
    /// Normalization base, subtract from PTS/DTS of `items` and all chunks
    /// received later
    pub first_pts: Duration,
    /// Chunks with PTS below this value are scheduled in the past, they are kept
    /// only so the decoder can produce the frames at and after the cutoff
    pub cutoff_pts: Duration,
    pub reason: LiveEdgeStartReason,
    /// Held chunks that are still relevant, in arrival order
    pub items: Vec<T>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LiveEdgeStartReason {
    /// Incoming rate stabilized to real time
    LiveEdge,
    /// Rate did not stabilize within `stabilization_timeout`
    StabilizationTimeout,
    /// Held media exceeded `max_hold`
    HoldOverflow,
    /// `flush` was called
    Flush,
}

#[derive(Debug)]
struct HeldChunk<T> {
    pts: Duration,
    keyframe: bool,
    video: bool,
    item: T,
}

impl<T> LiveEdgeSync<T> {
    pub fn new(config: LiveEdgeSyncConfig) -> Self {
        Self {
            config,
            held: VecDeque::new(),
            started: false,
            first_sample_at: None,
            max_pts: None,
            min_held_pts: None,
            min_offset: None,
            plateau_anchor: 0,
            plateau_since: Duration::ZERO,
        }
    }

    pub fn started(&self) -> bool {
        self.started
    }

    /// `now` is the duration since the queue sync point (`sync_point.elapsed()`)
    pub fn push(&mut self, now: Duration, meta: LiveEdgeChunkMeta, item: T) -> LiveEdgeSyncEvent<T> {
        if self.started {
            return LiveEdgeSyncEvent::Forward(item);
        }
        self.sample(now, meta.pts);
        self.min_held_pts = Some(match self.min_held_pts {
            Some(min_pts) => min_pts.min(meta.pts),
            None => meta.pts,
        });
        self.held.push_back(HeldChunk {
            pts: meta.pts,
            keyframe: meta.keyframe,
            video: matches!(meta.kind, MediaKind::Video(_)),
            item,
        });
        match self.check_triggers(now) {
            Some(start) => LiveEdgeSyncEvent::Start(start),
            None => LiveEdgeSyncEvent::Hold,
        }
    }

    /// Check time based triggers, has to be called when no chunks arrive to detect
    /// stabilization during silence
    pub fn poll(&mut self, now: Duration) -> Option<LiveEdgeSyncStart<T>> {
        if self.started {
            return None;
        }
        self.check_triggers(now)
    }

    /// `now` value at which the next `poll` call can trigger a start
    pub fn deadline(&self) -> Option<Duration> {
        if self.started {
            return None;
        }
        let first_sample_at = self.first_sample_at?;
        Some(Duration::min(
            self.plateau_since + self.config.stabilization_period,
            first_sample_at + self.config.stabilization_timeout,
        ))
    }

    /// Force the stream to start with all held chunks preserved (e.g. on EOS
    /// during stabilization)
    pub fn flush(&mut self, now: Duration) -> Option<LiveEdgeSyncStart<T>> {
        if self.started || self.held.is_empty() {
            return None;
        }
        Some(self.start_keep_all(now, LiveEdgeStartReason::Flush))
    }

    pub fn held_duration(&self) -> Duration {
        match (self.max_pts, self.min_held_pts) {
            (Some(max_pts), Some(min_pts)) => max_pts.saturating_sub(min_pts),
            _ => Duration::ZERO,
        }
    }

    fn sample(&mut self, now: Duration, pts: Duration) {
        let pts = match self.max_pts {
            Some(max_pts) => max_pts.max(pts),
            None => pts,
        };
        self.max_pts = Some(pts);

        let offset = nanos(now) - nanos(pts);
        match self.min_offset {
            None => {
                self.first_sample_at = Some(now);
                self.min_offset = Some(offset);
                self.plateau_anchor = offset;
                self.plateau_since = now;
            }
            Some(min_offset) => {
                if offset < self.plateau_anchor - nanos(self.config.plateau_threshold) {
                    self.plateau_anchor = offset;
                    self.plateau_since = now;
                }
                self.min_offset = Some(min_offset.min(offset));
            }
        }
    }

    fn check_triggers(&mut self, now: Duration) -> Option<LiveEdgeSyncStart<T>> {
        let first_sample_at = self.first_sample_at?;
        if now.saturating_sub(self.plateau_since) >= self.config.stabilization_period {
            return Some(self.start_at_edge(now, LiveEdgeStartReason::LiveEdge));
        }
        if now.saturating_sub(first_sample_at) >= self.config.stabilization_timeout {
            return Some(self.start_keep_all(now, LiveEdgeStartReason::StabilizationTimeout));
        }
        if self.held_duration() > self.config.max_hold {
            return Some(self.start_at_edge(now, LiveEdgeStartReason::HoldOverflow));
        }
        None
    }

    fn start_at_edge(&mut self, now: Duration, reason: LiveEdgeStartReason) -> LiveEdgeSyncStart<T> {
        let edge = self.min_offset.unwrap_or(0);
        let buffer = self
            .held_duration()
            .clamp(self.config.min_buffer, self.config.max_buffer);
        // chunk with PTS `p` is scheduled at `display_offset + p`
        let display_offset = edge + nanos(buffer);
        let cutoff = nanos(now) - display_offset;
        self.start(display_offset, cutoff, reason)
    }

    fn start_keep_all(&mut self, now: Duration, reason: LiveEdgeStartReason) -> LiveEdgeSyncStart<T> {
        let first_pts = nanos(self.min_held_pts.unwrap_or(Duration::ZERO));
        let display_offset = nanos(now) - first_pts;
        self.start(display_offset, first_pts, reason)
    }

    fn start(
        &mut self,
        display_offset: i128,
        cutoff: i128,
        reason: LiveEdgeStartReason,
    ) -> LiveEdgeSyncStart<T> {
        self.started = true;

        // Cutting video exactly at the cutoff would leave it undecodable, keep
        // everything from the last keyframe that covers the cutoff. If there is
        // no keyframe before the cutoff start at the first one.
        let video_start = self
            .held
            .iter()
            .filter(|held| held.video && held.keyframe && nanos(held.pts) <= cutoff)
            .map(|held| nanos(held.pts))
            .max()
            .or_else(|| {
                self.held
                    .iter()
                    .filter(|held| held.video && held.keyframe)
                    .map(|held| nanos(held.pts))
                    .min()
            })
            .unwrap_or(cutoff);

        let mut first_pts: Option<Duration> = None;
        let mut items = Vec::new();
        for held in self.held.drain(..) {
            let start_pts = if held.video { video_start } else { cutoff };
            if nanos(held.pts) < start_pts {
                continue;
            }
            first_pts = Some(match first_pts {
                Some(first_pts) => first_pts.min(held.pts),
                None => held.pts,
            });
            items.push(held.item);
        }
        let first_pts = first_pts.unwrap_or_else(|| duration_from_nanos(cutoff));

        LiveEdgeSyncStart {
            queue_offset: duration_from_nanos(display_offset + nanos(first_pts)),
            first_pts,
            cutoff_pts: duration_from_nanos(cutoff),
            reason,
            items,
        }
    }
}

fn nanos(duration: Duration) -> i128 {
    duration.as_nanos() as i128
}

fn duration_from_nanos(nanos: i128) -> Duration {
    Duration::from_nanos(nanos.max(0) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codecs::{AudioCodec, VideoCodec};

    fn ms(value: u64) -> Duration {
        Duration::from_millis(value)
    }

    fn video(pts: Duration, keyframe: bool) -> LiveEdgeChunkMeta {
        LiveEdgeChunkMeta {
            pts,
            keyframe,
            kind: MediaKind::Video(VideoCodec::H264),
        }
    }

    fn audio(pts: Duration) -> LiveEdgeChunkMeta {
        LiveEdgeChunkMeta {
            pts,
            keyframe: true,
            kind: MediaKind::Audio(AudioCodec::Aac),
        }
    }

    /// Continuous real time delivery (RTMP-like), 100ms video frames, keyframe
    /// every 1s, first media packet at now=10s with pts=0
    #[test]
    fn steady_delivery_starts_after_stabilization_period() {
        let mut sync = LiveEdgeSync::new(LiveEdgeSyncConfig::default());

        let mut start = None;
        for i in 0..=20u64 {
            let meta = video(ms(i * 100), i % 10 == 0);
            match sync.push(ms(10_000 + i * 100), meta, i) {
                LiveEdgeSyncEvent::Hold => assert!(i < 20),
                LiveEdgeSyncEvent::Start(s) => {
                    assert_eq!(i, 20);
                    start = Some(s);
                }
                LiveEdgeSyncEvent::Forward(_) => panic!("unexpected forward"),
            }
        }

        let start = start.unwrap();
        assert_eq!(start.reason, LiveEdgeStartReason::LiveEdge);
        // held span (2s) is within [min_buffer, max_buffer], nothing is dropped
        assert_eq!(start.items, (0..=20).collect::<Vec<_>>());
        assert_eq!(start.first_pts, ms(0));
        // edge=10s, buffer=2s => first frame is scheduled exactly at start (12s)
        assert_eq!(start.queue_offset, ms(12_000));
        assert_eq!(start.cutoff_pts, ms(0));

        // after start chunks are forwarded
        match sync.push(ms(12_100), video(ms(2100), false), 21) {
            LiveEdgeSyncEvent::Forward(item) => assert_eq!(item, 21),
            event => panic!("unexpected event {event:?}"),
        }
        assert!(sync.started());
        assert_eq!(sync.deadline(), None);
    }

    /// Burst of backlog followed by silence (HLS-like). 6s of media arrives
    /// instantly at now=100s, video keyframes every 2s, audio every 100ms.
    #[test]
    fn burst_then_silence_starts_at_live_edge() {
        let mut sync = LiveEdgeSync::new(LiveEdgeSyncConfig {
            min_buffer: ms(1000),
            max_buffer: ms(3000),
            ..Default::default()
        });

        let mut id = 0u64;
        for i in 0..60u64 {
            let pts = ms(i * 100);
            assert!(matches!(
                sync.push(ms(100_000), video(pts, i % 20 == 0), id),
                LiveEdgeSyncEvent::Hold
            ));
            assert!(matches!(
                sync.push(ms(100_000), audio(pts), id + 1),
                LiveEdgeSyncEvent::Hold
            ));
            id += 2;
        }

        // silence, stabilization period not over yet
        assert!(sync.poll(ms(101_000)).is_none());
        assert_eq!(sync.deadline(), Some(ms(102_000)));

        let start = sync.poll(ms(102_000)).unwrap();
        assert_eq!(start.reason, LiveEdgeStartReason::LiveEdge);
        // edge = 100s - 5.9s = 94.1s, buffer = clamp(5.9s, 1s, 3s) = 3s,
        // cutoff = 102s - 97.1s = 4.9s
        assert_eq!(start.cutoff_pts, ms(4900));
        // video snaps back to the keyframe at 4s, audio is cut exactly
        assert_eq!(start.first_pts, ms(4000));
        assert_eq!(start.queue_offset, ms(101_100));
        let video_kept = start.items.iter().filter(|id| *id % 2 == 0).count();
        let audio_kept = start.items.iter().filter(|id| *id % 2 == 1).count();
        assert_eq!(video_kept, 20); // pts 4.0s..=5.9s
        assert_eq!(audio_kept, 11); // pts 4.9s..=5.9s
    }

    /// Source delivering 3x faster than real time never stabilizes, after the
    /// timeout it starts as buffered playback with all chunks preserved.
    #[test]
    fn faster_than_real_time_falls_back_to_keep_all() {
        let mut sync = LiveEdgeSync::new(LiveEdgeSyncConfig::default());

        let mut start = None;
        for i in 0..=100u64 {
            let event = sync.push(ms(50_000 + i * 100), video(ms(i * 300), i % 10 == 0), i);
            match event {
                LiveEdgeSyncEvent::Hold => assert!(i < 100),
                LiveEdgeSyncEvent::Start(s) => start = Some(s),
                LiveEdgeSyncEvent::Forward(_) => panic!("unexpected forward"),
            }
        }

        let start = start.unwrap();
        assert_eq!(start.reason, LiveEdgeStartReason::StabilizationTimeout);
        assert_eq!(start.items.len(), 101);
        assert_eq!(start.first_pts, ms(0));
        // oldest chunk is scheduled at the start time
        assert_eq!(start.queue_offset, ms(60_000));
        assert_eq!(start.cutoff_pts, ms(0));
    }

    /// Held media exceeding max_hold forces a start at the current edge estimate.
    #[test]
    fn hold_overflow_starts_at_current_edge() {
        let mut sync = LiveEdgeSync::new(LiveEdgeSyncConfig {
            min_buffer: ms(1000),
            max_buffer: ms(4000),
            max_hold: ms(5000),
            ..Default::default()
        });

        let mut start = None;
        for i in 0..=51u64 {
            let event = sync.push(ms(200_000), video(ms(i * 100), i % 10 == 0), i);
            match event {
                LiveEdgeSyncEvent::Hold => assert!(i < 51),
                LiveEdgeSyncEvent::Start(s) => start = Some(s),
                LiveEdgeSyncEvent::Forward(_) => panic!("unexpected forward"),
            }
        }

        let start = start.unwrap();
        assert_eq!(start.reason, LiveEdgeStartReason::HoldOverflow);
        // edge = 200s - 5.1s = 194.9s, buffer = clamp(5.1s, 1s, 4s) = 4s,
        // cutoff = 200s - 198.9s = 1.1s, video snaps to keyframe at 1s
        assert_eq!(start.cutoff_pts, ms(1100));
        assert_eq!(start.first_pts, ms(1000));
        assert_eq!(start.items, (10..=51).collect::<Vec<_>>());
        assert_eq!(start.queue_offset, ms(199_900));
    }

    #[test]
    fn flush_starts_with_all_chunks() {
        let mut sync = LiveEdgeSync::new(LiveEdgeSyncConfig::default());
        assert!(sync.flush(ms(1000)).is_none());

        assert!(matches!(
            sync.push(ms(30_000), video(ms(0), true), 0u64),
            LiveEdgeSyncEvent::Hold
        ));
        let start = sync.flush(ms(30_500)).unwrap();
        assert_eq!(start.reason, LiveEdgeStartReason::Flush);
        assert_eq!(start.items, vec![0]);
        assert_eq!(start.queue_offset, ms(30_500));
        assert!(sync.flush(ms(31_000)).is_none());
    }

    /// Non-monotonic PTS (B-frame reordering) must not destabilize the edge
    /// estimate, only the running max is sampled.
    #[test]
    fn b_frame_reordering_is_ignored() {
        let mut sync = LiveEdgeSync::new(LiveEdgeSyncConfig::default());

        let mut start = None;
        for i in 0..=20u64 {
            // decode order I(0) P(200) B(100) P(400) B(300) ...
            let pts = match (i, i % 2) {
                (0, _) => ms(0),
                (_, 1) => ms((i + 1) * 100),
                _ => ms((i - 1) * 100),
            };
            match sync.push(ms(10_000 + i * 100), video(pts, i == 0), i) {
                LiveEdgeSyncEvent::Hold => assert!(i < 20),
                LiveEdgeSyncEvent::Start(s) => start = Some(s),
                LiveEdgeSyncEvent::Forward(_) => panic!("unexpected forward"),
            }
        }

        let start = start.unwrap();
        assert_eq!(start.reason, LiveEdgeStartReason::LiveEdge);
        assert_eq!(start.items.len(), 21);
        assert_eq!(start.first_pts, ms(0));
    }

    #[test]
    fn no_triggers_before_first_chunk() {
        let mut sync = LiveEdgeSync::<u64>::new(LiveEdgeSyncConfig::default());
        assert!(sync.poll(ms(100_000)).is_none());
        assert_eq!(sync.deadline(), None);
        assert!(!sync.started());
    }
}
