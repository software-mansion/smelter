use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Once},
    time::{Duration, Instant},
};

use crate::event::{Event, EventEmitter};
use crate::prelude::*;

use super::{InputOptions, QueueAudioOutput};
use compositor_render::InputId;
use crossbeam_channel::{Receiver, TryRecvError};

#[derive(Debug)]
pub struct AudioQueue {
    sync_point: Instant,
    inputs: HashMap<InputId, AudioQueueInput>,
    event_emitter: Arc<EventEmitter>,
}

impl AudioQueue {
    pub fn new(sync_point: Instant, event_emitter: Arc<EventEmitter>) -> Self {
        AudioQueue {
            inputs: HashMap::new(),
            event_emitter,
            sync_point,
        }
    }

    pub fn add_input(
        &mut self,
        input_id: &InputId,
        receiver: Receiver<PipelineEvent<InputAudioSamples>>,
        opts: InputOptions,
    ) {
        self.inputs.insert(
            input_id.clone(),
            AudioQueueInput {
                input_id: input_id.clone(),
                queue: VecDeque::new(),
                receiver,
                required: opts.required,
                offset: opts.offset,
                eos_sent: false,
                event_emitter: self.event_emitter.clone(),
                eos_received: false,
                first_batch_sent: false,
                sync_point: self.sync_point,
                first_batch_pts: None,
            },
        );
    }

    pub fn remove_input(&mut self, input_id: &InputId) {
        self.inputs.remove(input_id);
    }

    /// Checks if all inputs are ready to produce frames for specific PTS value (if all inputs have
    /// frames closest to buffer_pts).
    pub(super) fn check_all_inputs_ready_for_pts(
        &mut self,
        pts_range: (Duration, Duration),
        queue_start_pts: Duration,
    ) -> bool {
        self.inputs
            .values_mut()
            .all(|input| input.try_enqueue_until_ready_for_pts(pts_range, queue_start_pts))
    }

    /// Checks if all required inputs are ready to produce frames for specific PTS value (if
    /// all required inputs have frames closest to buffer_pts).
    pub(super) fn check_all_required_inputs_ready_for_pts(
        &mut self,
        pts_range: (Duration, Duration),
        queue_start_pts: Duration,
    ) -> bool {
        self.inputs.values_mut().all(|input| {
            (!input.required) || input.try_enqueue_until_ready_for_pts(pts_range, queue_start_pts)
        })
    }

    /// Checks if any of the required input streams have an offset that would
    /// require the stream to be used for PTS=`next_buffer_pts`
    pub(super) fn has_required_inputs_for_pts(
        &mut self,
        next_buffer_pts: Duration,
        queue_start_pts: Duration,
    ) -> bool {
        self.inputs
            .values_mut()
            .any(|input| input.is_required_ready_for_pts(next_buffer_pts, queue_start_pts))
    }

    pub(super) fn pop_samples_set(
        &mut self,
        range: (Duration, Duration),
        queue_start_pts: Duration,
    ) -> QueueAudioOutput {
        let (start_pts, end_pts) = range;
        let samples = self
            .inputs
            .iter_mut()
            .map(|(input_id, input)| (input_id.clone(), input.pop_samples(range, queue_start_pts)))
            .collect();

        QueueAudioOutput {
            samples,
            start_pts,
            end_pts,
        }
    }

    pub(super) fn drop_old_samples_before_start(&mut self) {
        for input in self.inputs.values_mut() {
            input.drop_old_samples_before_start()
        }
    }
}

#[derive(Debug)]
struct AudioQueueInput {
    input_id: InputId,
    /// Samples/batches are PTS ordered where PTS=0 represents beginning of the stream.
    queue: VecDeque<InputAudioSamples>,
    /// Samples from the channel might have any PTS, they need to be processed before
    /// adding them to the `queue`.
    receiver: Receiver<PipelineEvent<InputAudioSamples>>,
    /// If stream is required the queue should wait for frames. For optional
    /// inputs a queue will wait only as long as a buffer allows.
    required: bool,
    /// Offset of the stream relative to the start. If set to `None`
    /// offset will be resolved automatically on the stream start.
    offset: Option<Duration>,

    eos_received: bool,
    eos_sent: bool,
    first_batch_sent: bool,

    sync_point: Instant,
    first_batch_pts: Option<Duration>,

    once_on_ready: Once,

    event_emitter: Arc<EventEmitter>,
}

impl AudioQueueInput {
    /// Get batches that have samples in range `range` and remove them from the queue.
    /// Batches that are partially in range will still be returned, but they won't be
    /// removed from the queue.
    fn pop_samples(
        &mut self,
        pts_range: (Duration, Duration),
        queue_start_pts: Duration,
    ) -> PipelineEvent<Vec<InputAudioSamples>> {
        // ignore result, we only need to ensure samples are enqueued
        self.try_enqueue_until_ready_for_pts(pts_range, queue_start_pts);

        let (start_pts, end_pts) = pts_range;

        let popped_samples = self
            .queue
            .iter()
            .filter(|batch| batch.start_pts <= end_pts && batch.end_pts >= start_pts)
            .cloned()
            .collect::<Vec<InputAudioSamples>>();

        // Drop all batches older than `end_pts`. Entire batch (all samples inside) has to be older.
        while self
            .queue
            .front()
            .is_some_and(|batch| batch.end_pts < end_pts)
        {
            self.queue.pop_front();
        }

        if self.eos_received && popped_samples.is_empty() && self.queue.is_empty() && !self.eos_sent
        {
            self.eos_sent = true;
            self.event_emitter
                .emit(Event::AudioInputStreamEos(self.input_id.clone()));
            PipelineEvent::EOS
        } else {
            if !self.first_batch_sent && !popped_samples.is_empty() {
                self.event_emitter
                    .emit(Event::AudioInputStreamPlaying(self.input_id.clone()));
                self.first_batch_sent = true
            }
            PipelineEvent::Data(popped_samples)
        }
    }

    fn try_enqueue_until_ready_for_pts(
        &mut self,
        pts_range: (Duration, Duration),
        queue_start_pts: Duration,
    ) -> bool {
        if self.eos_received {
            return true;
        }

        // range in queue pts time frame
        let end_pts = pts_range.1;

        fn has_all_samples_for_pts_range(
            queue: &VecDeque<InputAudioSamples>,
            range_end_pts: Duration,
        ) -> bool {
            match queue.back() {
                Some(batch) => batch.end_pts >= range_end_pts,
                None => false,
            }
        }

        while !has_all_samples_for_pts_range(&self.queue, end_pts) {
            if self.try_enqueue_samples(Some(queue_start_pts)).is_err() {
                return false;
            }
        }
        true
    }

    /// Drops samples that won't be used for processing. This function should only be called before
    /// queue start.
    fn drop_old_samples_before_start(&mut self) {
        loop {
            if self.queue.is_empty() && self.try_enqueue_samples(None).is_err() {
                return;
            }
            let Some(first_batch) = self.queue.front() else {
                return;
            };
            // If batch end is still in the future then do not drop.
            if self.sync_point + first_batch.end_pts >= Instant::now() {
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

    fn try_enqueue_samples(
        &mut self,
        queue_start_pts: Option<Duration>,
    ) -> Result<(), TryRecvError> {
        match self.offset {
            Some(offset) => match queue_start_pts {
                Some(queue_start_pts) => {
                    let sample_batch = self.receiver.try_recv()?;
                    match sample_batch {
                        // pts start from sync point
                        PipelineEvent::Data(mut sample_batch) => {
                            let first_pts =
                                *self.first_batch_pts.get_or_insert(sample_batch.start_pts);
                            sample_batch.start_pts =
                                queue_start_pts + offset + sample_batch.start_pts - first_pts;
                            sample_batch.end_pts =
                                queue_start_pts + offset + sample_batch.end_pts - first_pts;
                            self.queue.push_back(sample_batch);
                        }
                        PipelineEvent::EOS => self.eos_received = true,
                    };
                }
                None => return Err(TryRecvError::Empty),
            },
            None => {
                let sample_batch = self.receiver.try_recv()?;
                match sample_batch {
                    PipelineEvent::Data(sample_batch) => {
                        self.queue.push_back(sample_batch);
                    }
                    PipelineEvent::EOS => self.eos_received = true,
                };
            }
        }

        Ok(())
    }
}
