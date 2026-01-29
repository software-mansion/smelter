use std::time::Duration;

use crate::{PipelineCtx, pipeline::utils::input_buffer::InputBuffer};

pub(crate) struct RtmpStreamState {
    queue_start_time: std::time::Instant,
    buffer: InputBuffer,
    first_packet_offset: Option<Duration>,
}

impl RtmpStreamState {
    pub(crate) fn new(ctx: &PipelineCtx, buffer: InputBuffer) -> Self {
        Self {
            queue_start_time: ctx.queue_sync_point,
            buffer,
            first_packet_offset: None,
        }
    }

    pub(crate) fn pts_dts_from_timestamps(
        &mut self,
        pts_ms: i64,
        dts_ms: i64,
    ) -> (Duration, Option<Duration>) {
        let pts = Duration::from_millis(pts_ms.max(0) as u64);
        let dts = Duration::from_millis(dts_ms.max(0) as u64);

        let offset = self
            .first_packet_offset
            .get_or_insert_with(|| self.queue_start_time.elapsed().saturating_sub(pts));

        let pts = pts + *offset;
        let dts = dts + *offset;

        self.buffer.recalculate_buffer(pts);
        (pts + self.buffer.size(), Some(dts))
    }
}
