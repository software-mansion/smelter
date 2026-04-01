use std::{collections::VecDeque, sync::Arc, time::Duration};

use crossbeam_channel::{Receiver, Sender, TryRecvError, bounded};
use smelter_render::{Frame, InputId};
use tracing::warn;

use crate::{
    PipelineCtx, PipelineEvent, Ref,
    event::{Event, EventEmitter},
    queue::{QueueContext, queue_input::TrackOffset, utils::EmitOnceGuard},
};

#[derive(Clone)]
pub(super) struct FrameEvent {
    pub required: bool,
    pub event: PipelineEvent<Frame>,
}

pub(crate) struct VideoQueueInput {
    queue_ctx: QueueContext,
    /// Frames from the channel might have any PTS, they need to be processed
    /// before adding them to the `queue`.
    receiver: VideoInputReceiver,
    /// If stream is required the queue should wait for frames. For optional
    /// inputs a queue will wait only as long as a buffer allows.
    required: bool,
    /// Offset of the stream relative to the start. If set to `None`
    /// offset will be resolved automatically on the stream start.
    offset_from_start: Option<Duration>,

    track_offset: TrackOffset,

    paused: bool,
    paused_event: Option<FrameEvent>,

    event_delivered_guard: EmitOnceGuard,
    event_playing_guard: EmitOnceGuard,
    event_eos_guard: EmitOnceGuard,
    event_emitter: Arc<EventEmitter>,
    input_id: InputId,
}

impl VideoQueueInput {
    pub(super) fn new(
        ctx: &Arc<PipelineCtx>,
        input_ref: &Ref<InputId>,
        required: bool,
        offset_from_start: Option<Duration>,
        track_offset: TrackOffset,
    ) -> (Self, Sender<Frame>) {
        let (receiver, sender) = VideoInputReceiver::new(ctx.queue_ctx.side_channel_delay);
        let input = Self {
            queue_ctx: ctx.queue_ctx.clone(),
            required,
            offset_from_start,
            receiver,
            track_offset,
            paused: false,
            paused_event: None,
            event_delivered_guard: EmitOnceGuard::new(
                Event::VideoInputStreamDelivered(input_ref.id().clone()),
                &ctx.event_emitter,
            ),
            event_playing_guard: EmitOnceGuard::new(
                Event::VideoInputStreamPlaying(input_ref.id().clone()),
                &ctx.event_emitter,
            ),
            event_eos_guard: EmitOnceGuard::new(
                Event::VideoInputStreamEos(input_ref.id().clone()),
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
        if self.paused {
            return;
        }
        let pts = self.queue_ctx.last_pts.value().unwrap_or_default();
        let queue_start_pts = self.queue_ctx.start_pts.value();
        let frame = queue_start_pts
            .and_then(|queue_start_pts| {
                // Partially duplicate get_frame logic, we can't call it directly
                // because we don't want to tiger eos event.

                let offset = self.resolve_offset(pts, queue_start_pts);

                // if pts is before offset we don't want to return it yet
                if let Some(offset_from_start) = self.offset_from_start
                    && pts < queue_start_pts + offset_from_start
                {
                    return None;
                }
                offset
            })
            .and_then(|offset| self.receiver.get_for_pts(pts.saturating_sub(offset)));

        self.paused_event = frame.map(|frame| FrameEvent {
            required: self.required,
            event: PipelineEvent::Data(frame),
        });
        self.paused = true;

        self.event_emitter
            .emit(Event::VideoInputStreamPaused(self.input_id.clone()));
    }

    pub(super) fn resume(&mut self) {
        if self.paused {
            self.paused = false;
            self.paused_event = None;

            // it will send playing event on next frame
            self.event_playing_guard.reset();
        };
    }

    /// Return frame for PTS and drop all the older frames. This function does not check
    /// whether stream is required or not.
    pub(super) fn get_frame(
        &mut self,
        pts: Duration,
        queue_start_pts: Duration,
    ) -> Option<FrameEvent> {
        if self.paused {
            return self.paused_event.clone();
        }

        let Some(offset) = self.resolve_offset(pts, queue_start_pts) else {
            return None;
        };

        // if buffer_pts is before offset we don't want to return it yet
        if let Some(offset_from_start) = self.offset_from_start
            && pts < queue_start_pts + offset_from_start
        {
            return None;
        }

        match self.receiver.get_for_pts(pts.saturating_sub(offset)) {
            Some(mut frame) => {
                self.event_playing_guard.emit();
                frame.pts += offset;
                Some(FrameEvent {
                    required: self.required,
                    event: PipelineEvent::Data(frame),
                })
            }
            None => {
                if self.is_done() && !self.event_eos_guard.emited() {
                    self.event_eos_guard.emit();
                    Some(FrameEvent {
                        required: true,
                        event: PipelineEvent::EOS,
                    })
                } else {
                    None
                }
            }
        }
    }

    pub(super) fn try_enqueue_until_ready_for_pts(
        &mut self,
        next_pts: Duration,
        queue_start_pts: Duration,
    ) -> bool {
        if self.paused {
            return true;
        }

        let offset = self.resolve_offset(next_pts, queue_start_pts);

        if let Some(offset) = offset {
            return self
                .receiver
                .is_ready_for_pts(next_pts.saturating_sub(offset));
        }

        match self.receiver.state() {
            ReceiverState::New => match self.offset_from_start {
                Some(offset_from_start) => {
                    next_pts.saturating_sub(queue_start_pts) < offset_from_start
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
        if let Some(offset) = self.track_offset.get() {
            return Some(offset);
        }

        if self.receiver.state() != ReceiverState::Running {
            return None;
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

    /// Drops frames that won't be used for processing. This function should only be called before
    /// queue start.
    pub(super) fn drop_old_frames_before_start(&mut self) {
        if self.receiver.state() == ReceiverState::New {
            return;
        }

        self.event_delivered_guard.emit();
        if self.offset_from_start.is_none() {
            let now = self.queue_ctx.sync_point.elapsed();
            let offset = self.track_offset.get_or_init(now);
            let _ = self.receiver.is_ready_for_pts(now.saturating_sub(offset));
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum ReceiverState {
    New,
    Running,
    Done,
}

pub(crate) struct VideoInputReceiver {
    max_size: Duration,
    receiver: Receiver<Frame>,
    buffer: VecDeque<Frame>,
    disconnected: bool,
    state: ReceiverState,
    delay: Duration,
}

impl VideoInputReceiver {
    pub fn new(delay: Duration) -> (Self, Sender<Frame>) {
        let (sender, receiver) = bounded(1);
        let track = Self {
            max_size: Duration::from_secs(1),
            receiver,
            buffer: VecDeque::new(),
            disconnected: false,
            state: ReceiverState::New,
            delay,
        };
        (track, sender)
    }

    fn get_for_pts(&mut self, pts: Duration) -> Option<Frame> {
        if self.state == ReceiverState::Done {
            return None;
        }
        self.prepare_for_pts(pts);
        if self.disconnected && self.buffer.len() == 1 {
            let frame = self.buffer.pop_front();
            self.maybe_transition_to_done();
            frame
        } else {
            self.buffer.front().cloned()
        }
    }

    fn is_ready_for_pts(&mut self, pts: Duration) -> bool {
        if self.state == ReceiverState::Done {
            return true;
        }
        self.prepare_for_pts(pts);
        let mut iter = self.buffer.iter();
        match (iter.next(), iter.next()) {
            (Some(first), _) if first.pts > pts => true,
            (_, Some(second)) if second.pts > pts => true,
            _ => false,
        }
    }

    fn prepare_for_pts(&mut self, pts: Duration) {
        loop {
            self.try_enqueue();
            if self.buffer.is_empty() {
                return;
            }

            let closest_idx = self
                .buffer
                .iter()
                .enumerate()
                .min_by_key(|(_, frame)| frame.pts.as_nanos().abs_diff(pts.as_nanos()))
                .map(|(idx, _)| idx)
                .unwrap();

            let drained = closest_idx > 0;
            self.buffer.drain(0..closest_idx);
            self.maybe_transition_to_done();

            // If we drained frames, there may be room for new frames closer to pts
            if !drained {
                return;
            }
        }
    }

    fn try_enqueue(&mut self) -> bool {
        let mut enqueued = false;
        loop {
            if self.size() >= self.max_size {
                return enqueued;
            }
            match self.receiver.try_recv() {
                Ok(mut frame) => {
                    frame.pts += self.delay;
                    self.buffer.push_back(frame);
                    self.state = ReceiverState::Running;
                    enqueued = true;
                }
                Err(TryRecvError::Empty) => return enqueued,
                Err(TryRecvError::Disconnected) => {
                    self.disconnected = true;
                    self.maybe_transition_to_done();
                    return enqueued;
                }
            }
        }
    }

    fn state(&mut self) -> ReceiverState {
        self.try_enqueue();
        self.state
    }

    fn maybe_transition_to_done(&mut self) {
        if self.disconnected && self.buffer.is_empty() {
            self.state = ReceiverState::Done;
        }
    }

    pub fn size(&self) -> Duration {
        match (self.buffer.front(), self.buffer.back()) {
            (Some(front), Some(back)) => back.pts.saturating_sub(front.pts),
            _ => Duration::ZERO,
        }
    }
}
