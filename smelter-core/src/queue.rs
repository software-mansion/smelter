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

/// - Queue PTS values are measured from `sync_point` (an `Instant` captured at Queue
///   construction).
/// - `start_pts` is the duration from `sync_point` to the moment `start` is called.
/// - Inputs produce per-track PTS relative to the track's own zero. Moving those into the
///   queue's frame of reference is the job of `VideoQueueInput` / `AudioQueueInput`.
///   - `QueueTrackOffset::Pts(Duration::ZERO)` means the track's zero should line up with
///     the queue's `sync_point` — i.e. the track produces timestamps in sync with wall clock
///     (used by realtime protocols like WebRTC, V4L2, DeckLink).
///   - `QueueTrackOffset::Pts(d)` shifts the track so its zero maps to `sync_point + d`
///     (RTMP uses `effective_last_pts + RTMP_BUFFER` to leave room for decoding).
///   - `QueueTrackOffset::FromStart(d)` places the track's zero at `start_pts + d`.
///   - `QueueTrackOffset::None` defers: the offset is fixed on the first received packet.
/// - `TrackOffset` is shared between a track's video and audio so their synchronization
///   from the input side is preserved.
/// - Multiple tracks can be queued back-to-back via `queue_new_track`; the next one starts
///   once the current one is done. Callers can force an early swap with `abort_old_track`.
///
/// - Offset initialization:
///   - `Pts(d)`: `track_offset` is set to `d` at construction; it never changes.
///   - `FromStart(d)`: `track_offset` is initialized to `start_pts + d` when the first
///     packet is received after queue start.
///   - `None`: `track_offset` is initialized on the first packet — before queue start to
///     `sync_point.elapsed()` at that moment (via `drop_old_frames_before_start`), after
///     queue start to the current queue PTS at which the packet is observed.
///
/// - Side channel (`side_channel_delay`):
///   - Every input receiver (both with and without a side channel) shifts incoming PTS
///     by `side_channel_delay`, so the whole pipeline runs that far behind the inputs.
///   - Inputs with a side channel additionally allow their receiver buffer to grow up to
///     `side_channel_delay` worth of data, so the side-channel subscriber receives frames
///     ahead of when the queue consumes them — leaving the subscriber roughly
///     `side_channel_delay` time to process before the frame is due.
///
/// - Example usage scenarios:
///   - MP4 input:
///     - On seek, create a new track with `queue_new_track`, start new reader threads, then
///       call `abort_old_track` to switch immediately.
///     - On loop, create a new track; `abort_old_track` is optional. Skipping it may leak a
///       few extra frames from the previous iteration depending on buffer size.
///   - RTMP server input:
///     - Read `effective_last_pts` (valid before and after start).
///     - Register a track with `QueueTrackOffset::Pts(effective_last_pts + RTMP_BUFFER)`
///       (`RTMP_BUFFER` is currently 2s) so decoded data has time to land.
///     - Create the track with both audio and video senders; drop the unused one if no
///       config arrives within 5s.
///   - WHIP / WHEP / V4L2 / DeckLink:
///     - Register a track with `QueueTrackOffset::Pts(Duration::ZERO)` so input PTS is
///       aligned to `sync_point`.
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
        self.should_close.store(true, Ordering::Relaxed);
        // Drop all input receivers so any decoder/depayloader thread blocked on
        // QueueSender::send() unblocks with SendError and can exit. Without this,
        // the senders sit in QueueInput inside the HashMap, the HashMap doesn't
        // drop until Pipeline.queue drops (after Pipeline::drop returns), and
        // join_all in Pipeline::drop deadlocks waiting for those threads.
        self.inputs.lock().unwrap().clear();
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
