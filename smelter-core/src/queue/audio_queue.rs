use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use smelter_render::InputId;
use tracing::debug;

use crate::queue::{QueueAudioOutput, WeakQueueInput};

pub struct AudioQueue {
    sync_point: Instant,
    inputs: HashMap<InputId, WeakQueueInput>,
    ahead_of_time_processing: bool,
}

impl AudioQueue {
    pub fn new(sync_point: Instant, ahead_of_time_processing: bool) -> Self {
        AudioQueue {
            inputs: HashMap::new(),
            sync_point,
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
        if !self.ahead_of_time_processing && self.sync_point + pts_range.0 > Instant::now() {
            return false;
        }

        let all_inputs_ready = self.inputs.values().all(|weak| {
            weak.audio(|input| input.try_enqueue_until_ready_for_pts(pts_range, queue_start_pts))
                .unwrap_or(true)
        });
        if all_inputs_ready {
            return true;
        };

        let all_required_inputs_ready = self.inputs.values().all(|weak| {
            weak.audio(|input| {
                (!input.required())
                    || input.try_enqueue_until_ready_for_pts(pts_range, queue_start_pts)
            })
            .unwrap_or(true)
        });
        if !all_required_inputs_ready {
            return false;
        }

        if self.sync_point + pts_range.0 < Instant::now() {
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
