use tracing::debug;

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use crate::queue::QueueVideoOutput;

use crate::prelude::*;

use super::queue_input::WeakQueueInput;

pub struct VideoQueue {
    sync_point: Instant,
    inputs: HashMap<InputId, WeakQueueInput>,
    ahead_of_time_processing: bool,
}

impl VideoQueue {
    pub fn new(sync_point: Instant, ahead_of_time_processing: bool) -> Self {
        VideoQueue {
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

    pub fn pause_input(&mut self, input_id: &InputId, pts: Duration) {
        self.inputs
            .get(input_id)
            .and_then(|input| input.video(|input| input.pause(pts)));
    }

    pub fn resume_input(&mut self, input_id: &InputId, pts: Duration) {
        self.inputs
            .get(input_id)
            .and_then(|input| input.video(|input| input.resume(pts)));
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
        if !self.ahead_of_time_processing && self.sync_point + next_pts > Instant::now() {
            return false;
        }

        self.inputs
            .values()
            .filter_map(|input| input.upgrade())
            .for_each(|mut input| input.maybe_start_next_track());

        let all_inputs_ready = self.inputs.values().all(|weak| {
            weak.video(|input| input.try_enqueue_until_ready_for_pts(next_pts, queue_start_pts))
                .unwrap_or(true)
        });
        if all_inputs_ready {
            return true;
        }

        let all_required_inputs_ready = self.inputs.values().all(|weak| {
            weak.video(|input| {
                (!input.required())
                    || input.try_enqueue_until_ready_for_pts(next_pts, queue_start_pts)
            })
            .unwrap_or(true)
        });
        if !all_required_inputs_ready {
            return false;
        }

        if self.sync_point + next_pts < Instant::now() {
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
