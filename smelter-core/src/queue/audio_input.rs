use std::{collections::VecDeque, sync::Arc, time::Duration};

use crossbeam_channel::{Receiver, Sender, TryRecvError, bounded};
use smelter_render::InputId;
use tracing::{debug, trace, warn};

use crate::{
    PipelineCtx, PipelineEvent, Ref,
    event::{Event, EventEmitter},
    queue::{
        QueueContext, queue_input::TrackOffset, side_channel::AudioSideChannel,
        utils::EmitOnceGuard,
    },
};

use crate::prelude::*;

pub(super) struct AudioEvent {
    pub required: bool,
    pub event: PipelineEvent<Vec<InputAudioSamples>>,
}

const MIXER_STRETCH_BUFFER: Duration = Duration::from_millis(80);

pub(crate) struct AudioQueueInput {
    queue_ctx: QueueContext,
    /// Samples from the channel might have any PTS, they need to be processed
    /// before adding them to the `queue`.
    receiver: AudioInputReceiver,
    /// If stream is required the queue should wait for frames. For optional
    /// inputs a queue will wait only as long as a buffer allows.
    required: bool,
    /// Offset of the stream relative to the start. If set to `None`
    /// offset will be resolved automatically on the stream start.
    offset_from_start: Option<Duration>,

    track_offset: TrackOffset,

    paused: bool,

    event_delivered_guard: EmitOnceGuard,
    event_playing_guard: EmitOnceGuard,
    event_eos_guard: EmitOnceGuard,
    event_emitter: Arc<EventEmitter>,
    input_id: InputId,
}

impl AudioQueueInput {
    pub(super) fn new(
        ctx: &Arc<PipelineCtx>,
        input_ref: &Ref<InputId>,
        required: bool,
        offset: Option<Duration>,
        track_offset: TrackOffset,
        side_channel: Option<AudioSideChannel>,
    ) -> (Self, Sender<InputAudioSamples>) {
        let (receiver, sender) =
            AudioInputReceiver::new(ctx.queue_ctx.side_channel_delay, side_channel);
        let input = Self {
            queue_ctx: ctx.queue_ctx.clone(),
            required,
            offset_from_start: offset,
            receiver,
            track_offset,
            paused: false,
            event_delivered_guard: EmitOnceGuard::new(
                Event::AudioInputStreamDelivered(input_ref.id().clone()),
                &ctx.event_emitter,
            ),
            event_playing_guard: EmitOnceGuard::new(
                Event::AudioInputStreamPlaying(input_ref.id().clone()),
                &ctx.event_emitter,
            ),
            event_eos_guard: EmitOnceGuard::new(
                Event::AudioInputStreamEos(input_ref.id().clone()),
                &ctx.event_emitter,
            ),
            event_emitter: ctx.event_emitter.clone(),
            input_id: input_ref.id().clone(),
        };
        (input, sender)
    }

    pub(super) fn is_done(&mut self) -> bool {
        matches!(self.receiver.state(), ReceiverState::Done)
    }

    pub(super) fn required(&self) -> bool {
        self.required
    }

    pub(super) fn pause(&mut self) {
        if !self.paused {
            self.paused = true;
            self.event_emitter
                .emit(Event::AudioInputStreamPaused(self.input_id.clone()));
        }
    }

    pub(super) fn resume(&mut self) {
        if self.paused {
            self.paused = false;
            self.event_playing_guard.reset();
        };
    }

    /// Pop all batches with PTS smaller than `pts_range.1`.
    /// Every batch is returned exactly once (always popped, never dropped).
    pub(super) fn pop_samples(
        &mut self,
        pts_range: (Duration, Duration),
        queue_start_pts: Duration,
    ) -> AudioEvent {
        if self.paused {
            return AudioEvent {
                required: false,
                event: PipelineEvent::Data(vec![]),
            };
        }

        let Some(offset) = self.resolve_offset(pts_range.0, queue_start_pts) else {
            return AudioEvent {
                required: self.required,
                event: PipelineEvent::Data(vec![]),
            };
        };

        if let Some(offset_from_start) = self.offset_from_start
            && pts_range.1 < queue_start_pts + offset_from_start
        {
            return AudioEvent {
                required: self.required,
                event: PipelineEvent::Data(vec![]),
            };
        }

        let input_pts = pts_range.1.saturating_sub(offset) + MIXER_STRETCH_BUFFER;
        trace!(queue_pts=?pts_range, ?input_pts, "Try get samples batch");

        let mut samples = self.receiver.pop_before_pts(input_pts);
        for batch in &mut samples {
            batch.start_pts += offset;
        }

        if !samples.is_empty() {
            self.event_playing_guard.emit();
        }

        if samples.is_empty() && self.is_done() && !self.event_eos_guard.emited() {
            self.event_eos_guard.emit();
            return AudioEvent {
                required: true,
                event: PipelineEvent::EOS,
            };
        }

        AudioEvent {
            required: self.required,
            event: PipelineEvent::Data(samples),
        }
    }

    pub(super) fn is_ready_for_pts(
        &mut self,
        pts_range: (Duration, Duration),
        queue_start_pts: Duration,
    ) -> bool {
        if self.paused {
            return true;
        }

        let offset = self.resolve_offset(pts_range.0, queue_start_pts);

        if let Some(offset) = offset {
            // extra buffer offsets additional latency/delay from audio mixer resampler.
            let input_pts = pts_range.1.saturating_sub(offset) + MIXER_STRETCH_BUFFER;
            trace!(queue_pts=?pts_range, ?input_pts, "Is next sample batch ready for PTS");
            return self.receiver.is_ready_for_pts(input_pts);
        }

        match self.receiver.state() {
            ReceiverState::New => match self.offset_from_start {
                Some(offset_from_start) => {
                    pts_range.1.saturating_sub(queue_start_pts) < offset_from_start
                }
                None => true,
            },
            ReceiverState::Running => {
                warn!("receiver running, offset should already be resolved");
                true
            }
            ReceiverState::Done => true,
        }
    }

    fn resolve_offset(
        &mut self,
        buffer_pts: Duration,
        queue_start_pts: Duration,
    ) -> Option<Duration> {
        if self.receiver.state() != ReceiverState::Running {
            return self.track_offset.get();
        }

        self.event_delivered_guard.emit();
        let offset = match self.offset_from_start {
            Some(offset_from_start) => self
                .track_offset
                .get_or_init(offset_from_start + queue_start_pts),
            None => self.track_offset.get_or_init(buffer_pts),
        };
        Some(offset)
    }

    /// Drops samples that won't be used for processing. This function should only be called before
    /// queue start.
    pub(super) fn drop_old_samples_before_start(&mut self) {
        if self.receiver.state() == ReceiverState::New {
            return;
        }

        self.event_delivered_guard.emit();
        if self.offset_from_start.is_none() {
            let now = self.queue_ctx.sync_point.elapsed();
            let offset = self.track_offset.get_or_init(now);
            let _ = self.receiver.pop_before_pts(now.saturating_sub(offset));
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum ReceiverState {
    New,
    Running,
    Done,
}

pub(crate) struct AudioInputReceiver {
    max_size: Duration,
    receiver: Receiver<InputAudioSamples>,
    buffer: VecDeque<InputAudioSamples>,
    disconnected: bool,
    state: ReceiverState,
    delay: Duration,
    side_channel: Option<AudioSideChannel>,
}

impl AudioInputReceiver {
    pub fn new(
        delay: Duration,
        side_channel: Option<AudioSideChannel>,
    ) -> (Self, Sender<InputAudioSamples>) {
        let (sender, receiver) = bounded(1);
        let track = Self {
            max_size: Duration::from_millis(100),
            receiver,
            buffer: VecDeque::new(),
            disconnected: false,
            state: ReceiverState::New,
            delay,
            side_channel,
        };
        (track, sender)
    }

    /// Pop all batches with `start_pts < pts`. Every batch is returned exactly once.
    fn pop_before_pts(&mut self, pts: Duration) -> Vec<InputAudioSamples> {
        if self.state == ReceiverState::Done {
            return Vec::new();
        }
        self.try_enqueue_until(pts);

        let mut result = Vec::new();
        while let Some(batch) = self.buffer.front() {
            if batch.start_pts < pts {
                result.push(self.buffer.pop_front().unwrap());
            } else {
                break;
            }
        }
        self.maybe_transition_to_done();
        result
    }

    fn is_ready_for_pts(&mut self, end_pts: Duration) -> bool {
        if self.state() == ReceiverState::Done || self.disconnected {
            return true;
        }
        self.try_enqueue_until(end_pts);
        match self.buffer.back() {
            Some(batch) => batch.end_pts() >= end_pts,
            None => matches!(self.state, ReceiverState::Done),
        }
    }

    /// Enqueue batches from the channel, allowing exceeding `max_size`
    /// if the buffer doesn't yet cover `needed_pts`.
    fn try_enqueue_until(&mut self, needed_pts: Duration) {
        let side_channel_size = match self.side_channel {
            Some(_) => self.delay,
            None => Duration::ZERO,
        };
        loop {
            if self.disconnected {
                return;
            }
            let back = self.buffer.back();
            let has_needed = back
                .map(|batch| batch.end_pts() > needed_pts)
                .unwrap_or(false);
            if has_needed && self.size() >= self.max_size && self.size() >= side_channel_size {
                return;
            }
            match self.receiver.try_recv() {
                Ok(mut batch) => {
                    trace!(pts_range=?batch.pts_range(), pending=self.receiver.len(), "Enqueue samples");
                    batch.start_pts += self.delay;
                    if let Some(side_channel) = &self.side_channel {
                        side_channel.send_samples(&batch);
                    }
                    self.buffer.push_back(batch);
                    self.state = ReceiverState::Running;
                }
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    debug!("Queue audio channel disconnected");
                    self.disconnected = true;
                    self.maybe_transition_to_done();
                    return;
                }
            }
        }
    }

    fn state(&mut self) -> ReceiverState {
        self.try_enqueue_until(Duration::ZERO);
        self.state
    }

    fn maybe_transition_to_done(&mut self) {
        if self.disconnected && self.buffer.is_empty() {
            self.state = ReceiverState::Done;
            debug!("Queue audio input done")
        }
    }

    pub fn size(&self) -> Duration {
        match (self.buffer.front(), self.buffer.back()) {
            (Some(front), Some(back)) => back.end_pts().saturating_sub(front.start_pts),
            _ => Duration::ZERO,
        }
    }
}
