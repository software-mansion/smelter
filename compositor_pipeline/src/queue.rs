mod audio_queue;
mod queue_thread;
mod utils;
mod video_queue;

use std::{
    collections::HashMap,
    fmt::Debug,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use compositor_render::{Frame, FrameSet, Framerate, InputId};
use crossbeam_channel::{bounded, Receiver, Sender};

use crate::audio_mixer::InputSamplesSet;

use crate::prelude::*;

use self::{
    audio_queue::AudioQueue,
    queue_thread::{QueueStartEvent, QueueThread},
    utils::Clock,
    video_queue::VideoQueue,
};

const DEFAULT_AUDIO_CHUNK_DURATION: Duration = Duration::from_millis(20); // typical audio packet size

#[derive(Debug)]
pub struct QueueDataReceiver {
    pub video: Option<Receiver<PipelineEvent<Frame>>>,
    pub audio: Option<Receiver<PipelineEvent<InputAudioSamples>>>,
}

#[derive(Debug, Clone, Copy)]
pub struct QueueOptions {
    pub default_buffer_duration: Duration,
    pub output_framerate: Framerate,

    pub ahead_of_time_processing: bool,
    pub run_late_scheduled_events: bool,
    pub never_drop_output_frames: bool,
}

impl From<&PipelineOptions> for QueueOptions {
    fn from(opt: &PipelineOptions) -> Self {
        Self {
            default_buffer_duration: opt.default_buffer_duration,
            output_framerate: opt.output_framerate,

            ahead_of_time_processing: opt.ahead_of_time_processing,
            run_late_scheduled_events: opt.run_late_scheduled_events,
            never_drop_output_frames: opt.never_drop_output_frames,
        }
    }
}

/// Queue is responsible for consuming frames from different inputs and producing
/// sets of frames from all inputs in a single batch.
///
/// - PTS of inputs streams can be in any frame of reference.
/// - PTS of frames stored in queue are in a frame of reference where PTS=0 represents.
///   first frame/packet.
/// - PTS of output frames is in a frame of reference where PTS=0 represents
///   start request.
pub struct Queue {
    video_queue: Mutex<VideoQueue>,
    audio_queue: Mutex<AudioQueue>,

    output_framerate: Framerate,

    /// Duration of queue output samples set.
    audio_chunk_duration: Duration,

    /// Define if queue should process frames if all inputs are ready.
    ahead_of_time_processing: bool,
    /// If true do not drop output frames even if queue is behind the
    /// real time clock.
    never_drop_output_frames: bool,

    /// Defines behavior when event is scheduled too late:
    /// true - Event will be executed immediately.
    /// false - Event will be discarded.
    run_late_scheduled_events: bool,

    default_buffer_duration: Duration,

    start_time: Mutex<Option<Instant>>,
    start_sender: Mutex<Option<Sender<QueueStartEvent>>>,
    scheduled_event_sender: Sender<ScheduledEvent>,

    clock: Clock,

    should_close: AtomicBool,
}

#[derive(Debug)]
pub(super) struct QueueVideoOutput {
    pub(super) pts: Duration,
    pub(super) frames: HashMap<InputId, PipelineEvent<Frame>>,
}

impl From<QueueVideoOutput> for FrameSet<InputId> {
    fn from(value: QueueVideoOutput) -> Self {
        Self {
            frames: value
                .frames
                .into_iter()
                .filter_map(|(key, value)| match value {
                    PipelineEvent::Data(data) => Some((key, data)),
                    PipelineEvent::EOS => None,
                })
                .collect(),
            pts: value.pts,
        }
    }
}

#[derive(Debug)]
pub(super) struct QueueAudioOutput {
    pub samples: HashMap<InputId, PipelineEvent<Vec<InputAudioSamples>>>,
    pub start_pts: Duration,
    pub end_pts: Duration,
}

impl From<QueueAudioOutput> for InputSamplesSet {
    fn from(value: QueueAudioOutput) -> Self {
        Self {
            samples: value
                .samples
                .into_iter()
                .filter_map(|(key, value)| match value {
                    PipelineEvent::Data(data) => Some((key, data)),
                    PipelineEvent::EOS => None,
                })
                .collect(),
            start_pts: value.start_pts,
            end_pts: value.end_pts,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct InputOptions {
    required: bool,
    offset: Option<Duration>,
    buffer_duration: Duration,
}

pub struct ScheduledEvent {
    pts: Duration,
    callback: Box<dyn FnOnce() + Send>,
}

impl<T: Clone> Clone for PipelineEvent<T> {
    fn clone(&self) -> Self {
        match self {
            PipelineEvent::Data(data) => PipelineEvent::Data(data.clone()),
            PipelineEvent::EOS => PipelineEvent::EOS,
        }
    }
}

impl Queue {
    pub(crate) fn new(opts: QueueOptions, ctx: &Arc<PipelineCtx>) -> Arc<Self> {
        let (queue_start_sender, queue_start_receiver) = bounded(0);
        let (scheduled_event_sender, scheduled_event_receiver) = bounded(0);
        let queue = Arc::new(Queue {
            video_queue: Mutex::new(VideoQueue::new(ctx.event_emitter.clone())),
            output_framerate: opts.output_framerate,

            audio_queue: Mutex::new(AudioQueue::new(ctx.event_emitter.clone())),
            audio_chunk_duration: DEFAULT_AUDIO_CHUNK_DURATION,

            start_time: Mutex::new(None),

            scheduled_event_sender,
            start_sender: Mutex::new(Some(queue_start_sender)),
            ahead_of_time_processing: opts.ahead_of_time_processing,
            never_drop_output_frames: opts.never_drop_output_frames,
            run_late_scheduled_events: opts.run_late_scheduled_events,
            default_buffer_duration: opts.default_buffer_duration,

            clock: Clock::new(),
            should_close: AtomicBool::new(false),
        });

        QueueThread::new(
            queue.clone(),
            queue_start_receiver,
            scheduled_event_receiver,
        )
        .spawn();

        queue
    }

    pub fn shutdown(&self) {
        self.should_close.store(true, Ordering::Relaxed)
    }

    pub fn add_input(
        &self,
        input_id: &InputId,
        receiver: QueueDataReceiver,
        opts: QueueInputOptions,
    ) {
        let input_options = InputOptions {
            required: opts.required,
            offset: opts.offset,
            buffer_duration: opts.buffer_duration.unwrap_or(self.default_buffer_duration),
        };

        if let Some(receiver) = receiver.video {
            self.video_queue.lock().unwrap().add_input(
                input_id,
                receiver,
                input_options,
                self.clock.clone(),
            );
        };
        if let Some(receiver) = receiver.audio {
            self.audio_queue.lock().unwrap().add_input(
                input_id,
                receiver,
                input_options,
                self.clock.clone(),
            );
        }
    }

    pub fn remove_input(&self, input_id: &InputId) {
        self.video_queue.lock().unwrap().remove_input(input_id);
        self.audio_queue.lock().unwrap().remove_input(input_id);
    }

    pub(super) fn start(
        self: &Arc<Self>,
        video_sender: Sender<QueueVideoOutput>,
        audio_sender: Sender<QueueAudioOutput>,
    ) {
        if let Some(sender) = self.start_sender.lock().unwrap().take() {
            let start_time = Instant::now();
            *self.start_time.lock().unwrap() = Some(start_time);
            sender
                .send(QueueStartEvent {
                    audio_sender,
                    video_sender,
                    start_time,
                })
                .unwrap()
        }
    }

    pub fn schedule_event(&self, pts: Duration, callback: Box<dyn FnOnce() + Send>) {
        self.scheduled_event_sender
            .send(ScheduledEvent { pts, callback })
            .unwrap();
    }
}

impl Debug for ScheduledEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScheduledEvent")
            .field("pts", &self.pts)
            .field("callback", &"<callback>".to_string())
            .finish()
    }
}
