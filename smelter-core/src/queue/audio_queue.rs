use std::{collections::HashMap, time::Duration};

use smelter_render::InputId;
use tracing::debug;

use crate::queue::{QueueAudioOutput, QueueContext, WeakQueueInput};

pub struct AudioQueue {
    queue_ctx: QueueContext,
    inputs: HashMap<InputId, WeakQueueInput>,
    ahead_of_time_processing: bool,
}

impl AudioQueue {
    pub fn new(queue_ctx: QueueContext, ahead_of_time_processing: bool) -> Self {
        AudioQueue {
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

    pub(super) fn pop_samples_set(
        &mut self,
        range: (Duration, Duration),
        queue_start_pts: Duration,
    ) -> QueueAudioOutput {
        let (start_pts, end_pts) = range;
        let mut required = false;
        let samples = self
            .inputs
            .iter()
            .filter_map(|(input_id, weak)| {
                let audio_event = weak.audio(|input| input.pop_samples(range, queue_start_pts))?;
                required = required || audio_event.required;
                Some((input_id.clone(), audio_event.event))
            })
            .collect();

        QueueAudioOutput {
            required,
            samples,
            start_pts,
            end_pts,
        }
    }

    pub(super) fn should_push_for_pts_range(
        &mut self,
        pts_range: (Duration, Duration),
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
                weak.audio(|input| {
                    let is_ready = input.is_ready_for_pts(pts_range, queue_start_pts);
                    let is_required = input.required();
                    (is_ready, is_required)
                })
            })
            .collect();

        if !self.ahead_of_time_processing
            && self.queue_ctx.sync_point + pts_range.0 > self.queue_ctx.clock.now()
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

        if self.queue_ctx.sync_point + pts_range.0 < self.queue_ctx.clock.now() {
            debug!("Pushing audio samples while some inputs are not ready.");
            return true;
        }
        false
    }

    pub(super) fn drop_old_samples_before_start(&mut self) {
        for weak in self.inputs.values() {
            weak.audio(|input| input.drop_old_samples_before_start());
        }
    }
}
