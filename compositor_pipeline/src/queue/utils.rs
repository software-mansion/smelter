use std::{
    collections::VecDeque,
    mem,
    sync::{
        atomic::{AtomicI64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use compositor_render::{Frame, InputId};
use tracing::warn;

use crate::event::{Event, EventEmitter};
use crate::prelude::*;

/// InputProcessor handles initial processing for frames/samples that are being
/// queued. For each received frame/sample batch, the `process_new_chunk`
/// method should be called and only elements returned should be used
/// in a queue.
///
/// 1. New input starts in `InputState::WaitingForStart`.
/// 2. When `process_new_chunk` is called for the first time it transitions to
///    the Buffering state.
/// 3. Each new call to the `process_new_chunk` is adding frames to the buffer
///    until it reaches a specific size/duration.
/// 4. After buffer reaches a certain size, calculate the offset and switch
///    to the `Ready` state.
/// 5. In `Ready` state `process_new_chunk` is immediately returning frame or sample
///    batch passed with arguments with modified pts.
#[derive(Debug)]
pub(super) struct InputProcessor<Payload: InputProcessorMediaExt> {
    input_id: InputId,

    buffer_duration: Duration,

    /// Moment where input transitioned to a ready state
    start_time: Option<Instant>,

    state: InputState<Payload>,

    clock: Clock,

    event_emitter: Arc<EventEmitter>,
}

#[derive(Debug)]
pub(super) enum InputState<Payload: InputProcessorMediaExt> {
    WaitingForStart,
    Buffering {
        buffer: Vec<(Payload, Duration)>,
    },
    Ready {
        /// Offset that needs to be applied(subtracted) to convert PTS of input
        /// frames into a time frame where PTS=0 represents first frame.
        offset: Duration,
    },
    Done,
}

impl<Payload: InputProcessorMediaExt> InputProcessor<Payload> {
    pub(super) fn new(
        buffer_duration: Duration,
        clock: Clock,
        input_id: InputId,
        event_emitter: Arc<EventEmitter>,
    ) -> Self {
        Self {
            buffer_duration,
            start_time: None,
            state: InputState::WaitingForStart,
            clock,
            input_id,
            event_emitter,
        }
    }

    pub(super) fn start_time(&self) -> Option<Instant> {
        self.start_time
    }

    pub(super) fn did_receive_eos(&self) -> bool {
        matches!(self.state, InputState::Done)
    }

    pub(super) fn process_new_chunk(
        &mut self,
        payload: PipelineEvent<Payload>,
    ) -> VecDeque<Payload> {
        match payload {
            PipelineEvent::Data(chunk) => self.handle_data(chunk),
            PipelineEvent::EOS => self.handle_eos(),
        }
    }

    fn handle_eos(&mut self) -> VecDeque<Payload> {
        match self.state {
            InputState::WaitingForStart => {
                self.state = InputState::Done;
                VecDeque::new()
            }
            InputState::Buffering { ref mut buffer } => {
                let first_pts = buffer.first().map(|(_, p)| *p).unwrap_or(Duration::ZERO);
                let chunks = mem::take(buffer)
                    .into_iter()
                    .map(|(mut buffer, _)| {
                        buffer.apply_offset(first_pts);
                        buffer
                    })
                    .collect();
                self.state = InputState::Done;
                self.start_time = Some(Instant::now());
                chunks
            }
            InputState::Ready { .. } => {
                self.state = InputState::Done;
                VecDeque::new()
            }
            InputState::Done => {
                warn!("Received more than one EOS.");
                VecDeque::new()
            }
        }
    }

    fn handle_data(&mut self, mut payload: Payload) -> VecDeque<Payload> {
        let pts = payload.pts();
        match self.state {
            InputState::WaitingForStart => {
                self.state = InputState::Buffering {
                    buffer: vec![(payload, pts)],
                };
                VecDeque::new()
            }
            InputState::Buffering { ref mut buffer } => {
                buffer.push((payload, pts));
                let first_pts = buffer.first().map(|(_, p)| *p).unwrap_or(Duration::ZERO);
                let last_pts = buffer.last().map(|(_, p)| *p).unwrap_or(Duration::ZERO);
                let buffer_duration = last_pts.saturating_sub(first_pts);

                if buffer_duration < self.buffer_duration {
                    VecDeque::new()
                } else {
                    let offset = first_pts;

                    let chunks = mem::take(buffer)
                        .into_iter()
                        .map(|(mut buffer, _)| {
                            buffer.apply_offset(offset);
                            buffer
                        })
                        .collect();
                    self.state = InputState::Ready { offset };
                    self.start_time = Some(self.clock.now());
                    self.on_ready();
                    chunks
                }
            }
            InputState::Ready { offset, .. } => {
                payload.apply_offset(offset);
                VecDeque::from([payload])
            }
            InputState::Done => {
                warn!("Received chunk after EOS.");
                VecDeque::new()
            }
        }
    }

    fn on_ready(&self) {
        match Payload::media_type() {
            MediaType::Audio => self
                .event_emitter
                .emit(Event::AudioInputStreamDelivered(self.input_id.clone())),
            MediaType::Video => self
                .event_emitter
                .emit(Event::VideoInputStreamDelivered(self.input_id.clone())),
        }
    }
}

pub(super) enum MediaType {
    Audio,
    Video,
}

pub(super) trait InputProcessorMediaExt {
    fn apply_offset(&mut self, offset: Duration);
    fn pts(&self) -> Duration;
    fn media_type() -> MediaType;
}

impl InputProcessorMediaExt for Frame {
    fn apply_offset(&mut self, offset: Duration) {
        self.pts = self.pts.saturating_sub(offset)
    }

    fn pts(&self) -> Duration {
        self.pts
    }

    fn media_type() -> MediaType {
        MediaType::Video
    }
}

impl InputProcessorMediaExt for InputAudioSamples {
    fn apply_offset(&mut self, offset: Duration) {
        self.start_pts = self.start_pts.saturating_sub(offset);
        self.end_pts = self.end_pts.saturating_sub(offset);
    }

    fn pts(&self) -> Duration {
        self.start_pts
    }

    fn media_type() -> MediaType {
        MediaType::Audio
    }
}

#[derive(Debug, Clone)]
pub(super) struct Clock(Arc<AtomicI64>);

impl Clock {
    pub(super) fn new() -> Self {
        Self(Arc::new(AtomicI64::new(0)))
    }

    pub(super) fn update_delay(&self, start_time: Instant, current_pts: Duration) {
        let real_now = Instant::now();
        let queue_now = start_time + current_pts;
        let delay_ns = if queue_now > real_now {
            -(queue_now.duration_since(real_now).as_nanos() as i64)
        } else {
            real_now.duration_since(queue_now).as_nanos() as i64
        };
        self.0.store(delay_ns, Ordering::Relaxed)
    }

    fn now(&self) -> Instant {
        let delay_nanos = self.0.load(Ordering::Relaxed);
        if delay_nanos >= 0 {
            Instant::now() - Duration::from_nanos(delay_nanos as u64)
        } else {
            Instant::now() + Duration::from_nanos(-delay_nanos as u64)
        }
    }
}
