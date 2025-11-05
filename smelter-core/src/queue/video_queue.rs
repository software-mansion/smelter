use crossbeam_channel::{Receiver, TryRecvError};
use tracing::debug;

use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
    time::{Duration, Instant},
};

use crate::{
    event::{Event, EventEmitter},
    queue::{QueueVideoOutput, SharedState, utils::EmitEventOnce},
};

use crate::prelude::*;

pub struct VideoQueue {
    sync_point: Instant,
    inputs: HashMap<InputId, VideoQueueInput>,
    event_emitter: Arc<EventEmitter>,
    ahead_of_time_processing: bool,
}

impl VideoQueue {
    pub fn new(
        sync_point: Instant,
        event_emitter: Arc<EventEmitter>,
        ahead_of_time_processing: bool,
    ) -> Self {
        VideoQueue {
            inputs: HashMap::new(),
            event_emitter,
            sync_point,
            ahead_of_time_processing,
        }
    }

    pub fn add_input(
        &mut self,
        input_id: &InputId,
        receiver: Receiver<PipelineEvent<Frame>>,
        opts: QueueInputOptions,
        shared_state: SharedState,
    ) {
        self.inputs.insert(
            input_id.clone(),
            VideoQueueInput {
                queue: VecDeque::new(),
                receiver,
                required: opts.required,

                eos_received: false,
                sync_point: self.sync_point,
                shared_state,

                offset_from_start: opts.offset,

                emit_once_delivered_event: EmitEventOnce::new(
                    Event::VideoInputStreamDelivered(input_id.clone()),
                    &self.event_emitter,
                ),
                emit_once_playing_event: EmitEventOnce::new(
                    Event::VideoInputStreamPlaying(input_id.clone()),
                    &self.event_emitter,
                ),
                emit_once_eos_event: EmitEventOnce::new(
                    Event::VideoInputStreamEos(input_id.clone()),
                    &self.event_emitter,
                ),
            },
        );
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
            .iter_mut()
            .filter_map(
                |(input_id, input)| match input.get_frame(buffer_pts, queue_start_pts) {
                    Some(frame_event) => {
                        required = required || frame_event.required;
                        Some((input_id.clone(), frame_event.event))
                    }
                    None => None,
                },
            )
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

        let all_inputs_ready = self
            .inputs
            .values_mut()
            .all(|input| input.try_enqueue_until_ready_for_pts(next_pts, queue_start_pts));
        if all_inputs_ready {
            return true;
        }

        let all_required_inputs_ready = self.inputs.values_mut().all(|input| {
            (!input.required) || input.try_enqueue_until_ready_for_pts(next_pts, queue_start_pts)
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
        for input in self.inputs.values_mut() {
            input.drop_old_frames_before_start()
        }
    }
}

struct FrameEvent {
    required: bool,
    event: PipelineEvent<Frame>,
}

pub struct VideoQueueInput {
    /// Frames are PTS ordered where PTS=0 represents beginning of the stream.
    queue: VecDeque<Frame>,
    /// Frames from the channel might have any PTS, they need to be processed
    /// before adding them to the `queue`.
    receiver: Receiver<PipelineEvent<Frame>>,
    /// If stream is required the queue should wait for frames. For optional
    /// inputs a queue will wait only as long as a buffer allows.
    required: bool,
    /// Offset of the stream relative to the start. If set to `None`
    /// offset will be resolved automatically on the stream start.
    offset_from_start: Option<Duration>,

    eos_received: bool,

    sync_point: Instant,
    shared_state: SharedState,

    emit_once_delivered_event: EmitEventOnce,
    emit_once_playing_event: EmitEventOnce,
    emit_once_eos_event: EmitEventOnce,
}

impl VideoQueueInput {
    /// Return frame for PTS and drop all the older frames. This function does not check
    /// whether stream is required or not.
    fn get_frame(&mut self, buffer_pts: Duration, queue_start_pts: Duration) -> Option<FrameEvent> {
        // ignore result, we only need to ensure frames are enqueued
        self.try_enqueue_until_ready_for_pts(buffer_pts, queue_start_pts);
        self.drop_old_frames(buffer_pts, queue_start_pts);

        let frame = match self.offset_pts(queue_start_pts) {
            Some(offset_pts) => {
                // if stream has offset and should not start yet, do not send any frames
                let after_offset_pts = offset_pts < buffer_pts;
                match after_offset_pts {
                    true => self.queue.front().cloned(),
                    false => None,
                }
            }
            None => {
                let after_first_pts = self
                    .shared_state
                    .first_pts()
                    .is_some_and(|first_pts| first_pts < buffer_pts);
                match after_first_pts {
                    true => self.queue.front().cloned(),
                    false => None,
                }
            }
        };

        // Handle a case where we have last frame and received EOS.
        // "drop_old_frames" is ensuring that there will only be one frame at
        // the end.
        if self.eos_received && self.queue.len() == 1 {
            self.queue.pop_front();
        }

        if self.eos_received && frame.is_none() && !self.emit_once_eos_event.already_sent() {
            self.emit_once_eos_event.emit();
            return Some(FrameEvent {
                required: true,
                event: PipelineEvent::EOS,
            });
        }

        match frame {
            Some(frame) => {
                self.emit_once_playing_event.emit();
                Some(FrameEvent {
                    required: self.required,
                    event: PipelineEvent::Data(frame),
                })
            }
            None => None,
        }
    }

    /// Check if the input has enough data in the queue to produce frames for `next_buffer_pts`.
    /// In particular if `self.offset` is in the future, then it will still return true even
    /// if it shouldn't produce any frames.
    /// After receiving EOS input is considered to always be "ready".
    ///
    /// We assume that the queue receives frames with monotonically increasing timestamps,
    /// so when all inputs queues have frames with pts larger or equal than buffer timestamp,
    /// the queue won't receive frames with pts "closer" to buffer pts.
    fn try_enqueue_until_ready_for_pts(
        &mut self,
        next_buffer_pts: Duration,
        queue_start_pts: Duration,
    ) -> bool {
        if self.eos_received {
            return true;
        }

        fn has_frame_for_pts(queue: &VecDeque<Frame>, next_buffer_pts: Duration) -> bool {
            match queue.back() {
                Some(last_frame) => last_frame.pts >= next_buffer_pts,
                None => false,
            }
        }

        while !has_frame_for_pts(&self.queue, next_buffer_pts) {
            if self.try_enqueue_frame(Some(queue_start_pts)).is_err() {
                return false;
            }
        }
        true
    }

    /// Drops frames that won't be used if the oldest pts that we will need in the future is
    /// `next_buffer_pts`.
    ///
    /// Finds frame that is closest to the next_buffer_pts and removes everything older.
    /// Frames in queue have monotonically increasing pts, so we can just drop all the frames
    /// before the "closest" one.
    /// If dropping frames removes everything from the queue try to enqueue some new frames
    /// and repeat the process.
    fn drop_old_frames(&mut self, next_buffer_pts: Duration, queue_start_pts: Duration) {
        let next_output_buffer_nanos = next_buffer_pts.as_nanos();
        loop {
            let closest_diff_frame_index = self
                .queue
                .iter()
                .enumerate()
                .min_by_key(|(_index, frame)| {
                    frame.pts.as_nanos().abs_diff(next_output_buffer_nanos)
                })
                .map(|(index, _frame)| index);

            if let Some(index) = closest_diff_frame_index {
                self.queue.drain(0..index);
            }

            if !self.queue.is_empty() {
                return;
            }

            // if queue is empty then try to enqueue some more frames
            if self.try_enqueue_frame(Some(queue_start_pts)).is_err() {
                return;
            }
        }
    }

    /// Drops frames that won't be used for processing. This function should only be called before
    /// queue start.
    fn drop_old_frames_before_start(&mut self) {
        loop {
            // if offset is defined try_enqueue_frame will always return err
            if self.queue.is_empty() && self.try_enqueue_frame(None).is_err() {
                return;
            }
            let Some(first_frame) = self.queue.front() else {
                return;
            };
            // If frame is still in the future then do not drop.
            if self.sync_point + first_frame.pts >= Instant::now() {
                return;
            }
            self.queue.pop_front();
        }
    }

    fn try_enqueue_frame(&mut self, queue_start_pts: Option<Duration>) -> Result<(), TryRecvError> {
        if !self.receiver.is_empty() {
            // if offset is defined the events are not dequeued before start
            // so we need to handle it here
            self.emit_once_delivered_event.emit();
        }

        if self.offset_from_start.is_none() {
            match self.receiver.try_recv()? {
                PipelineEvent::Data(frame) => {
                    if self.sync_point.elapsed() > frame.pts && !self.required {
                        debug!(?frame, "Frame delivered to late")
                    }
                    let _ = self.shared_state.get_or_init_first_pts(frame.pts);
                    self.queue.push_back(frame);
                }
                PipelineEvent::EOS => self.eos_received = true,
            };
        } else {
            let Some(offset_pts) = queue_start_pts.and_then(|start| self.offset_pts(start)) else {
                // if there is offset, do not enqueue before start
                return Err(TryRecvError::Empty);
            };
            match self.receiver.try_recv()? {
                // pts start from sync point
                PipelineEvent::Data(mut frame) => {
                    let first_pts = self.shared_state.get_or_init_first_pts(frame.pts);
                    frame.pts = offset_pts + frame.pts - first_pts;
                    if self.sync_point.elapsed() > frame.pts && !self.required {
                        debug!(?frame, "Frame delivered to late")
                    }
                    self.queue.push_back(frame);
                }
                PipelineEvent::EOS => self.eos_received = true,
            };
        }
        Ok(())
    }

    /// Offset value calculated in form of PTS(relative to sync point)
    fn offset_pts(&self, queue_start_pts: Duration) -> Option<Duration> {
        self.offset_from_start
            .map(|offset| queue_start_pts + offset)
    }
}
