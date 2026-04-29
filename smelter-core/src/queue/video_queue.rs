use tracing::debug;

use std::{collections::HashMap, time::Duration};

use crate::queue::{QueueContext, QueueVideoOutput};

use crate::prelude::*;

use super::queue_input::WeakQueueInput;

pub struct VideoQueue {
    queue_ctx: QueueContext,
    inputs: HashMap<InputId, WeakQueueInput>,
    ahead_of_time_processing: bool,
}

impl VideoQueue {
    pub fn new(queue_ctx: QueueContext, ahead_of_time_processing: bool) -> Self {
        VideoQueue {
            inputs: HashMap::new(),
            queue_ctx,
            ahead_of_time_processing,
        }
    }

    pub fn add_input(&mut self, input_id: &InputId, weak: WeakQueueInput) {
        self.inputs.insert(input_id.clone(), weak);
    }

    pub fn remove_input(&mut self, input_id: &InputId) {
        self.inputs.remove(input_id);
    }

    /// Gets frames closest to buffer pts. It does not check whether input is ready
    /// or not. It should not be called before pipeline start.
    pub(super) fn get_frames_batch(
        &mut self,
        buffer_pts: Duration,
        queue_start_pts: Duration,
    ) -> QueueVideoOutput {
        let mut required = false;
        let frames = self
            .inputs
            .iter()
            .filter_map(|(input_id, weak)| {
                let frame_event =
                    weak.video(|input| input.get_frame(buffer_pts, queue_start_pts))??;
                required = required || frame_event.required;
                Some((input_id.clone(), frame_event.event))
            })
            .collect();

        QueueVideoOutput {
            frames,
            required,
            pts: buffer_pts,
        }
    }

    pub(super) fn should_push_next_frameset(
        &mut self,
        next_pts: Duration,
        queue_start_pts: Duration,
    ) -> bool {
        self.inputs
            .values()
            .filter_map(|input| input.upgrade())
            .for_each(|input| input.maybe_start_next_track());

        let input_status: Vec<_> = self
            .inputs
            .values()
            .filter_map(|weak| {
                weak.video(|input| {
                    let is_ready = input.is_ready_for_pts(next_pts, queue_start_pts);
                    let is_required = input.required();
                    (is_ready, is_required)
                })
            })
            .collect();

        if !self.ahead_of_time_processing
            && self.queue_ctx.sync_point + next_pts > self.queue_ctx.clock.now()
        {
            return false;
        }

        let all_inputs_ready = input_status.iter().all(|(is_ready, _)| *is_ready);
        if all_inputs_ready {
            return true;
        }

        let all_required_inputs_ready = input_status
            .iter()
            .filter(|(_is_ready, is_required)| *is_required)
            .all(|(is_ready, _is_required)| *is_ready);
        if !all_required_inputs_ready {
            return false;
        }

        if self.queue_ctx.sync_point + next_pts < self.queue_ctx.clock.now() {
            debug!("Pushing video frames while some inputs are not ready.");
            return true;
        }
        false
    }

    pub(super) fn drop_old_frames_before_start(&mut self) {
        for weak in self.inputs.values() {
            weak.video(|input| input.drop_old_frames_before_start());
        }
    }
}
