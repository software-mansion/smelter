use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
    time::{Duration, Instant},
};

use crate::{
    event::{Event, EventEmitter},
    queue::{
        SharedState,
        utils::{EmitOnceGuard, PauseState, QueueState},
    },
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

                sync_point: self.sync_point,
                shared_state,

                offset_from_start: opts.offset,

                pause_state: PauseState::new(),
                state: QueueState::New,

                event_delivered_guard: EmitOnceGuard::new(
                    Event::AudioInputStreamDelivered(input_id.clone()),
                    &self.event_emitter,
                ),
                event_playing_guard: EmitOnceGuard::new(
                    Event::AudioInputStreamPlaying(input_id.clone()),
                    &self.event_emitter,
                ),
                event_eos_guard: EmitOnceGuard::new(
                    Event::AudioInputStreamEos(input_id.clone()),
                    &self.event_emitter,
                ),
            },
        );
    }

    pub fn remove_input(&mut self, input_id: &InputId) {
        self.inputs.remove(input_id);
    }

    pub fn pause_input(&mut self, input_id: &InputId, pts: Duration) {
        let Some(input) = self.inputs.get_mut(input_id) else {
            return;
        };
        if input.pause_state.pause(pts) {
            self.event_emitter
                .emit(Event::AudioInputStreamPaused(input_id.clone()));
        }
    }

    pub fn resume_input(&mut self, input_id: &InputId, pts: Duration) {
        let Some(input) = self.inputs.get_mut(input_id) else {
            return;
        };
        if input.pause_state.resume(pts, input.state) {
            // TS SDK tracks state based on those values, so if we pause in
            // non running state it will be stuck at paused until state does
            // not change
            if input.state == QueueState::Running {
                self.event_emitter
                    .emit(Event::AudioInputStreamPlaying(input_id.clone()));
            }
            input.queue.clear();
        }
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

    sync_point: Instant,
    shared_state: SharedState,

    pause_state: PauseState,
    state: QueueState,

    event_delivered_guard: EmitOnceGuard,
    event_playing_guard: EmitOnceGuard,
    event_eos_guard: EmitOnceGuard,
}

impl AudioQueueInput {
    /// Get batches that have samples in range `range` and remove them from the queue.
    /// Batches that are partially in range will still be returned. Single batch can
    /// be returned only once, elements downstream (audio mixer) is responsible for
    /// storing partially overlapping batches.
    fn pop_samples(
        &mut self,
        pts_range: (Duration, Duration),
        queue_start_pts: Duration,
    ) -> AudioEvent {
        if self.pause_state.is_paused() {
            return AudioEvent {
                required: false,
                event: PipelineEvent::Data(vec![]),
            };
        }

        // ignore result, we only need to ensure samples are enqueued
        self.try_enqueue_until_ready_for_pts(pts_range, queue_start_pts);

        let (_start_pts, end_pts) = pts_range;

        let mut popped_samples = vec![];
        while let Some(batch) = self.queue.front()
            && batch.start_pts <= end_pts + Duration::from_millis(120)
        {
            popped_samples.push(self.queue.pop_front().unwrap());
        }

        match self.state {
            QueueState::New | QueueState::Restarted if !popped_samples.is_empty() => {
                self.event_playing_guard.emit();
                self.state = QueueState::Running;
            }
            QueueState::Draining if self.queue.is_empty() && popped_samples.is_empty() => {
                self.reset_after_eos();
                return AudioEvent {
                    event: PipelineEvent::EOS,
                    required: true,
                };
            }
            _ => (),
        };
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
        if self.pause_state.is_paused() {
            return true;
        }

        if let QueueState::Draining = self.state {
            return true;
        }

        // range in queue pts time frame
        let end_pts = pts_range.1;

        fn has_all_samples_for_pts_range(
            queue: &VecDeque<InputAudioSamples>,
            range_end_pts: Duration,
        ) -> bool {
            match queue.back() {
                Some(batch) => batch.end_pts() >= range_end_pts + Duration::from_millis(120),
                None => false,
            }
        }

        while !has_all_samples_for_pts_range(&self.queue, end_pts) {
            if self.try_enqueue_samples(Some(queue_start_pts)).is_err() {
                return matches!(self.state, QueueState::Restarted);
            }
        }
        true
    }

    /// Drops samples that won't be used for processing. This function should only be called before
    /// queue start.
    fn drop_old_samples_before_start(&mut self) {
        loop {
            if let QueueState::Draining = self.state {
                self.reset_after_eos();
            }
            // if offset is defined try_enqueue_frame will always return err
            if self.queue.is_empty() && self.try_enqueue_samples(None).is_err() {
                return;
            }
            let Some(first_batch) = self.queue.front() else {
                return;
            };
            // If batch end is still in the future then do not drop.
            if self.sync_point + first_batch.end_pts() >= Instant::now() {
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
            self.event_delivered_guard.emit();
        }

        if self.offset_from_start.is_none() {
            match self.receiver.try_recv()? {
                PipelineEvent::Data(mut batch) => {
                    let _ = self.shared_state.get_or_init_first_pts(batch.start_pts);
                    batch.start_pts += self.pause_state.pts_offset();
                    self.queue.push_back(batch);
                }
                PipelineEvent::EOS => self.state = QueueState::Draining,
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
                    batch.start_pts =
                        (offset_pts + batch.start_pts + self.pause_state.pts_offset())
                            .saturating_sub(first_pts);
                    self.queue.push_back(batch);
                }
                PipelineEvent::EOS => self.state = QueueState::Draining,
            };
        }

        Ok(())
    }

    fn reset_after_eos(&mut self) {
        self.event_eos_guard.emit();

        self.event_playing_guard.reset();
        self.event_eos_guard.reset();

        // Reconnect after EOS will lose any relation to original offset
        // so we can behave as regular input here
        self.offset_from_start.take();
        self.queue.clear();
        self.state = QueueState::Restarted;
        self.pause_state.reset();
    }

    /// Offset value calculated in form of PTS(relative to sync point)
    fn offset_pts(&self, queue_start_pts: Duration) -> Option<Duration> {
        self.offset_from_start
            .map(|offset| queue_start_pts + offset)
    }
}
