use std::time::{Duration, Instant};

use ffmpeg_next::Packet;
use tracing::debug;

use crate::pipeline::utils::input_buffer::InputBuffer;

pub(super) struct StreamState {
    queue_start_time: Instant,
    buffer: InputBuffer,
    time_base: ffmpeg_next::Rational,

    reference_pts_and_timestamp: Option<(Duration, f64)>,

    pts_discontinuity: DiscontinuityState,
    dts_discontinuity: DiscontinuityState,
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
            pts_discontinuity: DiscontinuityState::new(false, time_base),
            dts_discontinuity: DiscontinuityState::new(true, time_base),
        }
    }

    pub(super) fn pts_dts_from_packet(&mut self, packet: &Packet) -> (Duration, Option<Duration>) {
        let pts_timestamp = packet.pts().unwrap_or(0) as f64;
        let dts_timestamp = packet.dts().map(|dts| dts as f64);
        let packet_duration = packet.duration() as f64;

        self.pts_discontinuity
            .detect_discontinuity(pts_timestamp, packet_duration);
        if let Some(dts) = dts_timestamp {
            self.dts_discontinuity
                .detect_discontinuity(dts, packet_duration);
        }

        let pts_timestamp = pts_timestamp + self.pts_discontinuity.offset;
        let dts_timestamp = dts_timestamp.map(|dts| dts + self.dts_discontinuity.offset);

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

struct DiscontinuityState {
    check_timestamp_monotonicity: bool,
    time_base: ffmpeg_next::Rational,
    prev_timestamp: Option<f64>,
    next_predicted_timestamp: Option<f64>,
    offset: f64,
}

impl DiscontinuityState {
    /// (10s) This value was picked arbitrarily but it's quite conservative.
    const DISCONTINUITY_THRESHOLD: f64 = 10.0;

    fn new(check_timestamp_monotonicity: bool, time_base: ffmpeg_next::Rational) -> Self {
        Self {
            check_timestamp_monotonicity,
            time_base,
            prev_timestamp: None,
            next_predicted_timestamp: None,
            offset: 0.0,
        }
    }

    fn detect_discontinuity(&mut self, timestamp: f64, packet_duration: f64) {
        let (Some(prev_timestamp), Some(next_timestamp)) =
            (self.prev_timestamp, self.next_predicted_timestamp)
        else {
            self.prev_timestamp = Some(timestamp);
            self.next_predicted_timestamp = Some(timestamp + packet_duration);
            return;
        };

        // Detect discontinuity
        let timestamp_delta =
            timestamp_to_secs(f64::abs(next_timestamp - timestamp), self.time_base);

        let is_discontinuity = timestamp_delta >= Self::DISCONTINUITY_THRESHOLD
            || (self.check_timestamp_monotonicity && prev_timestamp > timestamp);
        if is_discontinuity {
            debug!("Discontinuity detected: {prev_timestamp} -> {timestamp}");
            self.offset += next_timestamp - timestamp;
        }

        self.prev_timestamp = Some(timestamp);
        self.next_predicted_timestamp = Some(timestamp + packet_duration);
    }
}

fn timestamp_to_secs(timestamp: f64, time_base: ffmpeg_next::Rational) -> f64 {
    f64::max(timestamp, 0.0) * time_base.numerator() as f64 / time_base.denominator() as f64
}
