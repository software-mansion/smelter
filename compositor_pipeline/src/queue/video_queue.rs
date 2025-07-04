use compositor_render::Frame;
use compositor_render::InputId;
use crossbeam_channel::Receiver;
use crossbeam_channel::TryRecvError;

use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use crate::event::Event;
use crate::event::EventEmitter;

use super::utils::Clock;
use super::utils::InputProcessor;
use super::InputOptions;
use super::PipelineEvent;
use super::QueueVideoOutput;

pub struct VideoQueue {
    inputs: HashMap<InputId, VideoQueueInput>,
    event_emitter: Arc<EventEmitter>,
}

impl VideoQueue {
    pub fn new(event_emitter: Arc<EventEmitter>) -> Self {
        VideoQueue {
            inputs: HashMap::new(),
            event_emitter,
        }
    }

    pub fn add_input(
        &mut self,
        input_id: &InputId,
        receiver: Receiver<PipelineEvent<Frame>>,
        opts: InputOptions,
        clock: Clock,
    ) {
        self.inputs.insert(
            input_id.clone(),
            VideoQueueInput {
                input_id: input_id.clone(),
                queue: VecDeque::new(),
                receiver,
                input_frames_processor: InputProcessor::new(
                    opts.buffer_duration,
                    clock,
                    input_id.clone(),
                    self.event_emitter.clone(),
                ),
                required: opts.required,
                offset: opts.offset,
                eos_sent: false,
                first_frame_sent: false,
                event_emitter: self.event_emitter.clone(),
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
        queue_start: Instant,
    ) -> QueueVideoOutput {
        let frames = self
            .inputs
            .iter_mut()
            .filter_map(|(input_id, input)| {
                input
                    .get_frame(buffer_pts, queue_start)
                    .map(|frame| (input_id.clone(), frame))
            })
            .collect();

        QueueVideoOutput {
            frames,
            pts: buffer_pts,
        }
    }

    /// Checks if all inputs are ready to produce frames for specific PTS value (if all inputs have
    /// frames closest to buffer_pts).
    pub(super) fn check_all_inputs_ready_for_pts(
        &mut self,
        next_buffer_pts: Duration,
        queue_start: Instant,
    ) -> bool {
        self.inputs
            .values_mut()
            .all(|input| input.check_ready_for_pts(next_buffer_pts, queue_start))
    }

    /// Checks if all required inputs are ready to produce frames for specific PTS value (if
    /// all required inputs have frames closest to buffer_pts).
    pub(super) fn check_all_required_inputs_ready_for_pts(
        &mut self,
        next_buffer_pts: Duration,
        queue_start: Instant,
    ) -> bool {
        self.inputs.values_mut().all(|input| {
            (!input.required) || input.check_ready_for_pts(next_buffer_pts, queue_start)
        })
    }

    /// Checks if any of the required input stream have an offset that would
    /// require the stream to be used for PTS=`next_buffer_pts`
    pub(super) fn has_required_inputs_for_pts(
        &mut self,
        next_buffer_pts: Duration,
        queue_start: Instant,
    ) -> bool {
        self.inputs.values_mut().any(|input| {
            let should_already_start = |input: &mut VideoQueueInput| {
                input
                    .input_pts_from_queue_pts(next_buffer_pts, queue_start)
                    .is_some()
            };
            input.required && should_already_start(input)
        })
    }

    pub(super) fn drop_old_frames_before_start(&mut self) {
        for input in self.inputs.values_mut() {
            input.drop_old_frames_before_start()
        }
    }
}

pub struct VideoQueueInput {
    input_id: InputId,
    /// Frames are PTS ordered where PTS=0 represents beginning of the stream.
    queue: VecDeque<Frame>,
    /// Frames from the channel might have any PTS, they need to be processed
    /// before adding them to the `queue`.
    receiver: Receiver<PipelineEvent<Frame>>,
    /// Initial buffering + resets PTS to values starting with 0. All
    /// frames from receiver should be processed by this element.
    input_frames_processor: InputProcessor<Frame>,
    /// If stream is required the queue should wait for frames. For optional
    /// inputs a queue will wait only as long as a buffer allows.
    required: bool,
    /// Offset of the stream relative to the start. If set to `None`
    /// offset will be resolved automatically on the stream start.
    offset: Option<Duration>,

    eos_sent: bool,
    first_frame_sent: bool,

    event_emitter: Arc<EventEmitter>,
}

impl VideoQueueInput {
    /// Return frame for PTS and drop all the older frames. This function does not check
    /// whether stream is required or not.
    fn get_frame(
        &mut self,
        buffer_pts: Duration,
        queue_start: Instant,
    ) -> Option<PipelineEvent<Frame>> {
        // ignore result, we only need to ensure frames are enqueued
        self.check_ready_for_pts(buffer_pts, queue_start);

        self.drop_old_frames(buffer_pts, queue_start);
        let input_start_time = self.input_start_time()?;
        let frame = match self.offset {
            // if stream should not start yet, do not send any frames
            Some(offset) if offset > buffer_pts => None,
            // if stream is started then take the frames
            Some(offset) => self.queue.front().cloned().map(|mut frame| {
                frame.pts += offset;
                frame
            }),
            None => self.queue.front().cloned().map(|mut frame| {
                frame.pts = (input_start_time + frame.pts).duration_since(queue_start);
                frame
            }),
        };
        // Handle a case where we have last frame and received EOS.
        // "drop_old_frames" is ensuring that there will only be one frame at
        // the end.
        if self.input_frames_processor.did_receive_eos() && self.queue.len() == 1 {
            self.queue.pop_front();
        }

        if self.input_frames_processor.did_receive_eos() && frame.is_none() && !self.eos_sent {
            self.eos_sent = true;
            self.event_emitter
                .emit(Event::VideoInputStreamEos(self.input_id.clone()));
            Some(PipelineEvent::EOS)
        } else {
            if !self.first_frame_sent && frame.is_some() {
                self.event_emitter
                    .emit(Event::VideoInputStreamPlaying(self.input_id.clone()));
                self.first_frame_sent = true
            }
            frame.map(PipelineEvent::Data)
        }
    }

    /// Check if the input has enough data in the queue to produce frames for `next_buffer_pts`.
    /// In particular if `self.offset` is in the future, then it will still return true even
    /// if it shouldn't produce any frames.
    ///
    /// We assume that the queue receives frames with monotonically increasing timestamps,
    /// so when all inputs queues have frames with pts larger or equal than buffer timestamp,
    /// the queue won't receive frames with pts "closer" to buffer pts.
    fn check_ready_for_pts(&mut self, next_buffer_pts: Duration, queue_start: Instant) -> bool {
        if self.input_frames_processor.did_receive_eos() {
            return true;
        }

        let Some(next_buffer_pts) = self.input_pts_from_queue_pts(next_buffer_pts, queue_start)
        else {
            return match self.offset {
                Some(offset) => {
                    // if stream should start later than `next_buffer_pts`, then it's fine
                    // to consider it ready, because we will not use frames for that PTS
                    // regardless if they are there or not.
                    offset > next_buffer_pts
                }
                None => {
                    // It represents a stream that is still buffering. We know that frames
                    // from this input will not be used for this batch, so it is fine
                    // to consider this "ready".
                    true
                }
            };
        };

        fn has_frame_for_pts(queue: &VecDeque<Frame>, next_buffer_pts: Duration) -> bool {
            match queue.back() {
                Some(last_frame) => last_frame.pts >= next_buffer_pts,
                None => false,
            }
        }

        while !has_frame_for_pts(&self.queue, next_buffer_pts) {
            if self.try_enqueue_frame().is_err() {
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
    fn drop_old_frames(&mut self, next_buffer_pts: Duration, queue_start: Instant) {
        let Some(next_buffer_pts) = self.input_pts_from_queue_pts(next_buffer_pts, queue_start)
        else {
            // before first frame so nothing to drop
            return;
        };

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
            if self.try_enqueue_frame().is_err() {
                return;
            }
        }
    }

    /// Drops frames that won't be used for processing. This function should only be called before
    /// queue start.
    fn drop_old_frames_before_start(&mut self) {
        let Some(start_input_stream) = self.input_start_time() else {
            // before first frame, so nothing to do
            return;
        };

        if self.offset.is_some() {
            // if offset is defined never drop frames before start.
            return;
        };

        loop {
            if self.queue.is_empty() && self.try_enqueue_frame().is_err() {
                return;
            }
            let Some(first_frame) = self.queue.front() else {
                return;
            };
            // If frame is still in the future then do not drop.
            if start_input_stream + first_frame.pts >= Instant::now() {
                return;
            }
            self.queue.pop_front();
        }
    }

    /// Calculate input pts based on queue pts and queue start time. It can trigger
    /// enqueue internally.
    ///
    /// Returns None if:
    /// - Input is not ready and offset is unknown
    /// - If offset is negative (PTS refers to moment from before stream start)
    fn input_pts_from_queue_pts(
        &mut self,
        queue_pts: Duration,
        queue_start_time: Instant,
    ) -> Option<Duration> {
        let input_start_time = self.input_start_time();
        match self.offset {
            Some(offset) => queue_pts.checked_sub(offset),
            None => match input_start_time {
                Some(input_start_time) => {
                    (queue_start_time + queue_pts).checked_duration_since(input_start_time)
                }
                None => None,
            },
        }
    }

    /// Evaluate start time of this input. Start time represents an instant of time
    /// when input switched from buffering state to ready.
    fn input_start_time(&mut self) -> Option<Instant> {
        loop {
            if let Some(start_time) = self.input_frames_processor.start_time() {
                return Some(start_time);
            }

            if self.try_enqueue_frame().is_err() {
                return None;
            }
        }
    }

    fn try_enqueue_frame(&mut self) -> Result<(), TryRecvError> {
        let frame = self.receiver.try_recv()?;
        let mut frames = self.input_frames_processor.process_new_chunk(frame);
        self.queue.append(&mut frames);

        Ok(())
    }
}
