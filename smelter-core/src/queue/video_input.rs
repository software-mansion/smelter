use std::{
    collections::VecDeque,
    sync::Arc,
    time::{Duration, Instant},
};

use crossbeam_channel::{Receiver, Sender, TryRecvError, bounded};
use smelter_render::{Frame, InputId};

use crate::{
    PipelineCtx, PipelineEvent, Ref,
    event::{Event, EventEmitter},
    queue::{
        queue_input::TrackOffset,
        utils::{EmitOnceGuard, PauseState, QueueState},
    },
};

pub(super) struct FrameEvent {
    pub required: bool,
    pub event: PipelineEvent<Frame>,
}

pub(crate) struct QueueInputState {
    pub tracks: VecDeque<(Option<VideoInputReceiver>, Option<VideoInputReceiver>)>,
}

pub(crate) struct VideoQueueInput {
    /// Frames from the channel might have any PTS, they need to be processed
    /// before adding them to the `queue`.
    receiver: VideoInputReceiver,
    /// If stream is required the queue should wait for frames. For optional
    /// inputs a queue will wait only as long as a buffer allows.
    required: bool,
    /// Offset of the stream relative to the start. If set to `None`
    /// offset will be resolved automatically on the stream start.
    //offset_from_start: Option<Duration>,
    sync_point: Instant,
    /// Offset of the stream relative to the start. If set to `None`
    /// offset will be resolved automatically on the stream start.
    offset_from_start: Option<Duration>,

    track_offset: TrackOffset,

    pause_state: VideoPauseState,
    state: QueueState,

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
        offset: Option<Duration>,
        track_offset: TrackOffset,
    ) -> (Self, Sender<Frame>) {
        let (receiver, sender) = VideoInputReceiver::new();
        let input = Self {
            required,
            offset_from_start: offset,
            receiver,
            sync_point: ctx.queue_sync_point,
            track_offset,
            pause_state: VideoPauseState::new(),
            state: QueueState::New,
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

    pub(super) fn is_done(&self) -> bool {
        self.receiver.is_done()
    }

    pub(super) fn required(&self) -> bool {
        self.required
    }

    pub(super) fn pause(&mut self, pts: Duration) {
        if self.pause_state.pause(pts, self.receiver.get_for_pts(pts)) {
            self.event_emitter
                .emit(Event::VideoInputStreamPaused(self.input_id.clone()));
        }
    }

    pub(super) fn resume(&mut self, pts: Duration) {
        if self.pause_state.resume(pts, self.state) {
            if QueueState::Running == self.state {
                // TS SDK tracks state based on those values, so if we pause in
                // non running state it will be stuck at paused until state does
                // not change
                self.event_emitter
                    .emit(Event::VideoInputStreamPlaying(self.input_id.clone()));
            }
        };
    }

    /// Return frame for PTS and drop all the older frames. This function does not check
    /// whether stream is required or not.
    pub(super) fn get_frame(
        &mut self,
        buffer_pts: Duration,
        queue_start_pts: Duration,
    ) -> Option<FrameEvent> {
        if self.pause_state.is_paused() {
            return self
                .pause_state
                .paused_frame(buffer_pts)
                .map(|event| FrameEvent {
                    required: self.required,
                    event,
                });
        }

        let offset = match self.offset_from_start {
            Some(offset) => self
                .track_offset
                .get_or_init((buffer_pts + offset).saturating_sub(queue_start_pts)),
            None => self.track_offset.get_or_init(buffer_pts),
        };

        match self.receiver.get_for_pts(buffer_pts.saturating_sub(offset)) {
            Some(mut frame) => {
                frame.pts += offset;
                Some(FrameEvent {
                    required: self.required,
                    event: PipelineEvent::Data(frame),
                })
            }
            None => {
                if self.is_done() {
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

    /// Drops frames that won't be used for processing. This function should only be called before
    /// queue start.
    pub(super) fn drop_old_frames_before_start(&mut self) {
        let is_ready = self.receiver.is_ready_for_pts(Duration::ZERO);
        if self.offset_from_start.is_none() && is_ready {
            let now = self.sync_point.elapsed();
            let offset = self.track_offset.get_or_init(now);
            let _ = self.receiver.is_ready_for_pts(now.saturating_sub(offset));
        }
    }
}

struct VideoPauseState {
    inner: PauseState,
    paused_frame: Option<Frame>,
}

impl VideoPauseState {
    fn new() -> Self {
        Self {
            inner: PauseState::new(),
            paused_frame: None,
        }
    }

    fn pause(&mut self, pts: Duration, frame: Option<Frame>) -> bool {
        if !self.inner.pause(pts) {
            return false; // already paused
        }
        self.paused_frame = frame;
        true
    }

    fn resume(&mut self, pts: Duration, state: QueueState) -> bool {
        self.paused_frame = None;
        self.inner.resume(pts, state)
    }

    /// Returns the paused frame as a PipelineEvent with PTS shifted by time elapsed since pause.
    fn paused_frame(&self, buffer_pts: Duration) -> Option<PipelineEvent<Frame>> {
        self.paused_frame.clone().map(|mut frame| {
            if let Some(paused_at) = self.inner.paused_at_pts() {
                frame.pts += buffer_pts.saturating_sub(paused_at);
            }
            PipelineEvent::Data(frame)
        })
    }

    fn is_paused(&self) -> bool {
        self.inner.is_paused()
    }

    fn pts_offset(&self) -> Duration {
        self.inner.pts_offset()
    }
}

pub(crate) struct VideoInputReceiver {
    max_size: Duration,
    receiver: Receiver<Frame>,
    buffer: VecDeque<Frame>,
    is_done: bool,
}

impl VideoInputReceiver {
    pub fn new() -> (Self, Sender<Frame>) {
        let (sender, receiver) = bounded(1);
        let track = Self {
            max_size: Duration::from_secs(1),
            receiver,
            buffer: VecDeque::new(),
            is_done: false,
        };
        (track, sender)
    }

    fn get_for_pts(&mut self, pts: Duration) -> Option<Frame> {
        self.prepare_for_pts(pts);
        if self.is_done && self.buffer.len() == 1 {
            self.buffer.front().cloned()
        } else {
            self.buffer.pop_front()
        }
    }

    fn is_ready_for_pts(&mut self, pts: Duration) -> bool {
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
                Ok(frame) => {
                    self.buffer.push_back(frame);
                    enqueued = true;
                }
                Err(TryRecvError::Empty) => return enqueued,
                Err(TryRecvError::Disconnected) => {
                    self.is_done = true;
                    return enqueued;
                }
            }
        }
    }

    pub fn size(&self) -> Duration {
        match (self.buffer.front(), self.buffer.back()) {
            (Some(front), Some(back)) => back.pts.saturating_sub(front.pts),
            _ => Duration::ZERO,
        }
    }

    fn is_done(&self) -> bool {
        self.is_done
    }
}
