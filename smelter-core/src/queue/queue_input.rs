use std::{
    ops::DerefMut,
    sync::{Arc, Mutex, Weak},
    time::Duration,
};

use crossbeam_channel::{Receiver, Sender, bounded};
use smelter_render::InputId;
use tracing::info;

use crate::{
    event::EventEmitter,
    queue::{
        QueueContext,
        audio_input::AudioQueueInput,
        side_channel::{AudioSideChannel, VideoSideChannel},
        utils::PauseState,
        video_input::VideoQueueInput,
    },
    types::Ref,
};

use crate::prelude::*;

/// Maximum number of tracks waiting to be started. `QueueInput::queue_new_track`
/// blocks until there is room.
const MAX_PENDING_TRACKS: usize = 5;

struct PendingTrack {
    video: Option<VideoQueueInput>,
    audio: Option<AudioQueueInput>,
    track_offset: TrackOffset,
}

pub(crate) struct QueueSender<T>(crossbeam_channel::Sender<T>);

impl<T> QueueSender<T> {
    pub(crate) fn new(sender: crossbeam_channel::Sender<T>) -> Self {
        Self(sender)
    }

    pub fn send(&self, item: T) -> Result<(), crossbeam_channel::SendError<T>> {
        self.0.send(item)
    }

    #[allow(dead_code)]
    pub fn try_send(&self, item: T) -> Result<(), crossbeam_channel::TrySendError<T>> {
        self.0.try_send(item)
    }
}

impl<T> std::fmt::Debug for QueueSender<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QueueSender").finish()
    }
}

pub(super) struct InnerQueueInput {
    queue_ctx: QueueContext,
    event_emitter: Arc<EventEmitter>,
    input_ref: Ref<InputId>,

    video: Option<VideoQueueInput>,
    audio: Option<AudioQueueInput>,
    track_offset: TrackOffset,
    pause_state: PauseState,

    pending_sender: crossbeam_channel::Sender<PendingTrack>,
    pending_receiver: crossbeam_channel::Receiver<PendingTrack>,
    required: bool,
    video_side_channel: Option<VideoSideChannel>,
    audio_side_channel: Option<AudioSideChannel>,
    side_channel_delay: Duration,
}

impl InnerQueueInput {
    fn maybe_start_next_track(&mut self) {
        let video_eos_sent = self.video.as_ref().map(|v| v.eos_sent()).unwrap_or(true);
        let audio_eos_sent = self.audio.as_ref().map(|a| a.eos_sent()).unwrap_or(true);
        if video_eos_sent && audio_eos_sent {
            self.replace_track()
        }
    }

    /// Replace current track with the next pending, do nothing if there is no pending
    fn replace_track(&mut self) {
        let Ok(pending) = self.pending_receiver.try_recv() else {
            return;
        };
        info!(input_id=%self.input_ref, "Push track to queue");

        self.video = pending.video;
        self.audio = pending.audio;
        self.track_offset = pending.track_offset;
        if self.pause_state.is_paused() {
            let pts = self.queue_ctx.effective_last_pts();
            if let Some(v) = self.video.as_mut() {
                // trigger enqueue so new track can start with a frame
                match self.queue_ctx.start_pts.value() {
                    Some(start_pts) => {
                        v.is_ready_for_pts(pts, start_pts);
                    }
                    None => v.drop_old_frames_before_start(),
                };
                v.pause()
            }
            if let Some(a) = self.audio.as_mut() {
                a.pause()
            }
            self.pause_state.reset(pts);
        }
    }

    fn new_pending_track(
        &self,
        opts: QueueTrackOptions,
    ) -> (
        PendingTrack,
        Option<QueueSender<Frame>>,
        Option<QueueSender<InputAudioSamples>>,
    ) {
        let input_id = self.input_ref.to_string();
        info!(?opts, input_id, "Create new queue track");
        let (track_offset, offset_from_start) = match opts.offset {
            QueueTrackOffset::None => (TrackOffset::default(), None),
            QueueTrackOffset::Pts(duration) => (TrackOffset::new(duration), None),
            QueueTrackOffset::FromStart(duration) => (TrackOffset::default(), Some(duration)),
        };
        let (video_input, video_sender) = if opts.video {
            let side_channel = self
                .video_side_channel
                .as_ref()
                .map(|sc| sc.with_track_offset(&track_offset));
            let (video_input, video_sender) = VideoQueueInput::new(
                &self.queue_ctx,
                &self.event_emitter,
                &self.input_ref,
                self.required,
                offset_from_start,
                track_offset.clone(),
                side_channel,
                self.side_channel_delay,
            );
            (Some(video_input), Some(QueueSender::new(video_sender)))
        } else {
            (None, None)
        };
        let (audio_input, audio_sender) = if opts.audio {
            let side_channel = self
                .audio_side_channel
                .as_ref()
                .map(|sc| sc.with_track_offset(&track_offset));
            let (audio_input, audio_sender) = AudioQueueInput::new(
                &self.queue_ctx,
                &self.event_emitter,
                &self.input_ref,
                self.required,
                offset_from_start,
                track_offset.clone(),
                side_channel,
                self.side_channel_delay,
            );
            (Some(audio_input), Some(QueueSender::new(audio_sender)))
        } else {
            (None, None)
        };
        (
            PendingTrack {
                video: video_input,
                audio: audio_input,
                track_offset,
            },
            video_sender,
            audio_sender,
        )
    }

    /// Remember the start pts. On resume shift offset by the pts difference:
    /// - If input already started, add to track offset pts diff
    /// - If input did not started, track_offset was not initialized yet
    pub fn pause(&mut self) {
        if self.pause_state.is_paused() {
            return;
        }
        // zero before queue start
        let pts = self.queue_ctx.effective_last_pts();
        self.pause_state.pause(pts);
        if let Some(v) = self.video.as_mut() {
            v.pause()
        }
        if let Some(a) = self.audio.as_mut() {
            a.pause()
        }
    }

    pub fn resume(&mut self) {
        if !self.pause_state.is_paused() {
            return;
        }
        let pts = self.queue_ctx.effective_last_pts();
        if let Some(pause_time) = self.pause_state.resume(pts) {
            self.track_offset.map_add(pause_time);
        }
        if let Some(v) = self.video.as_mut() {
            v.resume()
        }
        if let Some(a) = self.audio.as_mut() {
            a.resume()
        }
    }
}

#[derive(Debug)]
pub(crate) enum QueueTrackOffset {
    None,
    /// Effectively offset from sync point
    Pts(Timestamp),
    /// Offset from start point
    FromStart(Duration),
}

#[derive(Debug)]
pub(crate) struct QueueTrackOptions {
    pub video: bool,
    pub audio: bool,
    pub offset: QueueTrackOffset,
}

#[derive(Clone)]
pub(crate) struct QueueInput(Arc<Mutex<InnerQueueInput>>);

#[derive(Clone)]
pub(crate) struct WeakQueueInput(Weak<Mutex<InnerQueueInput>>);

impl std::fmt::Debug for WeakQueueInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WeakQueueInput").finish()
    }
}

#[derive(Debug, Default, Clone)]
pub enum InputSideChannel<T> {
    #[default]
    Disabled,
    UnixSocket,
    Native(Sender<T>),
}

impl<T> InputSideChannel<T> {
    pub fn native(capacity: usize) -> (Self, Receiver<T>) {
        let (sender, receiver) = bounded(capacity);
        (Self::Native(sender), receiver)
    }
}

impl<T> PartialEq for InputSideChannel<T> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Disabled, Self::Disabled) | (Self::UnixSocket, Self::UnixSocket) => true,
            (Self::Native(left), Self::Native(right)) => left.same_channel(right),
            _ => false,
        }
    }
}

impl<T> From<bool> for InputSideChannel<T> {
    fn from(enabled: bool) -> Self {
        match enabled {
            true => Self::UnixSocket,
            false => Self::Disabled,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct QueueInputOptions {
    pub required: bool,
    pub audio_side_channel: InputSideChannel<InputAudioSamples>,
    pub video_side_channel: InputSideChannel<Frame>,
    pub side_channel_delay: Duration,
}

impl QueueInput {
    pub fn new(ctx: &Arc<PipelineCtx>, input_ref: &Ref<InputId>, opts: QueueInputOptions) -> Self {
        let socket_dir = ctx.queue_ctx.side_channel_socket_dir.as_deref();
        let video_side_channel = match (&opts.video_side_channel, socket_dir) {
            (InputSideChannel::UnixSocket, Some(dir)) => {
                VideoSideChannel::unix_socket(ctx, input_ref, dir)
            }
            (InputSideChannel::Native(sender), _) => {
                Some(VideoSideChannel::native(ctx, sender.clone()))
            }
            _ => None,
        };
        let audio_side_channel = match (&opts.audio_side_channel, socket_dir) {
            (InputSideChannel::UnixSocket, Some(dir)) => {
                AudioSideChannel::unix_socket(ctx, input_ref, dir)
            }
            (InputSideChannel::Native(sender), _) => {
                Some(AudioSideChannel::native(ctx, sender.clone()))
            }
            _ => None,
        };
        Self::new_inner(
            ctx.queue_ctx.clone(),
            ctx.event_emitter.clone(),
            input_ref,
            opts,
            video_side_channel,
            audio_side_channel,
        )
    }

    pub(super) fn new_inner(
        queue_ctx: QueueContext,
        event_emitter: Arc<EventEmitter>,
        input_ref: &Ref<InputId>,
        opts: QueueInputOptions,
        video_side_channel: Option<VideoSideChannel>,
        audio_side_channel: Option<AudioSideChannel>,
    ) -> Self {
        let (pending_sender, pending_receiver) = crossbeam_channel::bounded(MAX_PENDING_TRACKS);
        Self(Arc::new(Mutex::new(InnerQueueInput {
            queue_ctx,
            event_emitter,
            input_ref: input_ref.clone(),

            video: None,
            audio: None,
            track_offset: TrackOffset::default(),

            pending_sender,
            pending_receiver,

            required: opts.required,
            pause_state: PauseState::new(),
            video_side_channel,
            audio_side_channel,
            side_channel_delay: opts.side_channel_delay,
        })))
    }

    /// Blocks (without holding the inner mutex) if `MAX_PENDING_TRACKS` tracks
    /// are already pending, until some of them are dequeued.
    pub fn queue_new_track(
        &self,
        opts: QueueTrackOptions,
    ) -> (
        Option<QueueSender<Frame>>,
        Option<QueueSender<InputAudioSamples>>,
    ) {
        if !opts.video && !opts.audio {
            return (None, None);
        }
        let guard = self.0.lock().unwrap();
        let (track, video_sender, audio_sender) = guard.new_pending_track(opts);
        let pending_sender = guard.pending_sender.clone();
        drop(guard);
        // receiver is owned by InnerQueueInput, so send can't fail while
        // we hold the Arc
        let _ = pending_sender.send(track);
        (video_sender, audio_sender)
    }

    pub fn abort_old_track(&self) {
        self.0.lock().unwrap().replace_track()
    }

    pub fn pause(&self) {
        self.0.lock().unwrap().pause();
    }

    pub fn resume(&self) {
        self.0.lock().unwrap().resume();
    }

    pub fn downgrade(&self) -> WeakQueueInput {
        WeakQueueInput(Arc::downgrade(&self.0))
    }

    pub(super) fn maybe_start_next_track(&self) {
        self.0.lock().unwrap().maybe_start_next_track();
    }
}

impl WeakQueueInput {
    pub(super) fn video<F, R>(&self, f: F) -> Option<R>
    where
        F: FnOnce(&mut VideoQueueInput) -> R,
    {
        let arc = self.0.upgrade()?;
        let mut inner = arc.lock().unwrap();
        let video = inner.video.as_mut()?;
        Some(f(video))
    }

    pub(super) fn audio<F, R>(&self, f: F) -> Option<R>
    where
        F: FnOnce(&mut AudioQueueInput) -> R,
    {
        let arc = self.0.upgrade()?;
        let mut inner = arc.lock().unwrap();
        let audio = inner.audio.as_mut()?;
        Some(f(audio))
    }

    pub(crate) fn upgrade(&self) -> Option<QueueInput> {
        self.0.upgrade().map(QueueInput)
    }
}

#[derive(Default, Clone)]
pub(super) struct TrackOffset(Arc<Mutex<Option<Timestamp>>>);

impl TrackOffset {
    pub fn new(value: Timestamp) -> Self {
        Self(Arc::new(Mutex::new(Some(value))))
    }

    pub fn get(&self) -> Option<Timestamp> {
        *self.0.lock().unwrap()
    }

    pub fn get_or_init(&self, offset: Timestamp) -> Timestamp {
        *self.0.lock().unwrap().get_or_insert(offset)
    }

    pub fn map_add(&self, delta: Timestamp) {
        if let Some(offset) = self.0.lock().unwrap().deref_mut() {
            *offset += delta
        }
    }
}
