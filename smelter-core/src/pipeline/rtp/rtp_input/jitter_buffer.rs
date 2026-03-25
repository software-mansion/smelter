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
    /// If I receive packet for sample `x`, then don't wait
    /// for missing packets older than `(x-size)`
    FixedWindow(Duration),
    /// Packet needs to be returned from buffer before instant.elapsed() gets bigger
    /// than pts. (with some extra fixed buffer e.g. 80ms)
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
                RtpJitterBufferMode::RealTime => BufferingStrategy::LatencyOptimized(Arc::new(
                    Mutex::new(LatencyOptimizedBuffer::new(ctx, reference_time)),
                )),
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
    shared_ctx: RtpJitterBufferSharedContext,
    timestamp_sync: RtpTimestampSync,
    seq_num_rollover: SequenceNumberRollover,
    packets: BTreeMap<u64, JitterBufferPacket>,
    /// Last sequence number returned from `read_packet`
    next_seq_num: Option<u64>,
    on_stats_event: Box<dyn FnMut(RtpJitterBufferStatsEvent) + 'static + Send>,
}

/// We are assuming here that it is enough time to decode. Might be
/// problematic in case of B-frames, because it would require processing multiple
/// frames before
const MIN_DECODE_TIME: Duration = Duration::from_millis(30);

impl RtpJitterBuffer {
    pub fn new(
        shared_ctx: RtpJitterBufferSharedContext,
        clock_rate: u32,
        on_stats_event: Box<dyn FnMut(RtpJitterBufferStatsEvent) + 'static + Send>,
    ) -> Self {
        let timestamp_sync = RtpTimestampSync::new(shared_ctx.ntp_sync_point.clone(), clock_rate);

        Self {
            shared_ctx,
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

        self.shared_ctx.input_buffer.on_new_packet(pts);

        trace!(packet=?packet.header, ?pts, buffer_size=self.packets.len(), "Writing packet to jitter buffer");
        self.packets
            .insert(sequence_number, JitterBufferPacket { packet, pts });
    }

    pub fn try_read_packet(&mut self) -> Option<RtpInputEvent> {
        let (first_seq_num, _first_packet) = self.packets.first_key_value()?;

        if self.next_seq_num == Some(*first_seq_num) {
            return self.read_packet();
        }

        let wait_for_next_packet = match self.shared_ctx.mode {
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
                let next_pts = lowest_pts + self.shared_ctx.input_buffer.size();
                let reference_time = self.shared_ctx.ntp_sync_point.reference_time;
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

        let input_buffer_size = self.shared_ctx.input_buffer.size();
        let timestamp = packet.pts + input_buffer_size;

        let reference_time = self.shared_ctx.ntp_sync_point.reference_time;
        (self.on_stats_event)(RtpJitterBufferStatsEvent::EffectiveBuffer(
            timestamp.saturating_sub(reference_time.elapsed()),
        ));
        (self.on_stats_event)(RtpJitterBufferStatsEvent::InputBufferSize(
            input_buffer_size,
        ));

        self.next_seq_num = Some(seq_num + 1);
        Some(RtpInputEvent::Packet(RtpPacket {
            packet: packet.packet,
            timestamp,
        }))
    }

    pub fn peek_next_pts(&self) -> Option<Duration> {
        let (_, packet) = self.packets.first_key_value()?;
        Some(packet.pts + self.shared_ctx.input_buffer.size())
    }
}

#[derive(Clone)]
enum BufferingStrategy {
    FixedOffset { offset: Duration },
    LatencyOptimized(Arc<Mutex<LatencyOptimizedBuffer>>),
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
    pub fn on_new_packet(&self, pts: Duration) {
        match self {
            BufferingStrategy::LatencyOptimized(buffer) => {
                buffer.lock().unwrap().on_new_packet(pts)
            }
            BufferingStrategy::FixedOffset { .. } => (),
        }
    }

    pub fn size(&self) -> Duration {
        match self {
            BufferingStrategy::FixedOffset { offset } => *offset,
            BufferingStrategy::LatencyOptimized(buffer) => buffer.lock().unwrap().dynamic_buffer,
        }
    }
}

/// Buffer intended for low latency inputs, if input stream is not delivered on time,
/// it quickly increases. However, when buffer is stable for some time it starts to shrink to
/// minimize the latency.
pub(crate) struct LatencyOptimizedBuffer {
    reference_time: Instant,
    state: LatencyOptimizedBufferState,
    dynamic_buffer: Duration,

    /// effective_buffer = next_pts - queue_sync_point.elapsed()
    /// Estimates how much time packet has to reach the queue.

    /// If effective_buffer is above this threshold for a period of time, aggressively shrink
    /// the buffer.
    shrink_threshold_1: Duration,
    /// If effective_buffer is above this threshold for a period of time, shrink the buffer
    /// quickly
    shrink_threshold_2: Duration,
    /// If effective_buffer is above this threshold for a period of time, slowly shrink the buffer.
    max_desired_buffer: Duration,
    /// If effective_buffer is below this value, slowly increase the buffer with every packet.
    min_desired_buffer: Duration,
    /// If effective_buffer is below this threshold, aggressively and immediately increase the buffer.
    grow_threshold: Duration,
}

impl LatencyOptimizedBuffer {
    fn new(ctx: &PipelineCtx, reference_time: Instant) -> Self {
        // As a result for default numbers if effective_buffer is between 80ms and 240ms, no
        // adjustment/optimization will be triggered
        let grow_threshold = ctx.default_buffer_duration;
        let min_desired_buffer = grow_threshold + ctx.default_buffer_duration;
        let max_desired_buffer = min_desired_buffer + ctx.default_buffer_duration;
        let shrink_threshold_2 = max_desired_buffer + Duration::from_millis(400);
        let shrink_threshold_1 = shrink_threshold_2 + Duration::from_millis(400);
        Self {
            reference_time,
            dynamic_buffer: ctx.default_buffer_duration,
            state: LatencyOptimizedBufferState::Ok,

            grow_threshold,
            min_desired_buffer,
            max_desired_buffer,
            shrink_threshold_1,
            shrink_threshold_2,
        }
    }

    /// pts is a value relative to time elapsed from reference_time.
    /// If `reference_time.elapsed() == pts` that means that effective buffer is zero
    fn on_new_packet(&mut self, pts: Duration) {
        // Increment duration is larger than decrement, because when buffer is too small
        // we don't have much time to adjust to a difference.
        const INCREMENT_DURATION: Duration = Duration::from_micros(500);
        const SMALL_DECREMENT_DURATION: Duration = Duration::from_micros(200);
        const LARGE_DECREMENT_DURATION: Duration = Duration::from_micros(500);

        // Duration that defines at what point we can consider state stable enough
        // to consider shrinking the buffer
        const STABLE_STATE_DURATION: Duration = Duration::from_secs(10);

        let reference_time = self.reference_time;
        let next_pts = pts + self.dynamic_buffer;
        trace!(
            effective_buffer=?next_pts.saturating_sub(reference_time.elapsed()),
            dynamic_buffer=?self.dynamic_buffer,
            ?pts
        );

        if next_pts > reference_time.elapsed() + self.shrink_threshold_1 {
            let first_pts = self.state.set_too_large(next_pts);
            if next_pts.saturating_sub(first_pts) > STABLE_STATE_DURATION {
                self.dynamic_buffer = self
                    .dynamic_buffer
                    .saturating_sub(self.dynamic_buffer / 1000);
            }
        } else if next_pts > reference_time.elapsed() + self.shrink_threshold_2 {
            let first_pts = self.state.set_too_large(next_pts);
            if next_pts.saturating_sub(first_pts) > STABLE_STATE_DURATION {
                self.dynamic_buffer = self.dynamic_buffer.saturating_sub(LARGE_DECREMENT_DURATION);
            }
        } else if next_pts > reference_time.elapsed() + self.max_desired_buffer {
            let first_pts = self.state.set_too_large(next_pts);
            if next_pts.saturating_sub(first_pts) > STABLE_STATE_DURATION {
                self.dynamic_buffer = self.dynamic_buffer.saturating_sub(SMALL_DECREMENT_DURATION);
            }
        } else if next_pts > reference_time.elapsed() + self.min_desired_buffer {
            self.state.set_ok();
        } else if next_pts > reference_time.elapsed() + self.grow_threshold {
            trace!(
                old=?self.dynamic_buffer,
                new=?self.dynamic_buffer + INCREMENT_DURATION,
                "Increase latency optimized buffer"
            );
            self.state.set_too_small();
            self.dynamic_buffer += INCREMENT_DURATION;
        } else {
            let new_buffer =
                (reference_time.elapsed() + self.max_desired_buffer).saturating_sub(pts);
            debug!(
                old=?self.dynamic_buffer,
                new=?new_buffer,
                "Increase latency optimized buffer (force)"
            );
            self.state.set_too_small();
            // adjust buffer so:
            // pts + self.dynamic_buffer == self.sync_point.elapsed() + self.max_desired_buffer
            self.dynamic_buffer = new_buffer
        }
    }
}

enum LatencyOptimizedBufferState {
    Ok,
    TooSmall,
    TooLarge { first_pts: Duration },
}

impl LatencyOptimizedBufferState {
    fn set_too_large(&mut self, pts: Duration) -> Duration {
        match &self {
            LatencyOptimizedBufferState::TooLarge { first_pts } => *first_pts,
            _ => {
                *self = LatencyOptimizedBufferState::TooLarge { first_pts: pts };
                pts
            }
        }
    }

    fn set_too_small(&mut self) {
        *self = LatencyOptimizedBufferState::TooSmall
    }

    fn set_ok(&mut self) {
        *self = LatencyOptimizedBufferState::Ok
    }
}
