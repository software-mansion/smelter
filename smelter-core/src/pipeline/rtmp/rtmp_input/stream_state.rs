use std::time::{Duration, Instant};

use ffmpeg_next::Packet;

use crate::pipeline::utils::input_buffer::InputBuffer;

pub(super) struct StreamState {
    queue_start_time: Instant,
    buffer: InputBuffer,
    time_base: ffmpeg_next::Rational,

    first_packet_offset: Option<Duration>,
}

impl StreamState {
    pub(super) fn new(
        queue_start_time: Instant,
        time_base: ffmpeg_next::Rational,
        buffer: InputBuffer,
    ) -> Self {
        Self {
            queue_start_time,
            time_base,
            buffer,

            first_packet_offset: None,
        }
    }

    pub(super) fn pts_dts_from_packet(&mut self, packet: &Packet) -> (Duration, Option<Duration>) {
        let pts = timestamp_to_duration(packet.pts().unwrap_or(0), self.time_base);
        let dts = packet
            .dts()
            .map(|dts| timestamp_to_duration(dts, self.time_base));

        let offset = self
            .first_packet_offset
            .get_or_insert_with(|| self.queue_start_time.elapsed().saturating_sub(pts));

        let pts = pts + *offset;
        let dts = dts.map(|dts| dts + *offset);

        self.buffer.recalculate_buffer(pts);
        (pts + self.buffer.size(), dts)
    }
}

fn timestamp_to_duration(timestamp: i64, time_base: ffmpeg_next::Rational) -> Duration {
    let secs = f64::max(timestamp as f64, 0.0) * time_base.numerator() as f64
        / time_base.denominator() as f64;
    Duration::from_secs_f64(secs)
}
