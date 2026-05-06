use std::{
    collections::BTreeMap,
    fmt,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use tracing::{debug, trace};

use crate::pipeline::rtp::{
    RtpInputEvent, RtpPacket,
    rtp_input::{
        rollover_state::SequenceNumberRollover,
        rtcp_sync::{RtpNtpSyncPoint, RtpTimestampSync},
    },
};

use crate::prelude::*;

struct JitterBufferPacket {
    packet: webrtc::rtp::packet::Packet,
    pts: Duration,
}

#[derive(Debug, Clone, Copy)]
pub enum RtpJitterBufferMode {
    /// Release packets once the PTS span of buffered packets exceeds the window.
    /// Wall clock is not consulted.
    FixedWindow(Duration),
    /// Release packets when their output PTS (PTS + dynamic buffer size) is about
    /// to be reached on the wall clock, with `MIN_DECODE_TIME` of slack.
    RealTime,
}

// Value shared between both video and audio track, while actual RtpJitterBuffer
// should be created per track
#[derive(Debug, Clone)]
pub(crate) struct RtpJitterBufferSharedContext {
    mode: RtpJitterBufferMode,
    ntp_sync_point: Arc<RtpNtpSyncPoint>,
    input_buffer: BufferingStrategy,
}

impl RtpJitterBufferSharedContext {
    pub fn new(ctx: &Arc<PipelineCtx>, mode: RtpJitterBufferMode, reference_time: Instant) -> Self {
        let ntp_sync_point = RtpNtpSyncPoint::new(reference_time);
        Self {
            mode,
            ntp_sync_point,
            input_buffer: match mode {
                RtpJitterBufferMode::FixedWindow(window_size) => {
                    // PTS are synced on first packet, so if jitter buffer is the same as an
                    // input buffer then, at the worst case it would produce PTS at the time
                    // where queue already needs it, We are adding 80ms (`default_buffer_duration`)
                    // so packets can reach the queue on time.
                    //
                    // If input has an offset then above does not apply, however PTS should be
                    // normalized to zero, so adding a constant value should not affect anything
                    BufferingStrategy::FixedOffset {
                        offset: window_size + ctx.default_buffer_duration,
                    }
                }
                RtpJitterBufferMode::RealTime => {
                    BufferingStrategy::LatencyOptimized(LatencyOptimizedBuffer::new(reference_time))
                }
            },
        }
    }
}

/// Per-track jitter buffer for reordering and releasing RTP packets.
///
/// ## Packet flow
///
/// 1. `write_packet`
///    - Sync RTP timestamp to `reference_time`
/// 2. `try_read_packet`
///    - If `sequence_number` are continuous: return oldest
///    - If first packet or there are gaps in `sequence_number`:
///      - `FixedWindow(window)`: release when PTS span of buffered packets exceeds `window`.
///      - `RealTime`: release when `reference_time.elapsed()` is close enough to the packet's
///        output PTS (PTS + buffer). Used for latency-sensitive paths.
///
/// ## Timestamps
///
/// - Timestamp to PTS sync point is established immediately on write_packet of the first
///   packet. (`on_sender_report` can adjust it slightly)
/// - Timestamps on write are relative to `reference_time`
/// - Timestamps on read are shifted by extra buffer
/// - Unsupported scenarios:
///   - QueueTrackOffset::None + RealTime
///     - It does not make sense to support it, if jitter buffer
///       is synced to real time, normalizing it to zero is incorrect.
///
pub(crate) struct RtpJitterBuffer {
    mode: RtpJitterBufferMode,
    ntp_sync_point: Arc<RtpNtpSyncPoint>,
    input_buffer: BufferingStrategy,
    timestamp_sync: RtpTimestampSync,
    seq_num_rollover: SequenceNumberRollover,
    packets: BTreeMap<u64, JitterBufferPacket>,
    /// Next expected sequence number (last returned from `read_packet` + 1).
    next_seq_num: Option<u64>,
    on_stats_event: Box<dyn FnMut(RtpJitterBufferStatsEvent) + 'static + Send>,
}

/// We are assuming here that it is enough time to decode. Might be
/// problematic in case of B-frames, because it would require processing multiple
/// frames before
const MIN_DECODE_TIME: Duration = Duration::from_millis(80);

impl RtpJitterBuffer {
    pub fn new(
        shared_ctx: RtpJitterBufferSharedContext,
        clock_rate: u32,
        on_stats_event: Box<dyn FnMut(RtpJitterBufferStatsEvent) + 'static + Send>,
    ) -> Self {
        let RtpJitterBufferSharedContext {
            mode,
            ntp_sync_point,
            input_buffer,
        } = shared_ctx;
        let timestamp_sync = RtpTimestampSync::new(ntp_sync_point.clone(), clock_rate);

        Self {
            mode,
            ntp_sync_point,
            input_buffer,
            timestamp_sync,
            seq_num_rollover: SequenceNumberRollover::default(),
            packets: BTreeMap::new(),
            next_seq_num: None,
            on_stats_event,
        }
    }

    pub fn on_sender_report(&mut self, ntp_time: u64, rtp_timestamp: u32) {
        self.timestamp_sync
            .on_sender_report(ntp_time, rtp_timestamp);
    }

    pub fn write_packet(&mut self, packet: webrtc::rtp::packet::Packet) {
        let sequence_number = self
            .seq_num_rollover
            .rolled_sequence_number(packet.header.sequence_number);

        if let Some(last_returned) = self.next_seq_num
            && last_returned > sequence_number
        {
            debug!(sequence_number, "Packet to old. Dropping.");
            return;
        }

        (self.on_stats_event)(RtpJitterBufferStatsEvent::RtpPacketReceived);
        (self.on_stats_event)(RtpJitterBufferStatsEvent::BytesReceived(
            packet.payload.len(),
        ));

        // pts relative to reference_time in ntp_sync_point
        let pts = self
            .timestamp_sync
            .pts_from_timestamp(packet.header.timestamp);

        // We estimate buffer size here, but actual calculation and
        // packet smoothing happens when removing packet from jitter buffer.
        //
        // - Buffer needs to be estimated here because lost packet will delay popping
        // and totally pollute data
        // - PTS needs to be added after pop based on target at the time to avoid
        // reorders and minimize large jumps
        //
        self.input_buffer.on_new_packet(pts);

        trace!(packet=?packet.header, ?pts, buffer_size=self.packets.len(), "Writing packet to jitter buffer");
        self.packets
            .insert(sequence_number, JitterBufferPacket { packet, pts });
    }

    pub fn try_read_packet(&mut self) -> Option<RtpInputEvent> {
        let (first_seq_num, _first_packet) = self.packets.first_key_value()?;

        if self.next_seq_num == Some(*first_seq_num) {
            return self.read_packet();
        }

        let wait_for_next_packet = match self.mode {
            RtpJitterBufferMode::FixedWindow(window_size) => {
                let lowest_pts = self.packets.values().map(|packet| packet.pts).min()?;
                let highest_pts = self.packets.values().map(|packet| packet.pts).max()?;
                highest_pts.saturating_sub(lowest_pts) < window_size
            }
            RtpJitterBufferMode::RealTime => {
                let lowest_pts = self.packets.values().map(|packet| packet.pts).min()?;

                // TODO: if lowest pts is not first it means that we have B-frames
                //
                // It would be safer to use value based on index than constant, in the worst
                // case scenario this could be 16 frames that needs to decoded in that time
                let next_pts = lowest_pts + self.input_buffer.size();
                let reference_time = self.ntp_sync_point.reference_time;
                next_pts > reference_time.elapsed() + MIN_DECODE_TIME
            }
        };

        if wait_for_next_packet {
            return None;
        }

        self.read_packet()
    }

    pub fn read_packet(&mut self) -> Option<RtpInputEvent> {
        let first_entry = self.packets.first_entry()?;
        let seq_num = *first_entry.key();

        if let Some(next_seq_number) = self.next_seq_num
            && seq_num != next_seq_number
        {
            (self.on_stats_event)(RtpJitterBufferStatsEvent::RtpPacketLost);
            self.next_seq_num = Some(next_seq_number + 1);
            return Some(RtpInputEvent::LostPacket);
        }

        let packet = first_entry.remove();

        let timestamp = self.input_buffer.apply_offset(packet.pts);

        let reference_time = self.ntp_sync_point.reference_time;
        (self.on_stats_event)(RtpJitterBufferStatsEvent::EffectiveBuffer(
            timestamp.saturating_sub(reference_time.elapsed()),
        ));
        (self.on_stats_event)(RtpJitterBufferStatsEvent::InputBufferSize(
            self.input_buffer.size(),
        ));

        self.next_seq_num = Some(seq_num + 1);
        Some(RtpInputEvent::Packet(RtpPacket {
            packet: packet.packet,
            timestamp,
        }))
    }

    pub fn peek_next_pts(&self) -> Option<Duration> {
        let (_, packet) = self.packets.first_key_value()?;
        Some(packet.pts + self.input_buffer.size())
    }
}

#[derive(Clone)]
enum BufferingStrategy {
    FixedOffset { offset: Duration },
    LatencyOptimized(LatencyOptimizedBuffer),
}

impl fmt::Debug for BufferingStrategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FixedOffset { offset } => f
                .debug_struct("FixedOffset")
                .field("offset", offset)
                .finish(),
            Self::LatencyOptimized(_) => f.debug_tuple("LatencyOptimized").finish(),
        }
    }
}

impl BufferingStrategy {
    /// Called the moment a packet is received off the network. Records the
    /// trend observation against the *receive-time* effective buffer (so packets
    /// sitting in the jitter buffer waiting for predecessors don't get misread
    /// as emergencies) and updates the shared target size. Per-track size
    /// converges toward that target separately in `apply_offset` at pop time.
    pub fn on_new_packet(&mut self, pts: Duration) {
        match self {
            BufferingStrategy::LatencyOptimized(buffer) => buffer.on_new_packet(pts),
            BufferingStrategy::FixedOffset { .. } => (),
        }
    }

    pub fn apply_offset(&mut self, pts: Duration) -> Duration {
        match self {
            BufferingStrategy::FixedOffset { offset } => pts + *offset,
            BufferingStrategy::LatencyOptimized(buffer) => buffer.apply_offset(pts),
        }
    }

    pub fn size(&self) -> Duration {
        match self {
            BufferingStrategy::FixedOffset { offset } => *offset,
            BufferingStrategy::LatencyOptimized(buffer) => buffer.target_size(),
        }
    }
}

/// Per-track wrapper around a shared [`InnerLatencyOptimizedBuffer`]. The inner state
/// (observation history, trend resolution, target size selection) is shared across all
/// tracks of the same RTP stream via `Arc<Mutex<…>>`. Each track keeps its own `size`
/// and converges linearly toward the inner's target on every `apply_offset` call, so
/// per-track output PTS is smooth even though the target can jump.
#[derive(Clone)]
pub(crate) struct LatencyOptimizedBuffer {
    inner: Arc<Mutex<InnerLatencyOptimizedBuffer>>,
    /// Per-track buffer size. Slowly converges toward `inner.target_size`.
    size: Duration,
    /// Per-track largest PTS seen — drives the linear convergence's stream_delta.
    max_pts: Option<Duration>,
}

impl LatencyOptimizedBuffer {
    /// Linear convergence rate. The per-track `size` moves toward `target_size` by at
    /// most `stream_delta * CONVERGENCE_RATE` per call (~50 ms per second of stream
    /// time). Slow enough that target jumps don't translate into per-track jolts.
    const CONVERGENCE_RATE: f64 = 0.04;
    /// If the target diverges from the per-track size by more than this, snap
    /// directly to the target instead of rate-limiting. Avoids long catch-up
    /// after a `JumpGrow` or any other large target shift.
    const SNAP_THRESHOLD: Duration = Duration::from_millis(200);

    fn new(reference_time: Instant) -> Self {
        let inner = InnerLatencyOptimizedBuffer::new(reference_time);
        let size = inner.target_size;
        Self {
            inner: Arc::new(Mutex::new(inner)),
            size,
            max_pts: None,
        }
    }

    /// Decides the size of the buffer, but the change will be applied
    /// immediately on packets removed from jitter buffer
    fn on_new_packet(&mut self, pts: Duration) {
        self.inner.lock().unwrap().on_new_packet(pts);
    }

    fn apply_offset(&mut self, pts: Duration) -> Duration {
        let target_size = self.inner.lock().unwrap().target_size;

        let stream_delta = match self.max_pts {
            Some(prev_max_pts) => {
                self.max_pts = Some(Duration::max(prev_max_pts, pts));
                pts.saturating_sub(prev_max_pts)
            }
            None => {
                self.max_pts = Some(pts);
                Duration::ZERO
            }
        };
        if target_size.abs_diff(self.size) > Self::SNAP_THRESHOLD {
            self.size = target_size;
        } else {
            let max_step = stream_delta.mul_f64(Self::CONVERGENCE_RATE);
            self.size = if target_size > self.size {
                Duration::min(self.size + max_step, target_size)
            } else {
                Duration::max(self.size.saturating_sub(max_step), target_size)
            };
        }
        pts + self.size
    }

    fn target_size(&self) -> Duration {
        // lower size will mean that packet will be popped too early, which
        // is still preferred outcome compared to sending it too late
        Duration::min(self.inner.lock().unwrap().target_size, self.size)
    }
}

/// Shared trend state behind [`LatencyOptimizedBuffer`]. Holds the observation history,
/// resolves the trend, and drives the *target* buffer size. Per-track wrappers read this
/// target and converge their own `size` toward it.
///
/// All adjustments are driven by stream time (PTS deltas), never by wall clock:
/// - Every incoming packet — including out-of-order packets with `pts <= max_pts` — re-evaluates
///   the [`LatencyTrend`] state from its own `effective_buffer`. Late packets observed near the
///   edge of the buffer still pull the trend toward Grow.
/// - The target size only changes when a packet advances `max_pts`. The change is scaled by the
///   PTS delta since the previous max, so the rate of change is bounded in stream-time terms.
///
/// `effective_buffer = (pts + target_size) - reference_time.elapsed()` represents how much
/// margin a packet has before reaching the queue. Wall clock is only consulted to compute that
/// margin; it never gates the adjustment cadence.
struct InnerLatencyOptimizedBuffer {
    reference_time: Instant,
    target_size: Duration,

    /// Largest PTS observed so far. Target-size changes only run when a new packet exceeds it.
    max_pts: Option<Duration>,
    /// PTS at which the last `+GROW_JUMP` was applied. Drives the rate-limit on
    /// jumps; not used as a zone observation.
    last_jump_pts: Option<Duration>,

    /// Largest PTS observed with each grow-side zone classification. The effective
    /// `trend` returns the most-grow-leaning zone whose timestamp still lies within
    /// `TREND_WINDOW` of `pts`. A grow signal latches for the window length so that
    /// a recent Grow outranks a newer Shrink.
    last_jump_grow_pts: Option<Duration>,
    last_grow_fast_pts: Option<Duration>,
    last_grow_pts: Option<Duration>,
    last_stable_pts: Option<Duration>,

    /// PTS at which the current uninterrupted Shrink-or-stronger streak began, or
    /// `None` if the streak was just broken by a non-shrink observation. The trend
    /// only resolves to Shrink once `pts - shrink_streak_start >= TREND_WINDOW`,
    /// and the streak is intentionally preserved across successive shrink scales
    /// so we don't have to re-accumulate the window after every shrink.
    shrink_streak_start: Option<Duration>,
    /// Same idea for ShrinkFast — only ShrinkFast observations keep this streak
    /// alive; a plain Shrink (or any grow-side observation) clears it.
    shrink_fast_streak_start: Option<Duration>,

    thresholds: LatencyThresholds,
}

struct LatencyThresholds {
    /// Above this effective_buffer the buffer shrinks at the fast rate.
    shrink_fast: Duration,
    /// Above this effective_buffer the buffer shrinks at the slow rate.
    shrink: Duration,
    /// Below this effective_buffer the buffer grows proportionally at the slow rate.
    grow: Duration,
    /// Below this effective_buffer the buffer grows proportionally at the fast rate.
    grow_fast: Duration,
    /// Below this effective_buffer the buffer must jump up immediately.
    grow_jump: Duration,
}

impl InnerLatencyOptimizedBuffer {
    /// Slow shrink rate of stream time.
    const SHRINK_RATE: f64 = 0.005;
    /// Fast shrink rate per second of stream time.
    const SHRINK_FAST_RATE: f64 = 0.02;
    /// Slow grow rate per second of stream time.
    const GROW_RATE: f64 = 0.01;
    /// Fast grow rate per second of stream time.
    const GROW_FAST_RATE: f64 = 0.03;
    /// Fixed amount applied when the effective buffer drops below `grow_jump`.
    const GROW_JUMP: Duration = Duration::from_millis(1000);
    /// Minimum stream-time spacing between two grow jumps.
    const GROW_JUMP_INTERVAL: Duration = Duration::from_millis(3000);
    /// Stream-time window during which a per-zone observation still influences the
    /// effective trend. Equivalent to "no contradicting trend in N seconds" — when this
    /// window expires for a zone its observation is forgotten.
    const TREND_WINDOW: Duration = Duration::from_secs(10);

    fn new(reference_time: Instant) -> Self {
        // Stable zone spans grow..shrink (240..320ms).
        let thresholds = LatencyThresholds {
            grow_jump: Duration::from_millis(80),
            grow_fast: Duration::from_millis(160),
            grow: Duration::from_millis(240),
            shrink: Duration::from_millis(320),
            shrink_fast: Duration::from_millis(2000),
        };
        Self {
            reference_time,
            target_size: Duration::from_millis(600),
            max_pts: None,
            last_jump_pts: None,
            last_jump_grow_pts: None,
            last_grow_fast_pts: None,
            last_grow_pts: None,
            last_stable_pts: None,
            shrink_streak_start: None,
            shrink_fast_streak_start: None,
            thresholds,
        }
    }

    /// Called the moment a packet is received. Classifies it against the
    /// *receive-time* effective buffer, records the observation, resolves the
    /// trend, and updates `target_size`. Per-track wrappers read the updated
    /// target separately in `apply_offset`.
    fn on_new_packet(&mut self, pts: Duration) {
        let next_pts = pts + self.target_size;
        let effective_buffer = next_pts.saturating_sub(self.reference_time.elapsed());
        let observed = LatencyTrend::from_effective_buffer(effective_buffer, &self.thresholds);
        trace!(
            ?effective_buffer,
            target_size=?self.target_size,
            ?observed,
            "Latency trend observation",
        );
        self.record_observation(observed, pts);

        // Advance `max_pts` and compute the stream-time delta. For out-of-order or
        // first packets the delta is zero, which makes every proportional op a noop.
        let stream_delta = match self.max_pts {
            Some(prev_max_pts) => {
                self.max_pts = Some(Duration::max(prev_max_pts, pts));
                pts.saturating_sub(prev_max_pts)
            }
            None => {
                self.max_pts = Some(pts);
                Duration::ZERO
            }
        };

        match self.resolve_trend(pts) {
            LatencyTrend::ShrinkFast => self.scale_target(-Self::SHRINK_FAST_RATE, stream_delta),
            LatencyTrend::Shrink => self.scale_target(-Self::SHRINK_RATE, stream_delta),
            LatencyTrend::Stable => {}
            LatencyTrend::Grow => self.scale_target(Self::GROW_RATE, stream_delta),
            LatencyTrend::GrowFast => self.scale_target(Self::GROW_FAST_RATE, stream_delta),
            LatencyTrend::JumpGrow => self.try_grow_jump(pts),
        }
    }

    fn record_observation(&mut self, observed: LatencyTrend, pts: Duration) {
        let grow_slot = match observed {
            LatencyTrend::JumpGrow => Some(&mut self.last_jump_grow_pts),
            LatencyTrend::GrowFast => Some(&mut self.last_grow_fast_pts),
            LatencyTrend::Grow => Some(&mut self.last_grow_pts),
            LatencyTrend::Stable => Some(&mut self.last_stable_pts),
            LatencyTrend::Shrink | LatencyTrend::ShrinkFast => None,
        };
        if let Some(slot) = grow_slot {
            *slot = Some(match *slot {
                Some(prev) => Duration::max(prev, pts),
                None => pts,
            });
        }

        // Shrink streaks accumulate continuous evidence; any contradicting
        // observation breaks the streak so the next shrink has to start over.
        match observed {
            LatencyTrend::ShrinkFast => {
                self.shrink_streak_start.get_or_insert(pts);
                self.shrink_fast_streak_start.get_or_insert(pts);
            }
            LatencyTrend::Shrink => {
                self.shrink_streak_start.get_or_insert(pts);
                self.shrink_fast_streak_start = None;
            }
            LatencyTrend::Stable
            | LatencyTrend::Grow
            | LatencyTrend::GrowFast
            | LatencyTrend::JumpGrow => {
                self.shrink_streak_start = None;
                self.shrink_fast_streak_start = None;
            }
        }
    }

    /// Walk the zones from most-grow to most-shrink. Grow-side zones latch on a single
    /// recent observation. Shrink-side zones require a continuous streak of at least
    /// `TREND_WINDOW` of stream time, so an isolated shrink signal never fires.
    fn resolve_trend(&self, pts: Duration) -> LatencyTrend {
        let recent = |last: Option<Duration>| -> bool {
            last.is_some_and(|p| pts.saturating_sub(p) < Self::TREND_WINDOW)
        };
        let streak_ready = |start: Option<Duration>| -> bool {
            start.is_some_and(|s| pts.saturating_sub(s) >= Self::TREND_WINDOW)
        };
        if recent(self.last_jump_grow_pts) {
            LatencyTrend::JumpGrow
        } else if recent(self.last_grow_fast_pts) {
            LatencyTrend::GrowFast
        } else if recent(self.last_grow_pts) {
            LatencyTrend::Grow
        } else if recent(self.last_stable_pts) {
            LatencyTrend::Stable
        } else if streak_ready(self.shrink_fast_streak_start) {
            LatencyTrend::ShrinkFast
        } else if streak_ready(self.shrink_streak_start) {
            LatencyTrend::Shrink
        } else {
            LatencyTrend::Stable
        }
    }

    fn scale_target(&mut self, rate: f64, stream_delta: Duration) {
        if stream_delta == Duration::ZERO {
            return;
        }
        let factor = 1.0 + rate * stream_delta.as_secs_f64();
        let new_size = self.target_size.mul_f64(factor.max(0.0));
        trace!(
            ?new_size,
            size_diff_secs = new_size.as_secs_f64() - self.target_size.as_secs_f64(),
            rate,
            "Scale latency optimized target"
        );
        self.target_size = new_size;
        self.reset_grow_observations();
    }

    fn try_grow_jump(&mut self, pts: Duration) {
        self.reset_grow_observations();
        if let Some(last) = self.last_jump_pts
            && pts.saturating_sub(last) < Self::GROW_JUMP_INTERVAL
        {
            return;
        }

        self.last_jump_pts = Some(pts);
        let new_size = self.target_size + Self::GROW_JUMP;
        debug!(?new_size, "Grow latency optimized target (jump)");
        self.target_size = new_size;
    }

    /// All grow-side observations were measured against the old target
    /// size; after a target change they no longer reflect reality. Stable is kept
    /// because its zone remains valid after either direction of change and is what
    /// blocks spurious cross-side firing via the priority resolver. Shrink streaks
    /// are *not* reset here: continuing a shrink streak across successive shrink
    /// scales is the whole point of the streak — we don't want to re-accumulate
    /// `TREND_WINDOW` of evidence after every shrink.
    fn reset_grow_observations(&mut self) {
        self.last_jump_grow_pts = None;
        self.last_grow_fast_pts = None;
        self.last_grow_pts = None;
    }
}

/// State machine deciding which direction the buffer should move on the next PTS-advancing
/// packet. Re-derived per packet from `effective_buffer`.
#[derive(Debug, Clone, Copy)]
enum LatencyTrend {
    /// effective_buffer < grow_jump — must add a fixed jump.
    JumpGrow,
    /// effective_buffer in [grow_jump, grow_fast) — grow proportionally at the fast rate.
    GrowFast,
    /// effective_buffer in [grow_fast, grow) — grow proportionally at the slow rate.
    Grow,
    /// effective_buffer in [grow, shrink) — leave alone.
    Stable,
    /// effective_buffer in [shrink, shrink_fast) — shrink proportionally at the slow rate.
    Shrink,
    /// effective_buffer >= shrink_fast — shrink proportionally at the fast rate.
    ShrinkFast,
}

impl LatencyTrend {
    fn from_effective_buffer(effective_buffer: Duration, t: &LatencyThresholds) -> Self {
        if effective_buffer >= t.shrink_fast {
            Self::ShrinkFast
        } else if effective_buffer >= t.shrink {
            Self::Shrink
        } else if effective_buffer >= t.grow {
            Self::Stable
        } else if effective_buffer >= t.grow_fast {
            Self::Grow
        } else if effective_buffer >= t.grow_jump {
            Self::GrowFast
        } else {
            Self::JumpGrow
        }
    }
}
