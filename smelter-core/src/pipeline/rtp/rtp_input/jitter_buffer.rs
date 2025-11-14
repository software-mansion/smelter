use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, Instant},
};

use tracing::{debug, trace, warn};

use crate::pipeline::{
    rtp::{
        RtpPacket,
        rtp_input::rtcp_sync::{RtpNtpSyncPoint, RtpTimestampSync},
    },
    utils::input_buffer::InputBuffer,
};

use crate::prelude::*;

struct JitterBufferPacket {
    packet: webrtc::rtp::packet::Packet,
    pts: Duration,
    received_at: Instant,
}

#[derive(Debug, Clone)]
pub(crate) struct RtpJitterBufferInitOptions {
    mode: RtpJitterBufferMode,
    buffer: InputBuffer,
    ntp_sync_point: Arc<RtpNtpSyncPoint>,
}

impl RtpJitterBufferInitOptions {
    pub fn new(ctx: &Arc<PipelineCtx>, opts: RtpJitterBufferOptions) -> Self {
        Self {
            mode: opts.mode,
            buffer: InputBuffer::new(ctx, opts.buffer),
            ntp_sync_point: RtpNtpSyncPoint::new(),
        }
    }
}

pub(crate) struct RtpJitterBuffer {
    mode: RtpJitterBufferMode,
    input_buffer: InputBuffer,
    timestamp_sync: RtpTimestampSync,
    seq_num_rollover: SequenceNumberRollover,
    packets: BTreeMap<u64, JitterBufferPacket>,
    /// Last sequence number returned from `pop_packets`
    previous_seq_num: Option<u64>,
    queue_sync_point: Instant,
    on_stats_event: Box<dyn FnMut(RtpJitterBufferStatsEvent) + 'static + Send>,
}

/// We are assuming here that it is enough time to decode. Might be
/// problematic in case of B-frames, because it would require processing multiple
/// frames before
const MIN_DECODE_TIME: Duration = Duration::from_millis(30);

impl RtpJitterBuffer {
    pub fn new(
        ctx: &Arc<PipelineCtx>,
        opts: RtpJitterBufferInitOptions,
        clock_rate: u32,
        on_stats_event: Box<dyn FnMut(RtpJitterBufferStatsEvent) + 'static + Send>,
    ) -> Self {
        let timestamp_sync =
            RtpTimestampSync::new(ctx.queue_sync_point, opts.ntp_sync_point, clock_rate);

        Self {
            mode: opts.mode,
            input_buffer: opts.buffer,
            timestamp_sync,
            seq_num_rollover: SequenceNumberRollover::default(),
            packets: BTreeMap::new(),
            previous_seq_num: None,
            queue_sync_point: ctx.queue_sync_point,
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

        if let Some(last_returned) = self.previous_seq_num
            && last_returned > sequence_number
        {
            debug!(sequence_number, "Packet to old. Dropping.");
            return;
        }

        (self.on_stats_event)(RtpJitterBufferStatsEvent::RtpPacketReceived);

        let pts = self
            .timestamp_sync
            .pts_from_timestamp(packet.header.timestamp);

        self.input_buffer.recalculate_buffer(pts);

        trace!(packet=?packet.header, ?pts, buffer_size=self.packets.len(), "Writing packet to jitter buffer");
        self.packets.insert(
            sequence_number,
            JitterBufferPacket {
                packet,
                pts,
                received_at: Instant::now(),
            },
        );
    }

    pub fn pop_packet(&mut self) -> Option<RtpPacket> {
        let (first_seq_num, first_packet) = self.packets.first_key_value()?;

        // check if next sequence_number is ready (and return it if it is)
        match self.previous_seq_num {
            Some(previous_seq_num) if previous_seq_num + 1 == *first_seq_num => (),
            None => (),
            Some(previous_seq_num) => {
                match self.mode {
                    RtpJitterBufferMode::Fixed(duration) => {
                        // if input is required or offset is set, we can assume that we can wait
                        // a while, but it should not depend on queue clock
                        if first_packet.received_at.elapsed() < duration {
                            return None;
                        }
                    }
                    RtpJitterBufferMode::QueueBased => {
                        let lowest_pts = self.packets.values().map(|packet| packet.pts).min()?;

                        // TODO: if lowest pts is not first it means that we have B-frames
                        //
                        // It would be safer to use value based on index than constant, in the worst
                        // case scenario this could be 16 frames that needs to decoded in that time
                        let should_pop = lowest_pts + self.input_buffer.size()
                            < self.queue_sync_point.elapsed() + MIN_DECODE_TIME;
                        if !should_pop {
                            return None;
                        }
                    }
                }
                (self.on_stats_event)(RtpJitterBufferStatsEvent::RtpPacketLost(
                    first_seq_num.saturating_sub(previous_seq_num),
                ));
            }
        };

        let (first_seq_num, first_packet) = self.packets.pop_first()?;
        let timestamp = first_packet.pts + self.input_buffer.size();

        // effective buffer relative to queue clock
        (self.on_stats_event)(RtpJitterBufferStatsEvent::EffectiveBuffer(
            timestamp.saturating_sub(self.queue_sync_point.elapsed()),
        ));
        (self.on_stats_event)(RtpJitterBufferStatsEvent::InputBufferSize(
            self.input_buffer.size(),
        ));

        self.previous_seq_num = Some(first_seq_num);
        Some(RtpPacket {
            packet: first_packet.packet,
            timestamp,
        })
    }
}

#[derive(Debug, Default)]
struct SequenceNumberRollover {
    rollover_count: u64,
    last_value: Option<u16>,
}

impl SequenceNumberRollover {
    fn rolled_sequence_number(&mut self, sequence_number: u16) -> u64 {
        let last_value = *self.last_value.get_or_insert(sequence_number);

        let diff = u16::abs_diff(last_value, sequence_number);
        if diff >= u16::MAX / 2 {
            if last_value > sequence_number {
                self.rollover_count += 1;
            } else {
                // We received a packet from before the rollover, so we need to decrement the count
                self.rollover_count = self.rollover_count.saturating_sub(1);
            }
        }
        self.last_value = Some(sequence_number);

        (self.rollover_count * (u16::MAX as u64 + 1)) + sequence_number as u64
    }
}
