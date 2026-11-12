use std::{
    collections::VecDeque,
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
        audio_track::AudioQueueTrack,
        side_channel::{AudioSideChannel, VideoSideChannel},
        utils::PauseState,
        video_track::VideoQueueTrack,
    },
    types::Ref,
};

use crate::prelude::*;

/// Maximum number of track groups waiting to be started. `QueueInput::queue_new_track_group`
/// blocks until there is room.
const MAX_PENDING_TRACK_GROUPS: usize = 5;

/// Pending tracks start once their group is current and the previous track of the same kind
/// delivered EOS.
#[derive(Default)]
struct TrackGroupState {
    pending_video: VecDeque<VideoQueueTrack>,
    pending_audio: VecDeque<AudioQueueTrack>,
    group_offset: TrackGroupOffset,
    offset_from_start: Option<Duration>,
}

impl TrackGroupState {
    fn new(offset: QueueTrackGroupOffset) -> Self {
        let (group_offset, offset_from_start) = match offset {
            QueueTrackGroupOffset::None => (TrackGroupOffset::default(), None),
            QueueTrackGroupOffset::Pts(duration) => (TrackGroupOffset::new(duration), None),
            QueueTrackGroupOffset::FromStart(duration) => {
                (TrackGroupOffset::default(), Some(duration))
            }
        };
        Self {
            group_offset,
            offset_from_start,
            ..Default::default()
        }
    }

    fn add_video_track(&mut self, input: &InnerQueueInput) -> QueueSender<Frame> {
        let side_channel = input
            .video_side_channel
            .as_ref()
            .map(|sc| sc.with_group_offset(&self.group_offset));
        let (video, sender) = VideoQueueTrack::new(
            &input.queue_ctx,
            &input.event_emitter,
            &input.input_ref,
            input.required,
            self.offset_from_start,
            self.group_offset.clone(),
            side_channel,
            input.side_channel_delay,
        );
        self.pending_video.push_back(video);
        QueueSender::new(sender)
    }

    fn add_audio_track(&mut self, input: &InnerQueueInput) -> QueueSender<InputAudioSamples> {
        let side_channel = input
            .audio_side_channel
            .as_ref()
            .map(|sc| sc.with_group_offset(&self.group_offset));
        let (audio, sender) = AudioQueueTrack::new(
            &input.queue_ctx,
            &input.event_emitter,
            &input.input_ref,
            input.required,
            self.offset_from_start,
            self.group_offset.clone(),
            side_channel,
            input.side_channel_delay,
        );
        self.pending_audio.push_back(audio);
        QueueSender::new(sender)
    }
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

    pub fn into_inner(self) -> crossbeam_channel::Sender<T> {
        self.0
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

    video: Option<VideoQueueTrack>,
    audio: Option<AudioQueueTrack>,
    /// Current track group.
    group: Arc<Mutex<TrackGroupState>>,
    pause_state: PauseState,

    pending_sender: crossbeam_channel::Sender<Arc<Mutex<TrackGroupState>>>,
    pending_receiver: crossbeam_channel::Receiver<Arc<Mutex<TrackGroupState>>>,
    required: bool,
    stale_frame_timeout: Option<Duration>,
    video_side_channel: Option<VideoSideChannel>,
    audio_side_channel: Option<AudioSideChannel>,
    side_channel_delay: Duration,
}

impl InnerQueueInput {
    fn maybe_start_next_track_group(&mut self) {
        self.maybe_start_pending_tracks_in_current_group();
        let video_eos_sent = self.video.as_ref().map(|v| v.eos_sent()).unwrap_or(true);
        let audio_eos_sent = self.audio.as_ref().map(|a| a.eos_sent()).unwrap_or(true);
        if video_eos_sent && audio_eos_sent {
            self.replace_track_group()
        }
    }

    /// Start pending tracks of the current group if the previous track of the same kind is done.
    fn maybe_start_pending_tracks_in_current_group(&mut self) {
        let video_done = self.video.as_ref().is_none_or(|v| v.eos_sent());
        let audio_done = self.audio.as_ref().is_none_or(|a| a.eos_sent());
        let (video, audio) = {
            let mut group = self.group.lock().unwrap();
            (
                video_done
                    .then(|| group.pending_video.pop_front())
                    .flatten(),
                audio_done
                    .then(|| group.pending_audio.pop_front())
                    .flatten(),
            )
        };
        if let Some(mut video) = video {
            info!(input_id=%self.input_ref, "Start video track");
            if let Some(timeout) = self.stale_frame_timeout {
                video.set_stale_frame_timeout(timeout);
            }
            if self.pause_state.is_paused() {
                // trigger enqueue so new track can start with a frame
                match self.queue_ctx.start_pts.value() {
                    Some(start_pts) => {
                        video.is_ready_for_pts(self.queue_ctx.effective_last_pts(), start_pts);
                    }
                    None => video.drop_old_frames_before_start(),
                };
                video.pause()
            }
            self.video = Some(video);
        }
        if let Some(mut audio) = audio {
            info!(input_id=%self.input_ref, "Start audio track");
            if self.pause_state.is_paused() {
                audio.pause()
            }
            self.audio = Some(audio);
        }
    }

    /// Replace current track group with the next pending, do nothing if there is no pending
    fn replace_track_group(&mut self) {
        let Ok(pending) = self.pending_receiver.try_recv() else {
            return;
        };
        info!(input_id=%self.input_ref, "Push track group to queue");

        // Dropping the old group ends it, `IndependentTrackGroup` can't add tracks to it anymore.
        self.group = pending;
        self.video = None;
        self.audio = None;
        self.maybe_start_pending_tracks_in_current_group();
        if self.pause_state.is_paused() {
            self.pause_state.reset(self.queue_ctx.effective_last_pts());
        }
    }

    /// Remember the start pts. On resume shift offset by the pts difference:
    /// - If input already started, add pts diff to group offset
    /// - If input did not started, group_offset was not initialized yet
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
            self.group.lock().unwrap().group_offset.map_add(pause_time);
        }
        if let Some(v) = self.video.as_mut() {
            v.resume()
        }
        if let Some(a) = self.audio.as_mut() {
            a.resume()
        }
    }
}

#[derive(Debug, Clone)]
pub enum QueueTrackGroupOffset {
    None,
    /// Effectively offset from sync point
    Pts(Timestamp),
    /// Offset from start point
    FromStart(Duration),
}

#[derive(Debug)]
pub(crate) struct QueueTrackGroupOptions {
    pub video: bool,
    pub audio: bool,
    pub offset: QueueTrackGroupOffset,
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
        let (pending_sender, pending_receiver) =
            crossbeam_channel::bounded(MAX_PENDING_TRACK_GROUPS);
        Self(Arc::new(Mutex::new(InnerQueueInput {
            queue_ctx,
            event_emitter,
            input_ref: input_ref.clone(),

            video: None,
            audio: None,
            group: Default::default(),

            pending_sender,
            pending_receiver,

            required: opts.required,
            stale_frame_timeout: None,
            pause_state: PauseState::new(),
            video_side_channel,
            audio_side_channel,
            side_channel_delay: opts.side_channel_delay,
        })))
    }

    /// Blocks (without holding the inner mutex) if `MAX_PENDING_TRACK_GROUPS` groups
    /// are already pending, until some of them are dequeued.
    pub fn queue_new_track_group(
        &self,
        opts: QueueTrackGroupOptions,
    ) -> (
        Option<QueueSender<Frame>>,
        Option<QueueSender<InputAudioSamples>>,
    ) {
        if !opts.video && !opts.audio {
            return (None, None);
        }
        let guard = self.0.lock().unwrap();
        info!(?opts, input_id=%guard.input_ref, "Create new queue track group");
        let mut group = TrackGroupState::new(opts.offset);
        let video_sender = opts.video.then(|| group.add_video_track(&guard));
        let audio_sender = opts.audio.then(|| group.add_audio_track(&guard));
        let pending_sender = guard.pending_sender.clone();
        drop(guard);
        // receiver is owned by InnerQueueInput, so send can't fail while
        // we hold the Arc
        let _ = pending_sender.send(Arc::new(Mutex::new(group)));
        (video_sender, audio_sender)
    }

    /// Queue a track group that starts without tracks. Tracks are added with
    /// `IndependentTrackGroup::add_video_track`/`add_audio_track` and removed by dropping their
    /// sender. Blocks like `queue_new_track_group`.
    pub fn queue_new_independent_track_group(
        &self,
        offset: QueueTrackGroupOffset,
    ) -> IndependentTrackGroup {
        let guard = self.0.lock().unwrap();
        info!(input_id=%guard.input_ref, ?offset, "Create new independent queue track group");
        let group = Arc::new(Mutex::new(TrackGroupState::new(offset)));
        let handle = IndependentTrackGroup {
            input: self.downgrade(),
            group: Arc::downgrade(&group),
        };
        let pending_sender = guard.pending_sender.clone();
        drop(guard);
        let _ = pending_sender.send(group);
        handle
    }

    pub fn abort_old_track_group(&self) {
        self.0.lock().unwrap().replace_track_group()
    }

    /// Stop rendering the last frame once it is older than `timeout`. Without
    /// it the last frame is rendered until a newer one arrives. Applies to the
    /// current video track and to video tracks started later.
    pub fn set_stale_frame_timeout(&self, timeout: Duration) {
        let mut guard = self.0.lock().unwrap();
        guard.stale_frame_timeout = Some(timeout);
        if let Some(video) = guard.video.as_mut() {
            video.set_stale_frame_timeout(timeout);
        }
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

    pub(super) fn maybe_start_next_track_group(&self) {
        self.0.lock().unwrap().maybe_start_next_track_group();
    }
}

impl WeakQueueInput {
    pub(super) fn video<F, R>(&self, f: F) -> Option<R>
    where
        F: FnOnce(&mut VideoQueueTrack) -> R,
    {
        let arc = self.0.upgrade()?;
        let mut inner = arc.lock().unwrap();
        let video = inner.video.as_mut()?;
        Some(f(video))
    }

    pub(super) fn audio<F, R>(&self, f: F) -> Option<R>
    where
        F: FnOnce(&mut AudioQueueTrack) -> R,
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

#[derive(Debug, thiserror::Error)]
#[error("Track group ended or input was unregistered.")]
pub(crate) struct TrackGroupEndedError;

/// Handle to a track group created with `QueueInput::queue_new_independent_track_group`.
///
/// Tracks of the same kind play one after another: a newly added track starts once the previous
/// one delivered EOS (its sender was dropped and buffered data consumed).
pub(crate) struct IndependentTrackGroup {
    input: WeakQueueInput,
    /// Strong reference is owned by the queue input, upgrade fails once the group ended.
    group: Weak<Mutex<TrackGroupState>>,
}

impl IndependentTrackGroup {
    pub fn add_video_track(&self) -> Result<QueueSender<Frame>, TrackGroupEndedError> {
        let input = self.input.0.upgrade().ok_or(TrackGroupEndedError)?;
        let mut guard = input.lock().unwrap();
        // upgrade under input lock, so the group can't end before the track is pushed
        let group = self.group.upgrade().ok_or(TrackGroupEndedError)?;
        let sender = group.lock().unwrap().add_video_track(&guard);
        guard.maybe_start_pending_tracks_in_current_group();
        Ok(sender)
    }

    pub fn add_audio_track(&self) -> Result<QueueSender<InputAudioSamples>, TrackGroupEndedError> {
        let input = self.input.0.upgrade().ok_or(TrackGroupEndedError)?;
        let mut guard = input.lock().unwrap();
        // upgrade under input lock, so the group can't end before the track is pushed
        let group = self.group.upgrade().ok_or(TrackGroupEndedError)?;
        let sender = group.lock().unwrap().add_audio_track(&guard);
        guard.maybe_start_pending_tracks_in_current_group();
        Ok(sender)
    }
}

#[derive(Default, Clone)]
pub(super) struct TrackGroupOffset(Arc<Mutex<Option<Timestamp>>>);

impl TrackGroupOffset {
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
