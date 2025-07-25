use crossbeam_channel::{Receiver, TryRecvError};

use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
    time::{Duration, Instant},
};

use crate::event::Event;
use crate::event::EventEmitter;

use super::{InputOptions, PipelineEvent, QueueVideoOutput};

use crate::prelude::*;

pub struct VideoQueue {
    sync_point: Instant,
    inputs: HashMap<InputId, VideoQueueInput>,
    event_emitter: Arc<EventEmitter>,
}

impl VideoQueue {
    pub fn new(sync_point: Instant, event_emitter: Arc<EventEmitter>) -> Self {
        VideoQueue {
            inputs: HashMap::new(),
            event_emitter,
            sync_point,
        }
    }

    pub fn add_input(
        &mut self,
        input_id: &InputId,
        receiver: Receiver<PipelineEvent<Frame>>,
        opts: InputOptions,
    ) {
        self.inputs.insert(
            input_id.clone(),
            VideoQueueInput {
                input_id: input_id.clone(),
                queue: VecDeque::new(),
                receiver,
                required: opts.required,
                offset: opts.offset,
                eos_sent: false,
                eos_received: false,
                first_frame_sent: false,
                first_frame_pts: None,
                sync_point: self.sync_point,
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
        queue_start_pts: Duration,
    ) -> QueueVideoOutput {
        let frames = self
            .inputs
            .iter_mut()
            .filter_map(|(input_id, input)| {
                input
                    .get_frame(buffer_pts, queue_start_pts)
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
        queue_start_pts: Duration,
    ) -> bool {
        self.inputs
            .values_mut()
            .all(|input| input.try_enqueue_until_ready_for_pts(next_buffer_pts, queue_start_pts))
    }

    /// Checks if all required inputs are ready to produce frames for specific PTS value (if
    /// all required inputs have frames closest to buffer_pts).
    pub(super) fn check_all_required_inputs_ready_for_pts(
        &mut self,
        next_buffer_pts: Duration,
        queue_start_pts: Duration,
    ) -> bool {
        self.inputs.values_mut().all(|input| {
            (!input.required)
                || input.try_enqueue_until_ready_for_pts(next_buffer_pts, queue_start_pts)
        })
    }

    /// Checks if any of the required input stream have an offset that would
    /// require the stream to be used for PTS=`next_buffer_pts`
    ///
    /// For:
    /// - inputs without offset true
    /// - inputs with offset (offset + queue_start_pts < next_buffer_pts)
    pub(super) fn has_required_inputs_for_pts(
        &mut self,
        next_buffer_pts: Duration,
        queue_start_pts: Duration,
    ) -> bool {
        self.inputs
            .values_mut()
            .any(|input| input.is_required_ready_for_pts(next_buffer_pts, queue_start_pts))
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
    /// If stream is required the queue should wait for frames. For optional
    /// inputs a queue will wait only as long as a buffer allows.
    required: bool,
    /// Offset of the stream relative to the start. If set to `None`
    /// offset will be resolved automatically on the stream start.
    offset: Option<Duration>,

    eos_received: bool,
    eos_sent: bool,
    first_frame_sent: bool,

    sync_point: Instant,
    first_frame_pts: Option<Duration>,

    event_emitter: Arc<EventEmitter>,
}

impl VideoQueueInput {
    /// Return frame for PTS and drop all the older frames. This function does not check
    /// whether stream is required or not.
    fn get_frame(
        &mut self,
        buffer_pts: Duration,
        queue_start_pts: Duration,
    ) -> Option<PipelineEvent<Frame>> {
        // ignore result, we only need to ensure frames are enqueued
        self.try_enqueue_until_ready_for_pts(buffer_pts, queue_start_pts);
        self.drop_old_frames(buffer_pts, queue_start_pts);

        let frame = match self.offset {
            // if stream has offset and should not start yet, do not send any frames
            Some(offset) if offset + queue_start_pts > buffer_pts => None,
            // otherwise take the frame
            Some(_) | None => self.queue.front().cloned(),
        };
        // Handle a case where we have last frame and received EOS.
        // "drop_old_frames" is ensuring that there will only be one frame at
        // the end.
        if self.eos_received && self.queue.len() == 1 {
            self.queue.pop_front();
        }

        if self.eos_received && frame.is_none() && !self.eos_sent {
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
        if self.offset.is_some() {
            // if offset is defined never drop frames before start.
            return;
        };

        loop {
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

    fn is_required_ready_for_pts(
        &mut self,
        next_buffer_pts: Duration,
        queue_start_pts: Duration,
    ) -> bool {
        if !self.required {
            return false;
        }

        match self.offset {
            // if offset is in the past then input is required for the next batch
            Some(offset) => offset + queue_start_pts < next_buffer_pts,
            None => true,
        }
    }

    fn try_enqueue_frame(&mut self, queue_start_pts: Option<Duration>) -> Result<(), TryRecvError> {
        match self.offset {
            Some(offset) => match queue_start_pts {
                Some(queue_start_pts) => {
                    let frame = self.receiver.try_recv()?;
                    match frame {
                        // pts start from sync point
                        PipelineEvent::Data(mut frame) => {
                            let first_pts = *self.first_frame_pts.get_or_insert(frame.pts);
                            frame.pts = queue_start_pts + offset + frame.pts - first_pts;
                            self.queue.push_back(frame);
                        }
                        PipelineEvent::EOS => self.eos_received = true,
                    };
                }
                None => return Err(TryRecvError::Empty),
            },
            None => {
                let frame = self.receiver.try_recv()?;
                match frame {
                    PipelineEvent::Data(frame) => {
                        self.queue.push_back(frame);
                    }
                    PipelineEvent::EOS => self.eos_received = true,
                };
            }
        }

        Ok(())
    }
}
