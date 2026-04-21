mod audio_input;
mod audio_queue;
mod queue_input;
mod queue_thread;
mod side_channel;
mod utils;
mod video_input;
mod video_queue;

use std::{
    collections::HashMap,
    fmt::Debug,
    path::Path,
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use crossbeam_channel::{Sender, bounded};
use smelter_render::{FrameSet, Framerate, InputId};

use crate::audio_mixer::InputSamplesSet;

use crate::prelude::*;

pub use self::queue_input::QueueInputOptions;
pub(crate) use self::queue_input::{
    QueueInput, QueueSender, QueueTrackOffset, QueueTrackOptions, WeakQueueInput,
};

use self::{
    audio_queue::AudioQueue,
    queue_thread::{QueueStartEvent, QueueThread},
    video_queue::VideoQueue,
};

const DEFAULT_AUDIO_CHUNK_DURATION: Duration = Duration::from_millis(20); // typical audio packet size

#[derive(Debug, Clone)]
pub struct QueueOptions {
    pub output_framerate: Framerate,
    pub ahead_of_time_processing: bool,
    pub run_late_scheduled_events: bool,
    pub never_drop_output_frames: bool,
    pub side_channel_delay: Duration,
    pub side_channel_socket_dir: Option<Arc<Path>>,
}

impl From<&PipelineOptions> for QueueOptions {
    fn from(opt: &PipelineOptions) -> Self {
        Self {
            output_framerate: opt.output_framerate,

            ahead_of_time_processing: opt.ahead_of_time_processing,
            run_late_scheduled_events: opt.run_late_scheduled_events,
            never_drop_output_frames: opt.never_drop_output_frames,
            side_channel_delay: opt.side_channel_delay,
            side_channel_socket_dir: opt.side_channel_socket_dir.clone(),
        }
    }
}

/// - Queue PTS values start from sync_point (Instant).
/// - Queue start PTS is a duration from sync_point to the start request.
/// - All inputs should produce timestamp relative to zero for each track.
///   - If offset is Pts(Duration::ZERO), then input wants to produce timestamp in sync with real time
///     queue, e.g. important for protocols like WebRTC
///   - In most other cases first PTS of one of the track should be zero and for the other track
///     very close to zero (to account for track synchronization)
/// - New tracks are queued after each other, but caller can force it with `abort_old_tracks`
/// - Input receiver always operate on values relative to zero, it is responsibility of `VideoQueueInput`
///   and `AudioQueueInput` to move queue pts value
/// - TrackOffset is a shared value that needs to be added to Track PTS to go into queue frame of
///   reference. It is the same value for audio and video to preserve synchronization from input.
/// - Before input start
///   - For inputs without offset we remember current pts at a time of first packet.
///     - offset is initialized it return is_ready for the first time
///   - For inputs with offset we only check if it is ready for zero pts
///     - track_offset will never be initialized
/// - After input start
///   - For inputs without offset we remember current pts at a time of first packet.
///     - offset is initialized it return is_ready for the first time
///   - For inputs with offset we only check if it is ready for zero pts
///     - track_offset will be initialized when first packet is received, but current
///       pts does not affect what the offset will be
/// - Side channel
///   - Introduce extra latency SIDE_CHANNEL_DELAY
///   - Inputs that stream side channel
///     - Video/AudioInputReceiver duration should be increased by SIDE_CHANNEL_DELAY
///     - Add SIDE_CHANNEL_DELAY to each element in receiver
///   - Inputs that do not stream side channel
///     - Add SIDE_CHANNEL_DELAY to each element in receiver
///
/// - Example usage scenarios:
///   - MP4 input.
///     - When you seek, create new track with `queue_new_track`, start new threads and
///       call `abort_old_tracks`. Don't use offset unless user provided one
///     - When you loop, create new track but `abort_old_tracks` might not be necessary.
///       - If you use abort some last frames might be lost
///       - If you don't use it is possible based on buffer size that video or audio track might
///         play for a bit longer(this will be a better option when we introduce duration based
///         channels).
///   - RTMP server input
///    - Get effective last pts (should work before start).
///    - Call `queue_new_track` with pts + buffer size as an offset (e.g. default buffer 1sec)
///    - On new connection, you don't know if there is audio and video. Create track with
///      both, drop one if no config after 5 seconds.
///   - WHIP server input
///    - Get effective last pts (should work before start) and instant at the time.
///    - RtpNtpSyncTime should extrapolate PTS values from that pair
///    - On new connection,
pub struct Queue {
    queue_ctx: QueueContext,
    video_queue: Mutex<VideoQueue>,
    audio_queue: Mutex<AudioQueue>,
    inputs: Mutex<HashMap<InputId, QueueInput>>,

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
    start_sender: Mutex<Option<Sender<QueueStartEvent>>>,
    scheduled_event_sender: Sender<ScheduledEvent>,

    should_close: AtomicBool,
}

#[derive(Debug, Clone)]
pub struct QueueContext {
    pub sync_point: Instant,
    /// Duration since sync point, represents time of
    /// the queue start
    start_pts: SharedPts,
    last_pts: SharedPts,
    side_channel_delay: Duration,
    pub(crate) side_channel_socket_dir: Option<Arc<Path>>,
}

impl QueueContext {
    pub(crate) fn effective_last_pts(&self) -> Duration {
        self.last_pts
            .value()
            .unwrap_or_else(|| self.sync_point.elapsed())
    }
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
    pub(crate) fn new(opts: QueueOptions) -> Arc<Self> {
        let queue_ctx = QueueContext {
            sync_point: Instant::now(),
            start_pts: Default::default(),
            last_pts: Default::default(),
            side_channel_delay: opts.side_channel_delay,
            side_channel_socket_dir: opts.side_channel_socket_dir,
        };
        let (queue_start_sender, queue_start_receiver) = bounded(0);
        let (scheduled_event_sender, scheduled_event_receiver) = bounded(0);
        let queue = Arc::new(Queue {
            queue_ctx: queue_ctx.clone(),
            video_queue: Mutex::new(VideoQueue::new(
                queue_ctx.sync_point,
                opts.ahead_of_time_processing,
            )),
            audio_queue: Mutex::new(AudioQueue::new(
                queue_ctx.sync_point,
                opts.ahead_of_time_processing,
            )),
            inputs: Mutex::new(HashMap::new()),

            output_framerate: opts.output_framerate,
            audio_chunk_duration: DEFAULT_AUDIO_CHUNK_DURATION,

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

    pub fn ctx(&self) -> QueueContext {
        self.queue_ctx.clone()
    }

    pub fn shutdown(&self) {
        self.should_close.store(true, Ordering::Relaxed)
    }

    pub(crate) fn add_input(&self, input_id: &InputId, queue_input: QueueInput) {
        let weak = queue_input.downgrade();

        self.video_queue
            .lock()
            .unwrap()
            .add_input(input_id, weak.clone());
        self.audio_queue.lock().unwrap().add_input(input_id, weak);

        self.inputs
            .lock()
            .unwrap()
            .insert(input_id.clone(), queue_input);
    }

    pub fn remove_input(&self, input_id: &InputId) {
        self.inputs.lock().unwrap().remove(input_id);
        self.video_queue.lock().unwrap().remove_input(input_id);
        self.audio_queue.lock().unwrap().remove_input(input_id);
    }

    pub(super) fn start(
        self: &Arc<Self>,
        video_sender: Sender<QueueVideoOutput>,
        audio_sender: Sender<QueueAudioOutput>,
    ) {
        if let Some(sender) = self.start_sender.lock().unwrap().take() {
            let queue_start_pts = self.queue_ctx.sync_point.elapsed();
            self.queue_ctx.start_pts.update(queue_start_pts);
            sender
                .send(QueueStartEvent {
                    audio_sender,
                    video_sender,
                    queue_start_pts,
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

#[derive(Debug, Default, Clone)]
pub(super) struct SharedPts(Arc<RwLock<Option<Duration>>>);

impl SharedPts {
    fn update(&self, pts: Duration) {
        *self.0.write().unwrap() = Some(pts);
    }

    fn value(&self) -> Option<Duration> {
        *self.0.read().unwrap()
    }
}
