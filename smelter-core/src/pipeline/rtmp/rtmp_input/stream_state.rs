use std::time::{Duration, Instant};

use ffmpeg_next::Packet;

use crate::pipeline::utils::input_buffer::InputBuffer;

pub(super) struct StreamState {
    queue_start_time: Instant,
    buffer: InputBuffer,
    time_base: ffmpeg_next::Rational,

    reference_pts_and_timestamp: Option<(Duration, f64)>,
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

            reference_pts_and_timestamp: None,
        }
    }

    pub(super) fn pts_dts_from_packet(&mut self, packet: &Packet) -> (Duration, Option<Duration>) {
        let pts_timestamp = packet.pts().unwrap_or(0) as f64;
        let dts_timestamp = packet.dts().map(|dts| dts as f64);

        let (reference_pts, reference_timestamp) = *self
            .reference_pts_and_timestamp
            .get_or_insert_with(|| (self.queue_start_time.elapsed(), pts_timestamp));

        let pts_diff_secs = timestamp_to_secs(pts_timestamp - reference_timestamp, self.time_base);
        let pts =
            Duration::from_secs_f64(reference_pts.as_secs_f64() + f64::max(pts_diff_secs, 0.0));

        let dts = dts_timestamp.map(|dts| {
            Duration::from_secs_f64(f64::max(timestamp_to_secs(dts, self.time_base), 0.0))
        });

        self.buffer.recalculate_buffer(pts);
        (pts + self.buffer.size(), dts)
    }
}

fn timestamp_to_secs(timestamp: f64, time_base: ffmpeg_next::Rational) -> f64 {
    f64::max(timestamp, 0.0) * time_base.numerator() as f64 / time_base.denominator() as f64
}
