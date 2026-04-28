use std::{collections::VecDeque, sync::Arc, time::Duration};

use crossbeam_channel::{Receiver, Sender, TryRecvError, bounded};
use smelter_render::{Frame, InputId};
use tracing::{debug, trace, warn};

use crate::{
    PipelineEvent, Ref,
    event::{Event, EventEmitter},
    queue::{
        QueueContext, queue_input::TrackOffset, side_channel::VideoSideChannel,
        utils::EmitOnceGuard,
    },
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

    paused_pts: Option<Duration>,
    paused_frame: Option<Frame>,

    event_delivered_guard: EmitOnceGuard,
    event_playing_guard: EmitOnceGuard,
    event_eos_guard: EmitOnceGuard,
    event_emitter: Arc<EventEmitter>,
    input_id: InputId,
}

impl VideoQueueInput {
    pub(super) fn new(
        queue_ctx: &QueueContext,
        event_emitter: &Arc<EventEmitter>,
        input_ref: &Ref<InputId>,
        required: bool,
        offset_from_start: Option<Duration>,
        track_offset: TrackOffset,
        side_channel: Option<VideoSideChannel>,
    ) -> (Self, Sender<Frame>) {
        let (receiver, sender) =
            VideoInputReceiver::new(queue_ctx.side_channel_delay, side_channel);
        let input = Self {
            queue_ctx: queue_ctx.clone(),
            required,
            offset_from_start,
            receiver,
            track_offset,
            paused_pts: None,
            paused_frame: None,
            event_delivered_guard: EmitOnceGuard::new(
                Event::VideoInputStreamDelivered(input_ref.id().clone()),
                event_emitter,
            ),
            event_playing_guard: EmitOnceGuard::new(
                Event::VideoInputStreamPlaying(input_ref.id().clone()),
                event_emitter,
            ),
            event_eos_guard: EmitOnceGuard::new(
                Event::VideoInputStreamEos(input_ref.id().clone()),
                event_emitter,
            ),
            event_emitter: event_emitter.clone(),
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
        if self.paused_pts.is_some() {
            return;
        }
        let pts = self.queue_ctx.last_pts.value().unwrap_or_default();
        let queue_start_pts = self.queue_ctx.start_pts.value();
        let frame = queue_start_pts.and_then(|queue_start_pts| {
            // Partially duplicate get_frame logic, we can't call it directly
            // because we don't want to tiger eos event.
            let offset = self.resolve_offset(pts, queue_start_pts)?;
            self.receiver.get_for_pts(pts.saturating_sub(offset))
        });

        self.paused_frame = frame;
        self.paused_pts = Some(pts);

        self.event_emitter
            .emit(Event::VideoInputStreamPaused(self.input_id.clone()));
    }

    pub(super) fn resume(&mut self) {
        if self.paused_pts.is_some() {
            self.paused_pts = None;
            self.paused_frame = None;

            // it will send playing event on next frame
            self.event_playing_guard.reset();
        };
    }

    pub(super) fn paused_event(&self, pts: Duration) -> Option<FrameEvent> {
        let offset = self.track_offset.get()?;
        if let (Some(paused_pts), Some(mut frame)) = (self.paused_pts, self.paused_frame.clone()) {
            frame.pts += offset + pts.saturating_sub(paused_pts);
            return Some(FrameEvent {
                required: self.required,
                event: PipelineEvent::Data(frame),
            });
        }
        None
    }

    /// Return frame for PTS and drop all the older frames. This function does not check
    /// whether stream is required or not.
    pub(super) fn get_frame(
        &mut self,
        pts: Duration,
        queue_start_pts: Duration,
    ) -> Option<FrameEvent> {
        if self.paused_pts.is_some() {
            return self.paused_event(pts);
        }

        let offset = self.resolve_offset(pts, queue_start_pts)?;

        let input_pts = pts.saturating_sub(offset);
        trace!(queue_pts=?pts, ?input_pts, "Try get frame");

        match self.receiver.get_for_pts(input_pts) {
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

    pub(super) fn is_ready_for_pts(&mut self, pts: Duration, queue_start_pts: Duration) -> bool {
        if self.paused_pts.is_some() {
            return true;
        }

        let offset = self.resolve_offset(pts, queue_start_pts);

        if let Some(offset) = offset {
            let input_pts = pts.saturating_sub(offset);
            trace!(queue_pts=?pts, ?input_pts, "Is next frame ready for PTS");
            return self.receiver.is_ready_for_pts(input_pts);
        }

        match self.receiver.state() {
            ReceiverState::New => match self.offset_from_start {
                Some(offset_from_start) => pts.saturating_sub(queue_start_pts) < offset_from_start,
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

    /// Drops frames that won't be used for processing. This function should only be called before
    /// queue start.
    pub(super) fn drop_old_frames_before_start(&mut self) {
        if self.receiver.state() == ReceiverState::New {
            return;
        }

        self.event_delivered_guard.emit();
        if self.offset_from_start.is_none() {
            let now = self
                .queue_ctx
                .clock
                .elapsed_since(self.queue_ctx.sync_point);
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
    side_channel: Option<VideoSideChannel>,
}

impl VideoInputReceiver {
    pub fn new(delay: Duration, side_channel: Option<VideoSideChannel>) -> (Self, Sender<Frame>) {
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

    /// Get for pts returns the frame for specified pts.
    ///
    /// Frame pts always needs to be older (lower value). If it is not return None,
    /// this behavior diverges from `is_ready_for_pts`.
    fn get_for_pts(&mut self, pts: Duration) -> Option<Frame> {
        if self.state == ReceiverState::Done {
            return None;
        }
        self.prepare_for_pts(pts);
        match self.buffer.front() {
            Some(front) if front.pts > pts => return None,
            None => return None,
            _ => {}
        }
        if self.disconnected && self.buffer.len() == 1 {
            let frame = self.buffer.pop_front();
            self.maybe_transition_to_done();
            frame
        } else {
            self.buffer.front().cloned()
        }
    }

    /// Receiver is ready for pts if:
    /// - it already finished
    /// - it has any frame in buffer that is newer than pts
    ///
    /// If first pts is newer it is still considered ready, but get_for_pts
    /// will not return that frame for that pts
    fn is_ready_for_pts(&mut self, pts: Duration) -> bool {
        if self.disconnected {
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

    /// After this call, only the front frame of `self.buffer` might be older than `pts`;
    /// all remaining frames are newer.
    fn prepare_for_pts(&mut self, pts: Duration) {
        loop {
            self.try_enqueue();
            let mut dropped = false;

            // if second element is older than pts then we can drop firs one
            while let Some(second) = self.buffer.get(1)
                && second.pts <= pts
            {
                self.buffer.pop_front();
                dropped = true;
            }
            // If we dropped any frames, there may be room to enqueue more.
            if !dropped {
                self.maybe_transition_to_done();
                return;
            }
        }
    }

    fn try_enqueue(&mut self) {
        let side_channel_size = match self.side_channel {
            Some(_) => self.delay,
            None => Duration::ZERO,
        };

        loop {
            if self.disconnected {
                return;
            }

            if self.size() >= self.max_size && self.size() >= side_channel_size {
                return;
            }
            match self.receiver.try_recv() {
                Ok(mut frame) => {
                    trace!(pts=?frame.pts, pending=self.receiver.len(), "Enqueue frame");
                    frame.pts += self.delay;
                    if let Some(side_channel) = &mut self.side_channel {
                        side_channel.send_frame(&frame);
                    }
                    self.buffer.push_back(frame);
                    self.state = ReceiverState::Running;
                }
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    debug!("Queue video channel disconnected");
                    self.disconnected = true;
                    self.maybe_transition_to_done();
                    return;
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
            debug!("Queue video input done")
        }
    }

    pub fn size(&self) -> Duration {
        match (self.buffer.front(), self.buffer.back()) {
            (Some(front), Some(back)) => back.pts.saturating_sub(front.pts),
            _ => Duration::ZERO,
        }
    }
}
