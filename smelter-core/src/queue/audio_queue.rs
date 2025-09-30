use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
    time::{Duration, Instant},
};

use crate::{
    event::{Event, EventEmitter},
    queue::{utils::EmitEventOnce, SharedState},
};

use crate::prelude::*;

use super::QueueAudioOutput;
use crossbeam_channel::{Receiver, TryRecvError};
use smelter_render::InputId;
use tracing::debug;

pub struct AudioQueue {
    sync_point: Instant,
    inputs: HashMap<InputId, AudioQueueInput>,
    event_emitter: Arc<EventEmitter>,
    ahead_of_time_processing: bool,
}

impl AudioQueue {
    pub fn new(
        sync_point: Instant,
        event_emitter: Arc<EventEmitter>,
        ahead_of_time_processing: bool,
    ) -> Self {
        AudioQueue {
            inputs: HashMap::new(),
            event_emitter,
            sync_point,
            ahead_of_time_processing,
        }
    }

    pub fn add_input(
        &mut self,
        input_id: &InputId,
        receiver: Receiver<PipelineEvent<InputAudioSamples>>,
        opts: QueueInputOptions,
        shared_state: SharedState,
    ) {
        self.inputs.insert(
            input_id.clone(),
            AudioQueueInput {
                queue: VecDeque::new(),
                receiver,
                required: opts.required,

                eos_received: false,
                sync_point: self.sync_point,
                shared_state,

                offset_from_start: opts.offset,

                emit_once_delivered_event: EmitEventOnce::new(
                    Event::AudioInputStreamDelivered(input_id.clone()),
                    &self.event_emitter,
                ),
                emit_once_playing_event: EmitEventOnce::new(
                    Event::AudioInputStreamPlaying(input_id.clone()),
                    &self.event_emitter,
                ),
                emit_once_eos_event: EmitEventOnce::new(
                    Event::AudioInputStreamEos(input_id.clone()),
                    &self.event_emitter,
                ),
            },
        );
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
            .iter_mut()
            .map(|(input_id, input)| {
                let audio_event = input.pop_samples(range, queue_start_pts);
                required = required || audio_event.required;
                (input_id.clone(), audio_event.event)
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

        let all_inputs_ready = self
            .inputs
            .values_mut()
            .all(|input| input.try_enqueue_until_ready_for_pts(pts_range, queue_start_pts));
        if all_inputs_ready {
            return true;
        };

        let all_required_inputs_ready = self.inputs.values_mut().all(|input| {
            (!input.required) || input.try_enqueue_until_ready_for_pts(pts_range, queue_start_pts)
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
        for input in self.inputs.values_mut() {
            input.drop_old_samples_before_start()
        }
    }
}

struct AudioEvent {
    required: bool,
    event: PipelineEvent<Vec<InputAudioSamples>>,
}

struct AudioQueueInput {
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
    offset_from_start: Option<Duration>,

    eos_received: bool,

    sync_point: Instant,
    shared_state: SharedState,

    emit_once_delivered_event: EmitEventOnce,
    emit_once_playing_event: EmitEventOnce,
    emit_once_eos_event: EmitEventOnce,
}

impl AudioQueueInput {
    /// Get batches that have samples in range `range` and remove them from the queue.
    /// Batches that are partially in range will still be returned, but they won't be
    /// removed from the queue.
    fn pop_samples(
        &mut self,
        pts_range: (Duration, Duration),
        queue_start_pts: Duration,
    ) -> AudioEvent {
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

        if self.eos_received
            && popped_samples.is_empty()
            && self.queue.is_empty()
            && !self.emit_once_eos_event.already_sent()
        {
            self.emit_once_eos_event.emit();
            return AudioEvent {
                event: PipelineEvent::EOS,
                required: true,
            };
        }
        if !popped_samples.is_empty() {
            self.emit_once_playing_event.emit();
        }
        AudioEvent {
            required: self.required,
            event: PipelineEvent::Data(popped_samples),
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
            // if offset is defined try_enqueue_frame will always return err
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

    fn try_enqueue_samples(
        &mut self,
        queue_start_pts: Option<Duration>,
    ) -> Result<(), TryRecvError> {
        if !self.receiver.is_empty() {
            // if offset is defined the events are not dequeued before start
            // so we need to handle it here
            self.emit_once_delivered_event.emit();
        }

        if self.offset_from_start.is_none() {
            match self.receiver.try_recv()? {
                PipelineEvent::Data(batch) => {
                    let _ = self.shared_state.get_or_init_first_pts(batch.start_pts);
                    self.queue.push_back(batch);
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
                PipelineEvent::Data(mut batch) => {
                    let first_pts = self.shared_state.get_or_init_first_pts(batch.start_pts);
                    batch.start_pts = offset_pts + batch.start_pts - first_pts;
                    batch.end_pts = offset_pts + batch.end_pts - first_pts;
                    self.queue.push_back(batch);
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
