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

use crossbeam_channel::{bounded, Receiver, Sender};
use smelter_render::{Frame, FrameSet, Framerate, InputId};

use crate::audio_mixer::InputSamplesSet;

use crate::prelude::*;

use self::{
    audio_queue::AudioQueue,
    queue_thread::{QueueStartEvent, QueueThread},
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
    pub output_framerate: Framerate,
    pub ahead_of_time_processing: bool,
    pub run_late_scheduled_events: bool,
    pub never_drop_output_frames: bool,
}

impl From<&PipelineOptions> for QueueOptions {
    fn from(opt: &PipelineOptions) -> Self {
        Self {
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
/// - All PTS values represent duration from sync_point stored in pipeline.
///   (That will mean that pts of queue start will not be zero)
///   - If input has offset defined it is applied in queue, timestamps before represent
///     packets as if there was no offset
///   - offset value is counted from `start`, to calculate offset_pts you need to add queue_start_pts
/// - Conversion from/to this time frame should happen
///   - on input as early as possible (before decoder when handling protocol/container)
///   - on output as late as possible (after encoder when handling protocol/container)
/// - All public timestamp values should refer to time after start.
///   `queue_start_pts = sync_point - queue_start_time`
///   `public_pts = internal_pts - queue_start_pts`
pub struct Queue {
    video_queue: Mutex<VideoQueue>,
    audio_queue: Mutex<AudioQueue>,

    output_framerate: Framerate,

    /// Duration of queue output samples set.
    audio_chunk_duration: Duration,

    /// If true do not drop output frames even if queue is behind the
    /// real time clock.
    never_drop_output_frames: bool,

    /// Defines behavior when event is scheduled too late:
    /// true - Event will be executed immediately.
    /// false - Event will be discarded.
    run_late_scheduled_events: bool,

    sync_point: Instant,
    /// Duration since sync point, represents time of
    /// the queue start
    start_time_pts: Mutex<Option<Duration>>,
    start_sender: Mutex<Option<Sender<QueueStartEvent>>>,
    scheduled_event_sender: Sender<ScheduledEvent>,

    should_close: AtomicBool,
}

#[derive(Debug)]
pub(super) struct QueueVideoOutput {
    // If required this batch can't be dropped even if processing is behind
    pub(super) required: bool,
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
    pub required: bool,
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

pub struct ScheduledEvent {
    /// Public PTS value (relative to start, not to the sync_point)
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
        let sync_point = ctx.queue_sync_point;
        let queue = Arc::new(Queue {
            video_queue: Mutex::new(VideoQueue::new(
                sync_point,
                ctx.event_emitter.clone(),
                opts.ahead_of_time_processing,
            )),
            output_framerate: opts.output_framerate,

            audio_queue: Mutex::new(AudioQueue::new(
                sync_point,
                ctx.event_emitter.clone(),
                opts.ahead_of_time_processing,
            )),
            audio_chunk_duration: DEFAULT_AUDIO_CHUNK_DURATION,

            sync_point,
            start_time_pts: Mutex::new(None),

            scheduled_event_sender,
            start_sender: Mutex::new(Some(queue_start_sender)),
            never_drop_output_frames: opts.never_drop_output_frames,
            run_late_scheduled_events: opts.run_late_scheduled_events,

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
        let shared_state = SharedState::default();
        if let Some(receiver) = receiver.video {
            self.video_queue.lock().unwrap().add_input(
                input_id,
                receiver,
                opts,
                shared_state.clone(),
            );
        };
        if let Some(receiver) = receiver.audio {
            self.audio_queue.lock().unwrap().add_input(
                input_id,
                receiver,
                opts,
                shared_state.clone(),
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
            let start_time_pts = Instant::now().duration_since(self.sync_point);
            *self.start_time_pts.lock().unwrap() = Some(start_time_pts);
            sender
                .send(QueueStartEvent {
                    audio_sender,
                    video_sender,
                    start_time_pts,
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

// struct that holds first PTS of either video or audio track,
// when input has an offset defined, this value can be used to calculate
// new PTS values while preserving synchronization between tracks
#[derive(Default, Clone)]
struct SharedState(Arc<Mutex<Option<Duration>>>);

impl SharedState {
    fn get_or_init_first_pts(&self, pts: Duration) -> Duration {
        *self.0.lock().unwrap().get_or_insert(pts)
    }

    fn first_pts(&self) -> Option<Duration> {
        *self.0.lock().unwrap()
    }
}
