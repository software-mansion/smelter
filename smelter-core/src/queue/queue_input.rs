use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, Weak},
    time::Duration,
};

use crossbeam_channel::Sender;
use smelter_render::{Frame, InputId};

use crate::{
    queue::{audio_input::AudioQueueInput, utils::PauseState, video_input::VideoQueueInput},
    types::Ref,
};

use crate::prelude::*;

pub(super) struct InnerQueueInput {
    ctx: Arc<PipelineCtx>,
    input_ref: Ref<InputId>,

    video: Option<VideoQueueInput>,
    audio: Option<AudioQueueInput>,
    track_offset: TrackOffset,
    pause_state: PauseState,

    pending: VecDeque<(
        Option<VideoQueueInput>,
        Option<AudioQueueInput>,
        TrackOffset,
    )>,
    required: bool,
}

impl InnerQueueInput {
    fn maybe_start_next_track(&mut self) {
        let video_done = self.video.as_mut().map(|v| v.is_done()).unwrap_or(true);
        let audio_done = self.audio.as_mut().map(|a| a.is_done()).unwrap_or(true);
        if video_done && audio_done {
            self.replace_track()
        }
    }

    /// Replace current track with the next pending, do nothing if there is no pending
    fn replace_track(&mut self) {
        let Some((video, audio, track_offset)) = self.pending.pop_front() else {
            return;
        };
        self.video = video;
        self.audio = audio;
        self.track_offset = track_offset;
        if self.pause_state.is_paused() {
            self.video.as_mut().map(|v| v.pause());
            self.audio.as_mut().map(|a| a.pause());
            self.pause_state.reset();
        }
    }

    fn queue_new_track(
        &mut self,
        opts: QueueTrackOptions,
    ) -> (Option<Sender<Frame>>, Option<Sender<InputAudioSamples>>) {
        if !opts.video && !opts.audio {
            return (None, None);
        }
        let (track_offset, offset_from_start) = match opts.offset {
            QueueTrackOffset::None => (TrackOffset::default(), None),
            QueueTrackOffset::Pts(duration) => (TrackOffset::from(duration), None),
            QueueTrackOffset::FromStart(duration) => (TrackOffset::default(), Some(duration)),
        };
        let (video_input, video_sender) = if opts.video {
            let (video_input, video_sender) = VideoQueueInput::new(
                &self.ctx,
                &self.input_ref,
                self.required,
                offset_from_start,
                track_offset.clone(),
            );
            (Some(video_input), Some(video_sender))
        } else {
            (None, None)
        };
        let (audio_input, audio_sender) = if opts.audio {
            let (audio_input, audio_sender) = AudioQueueInput::new(
                &self.ctx,
                &self.input_ref,
                self.required,
                offset_from_start,
                track_offset.clone(),
            );
            (Some(audio_input), Some(audio_sender))
        } else {
            (None, None)
        };
        self.pending.push_back((video_input, audio_input, track_offset));
        (video_sender, audio_sender)
    }

    /// Remember the start pts. On resume shift offset by the pts difference:
    /// - If input already started, add to track offset pts diff
    /// - If input did not started, track_offset was not initialized yet
    pub fn pause(&mut self) {
        if self.pause_state.is_paused() {
            return;
        }
        // zero before queue start
        let pts = self.ctx.queue_ctx.effective_last_pts();
        self.pause_state.pause(pts);
        self.video.as_mut().map(|v| v.pause());
        self.audio.as_mut().map(|a| a.pause());
    }

    pub fn resume(&mut self) {
        if !self.pause_state.is_paused() {
            return;
        }
        let pts = self.ctx.queue_ctx.effective_last_pts();
        if let Some(pause_time) = self.pause_state.resume(pts) {
            self.track_offset.map_add(pause_time);
        }
        self.video.as_mut().map(|v| v.resume());
        self.audio.as_mut().map(|a| a.resume());
    }
}

pub(crate) enum QueueTrackOffset {
    None,
    /// Effectively offset from sync point
    Pts(Duration),
    /// Offset from start point
    FromStart(Duration),
}

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

impl QueueInput {
    pub fn new(ctx: &Arc<PipelineCtx>, input_ref: &Ref<InputId>, required: bool) -> Self {
        Self(Arc::new(Mutex::new(InnerQueueInput {
            ctx: ctx.clone(),
            input_ref: input_ref.clone(),

            video: None,
            audio: None,
            track_offset: TrackOffset::default(),

            pending: VecDeque::new(),

            required,
            pause_state: PauseState::new(),
        })))
    }

    pub fn queue_new_track(
        &self,
        opts: QueueTrackOptions,
    ) -> (Option<Sender<Frame>>, Option<Sender<InputAudioSamples>>) {
        self.0.lock().unwrap().queue_new_track(opts)
    }

    pub fn abort_old_tracks(&self) {
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

    pub(super) fn maybe_start_next_track(&mut self) {
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
pub(super) struct TrackOffset(Arc<Mutex<Option<Duration>>>);

impl TrackOffset {
    pub fn get(&self) -> Option<Duration> {
        *self.0.lock().unwrap()
    }

    pub fn get_or_init(&self, offset: Duration) -> Duration {
        *self.0.lock().unwrap().get_or_insert(offset)
    }

    pub fn map_add(&self, duration: Duration) {
        let mut guard = self.0.lock().unwrap();
        guard.as_mut().map(|offset| *offset = *offset + duration);
    }
}

impl From<Duration> for TrackOffset {
    fn from(value: Duration) -> Self {
        Self(Arc::new(Mutex::))
    }
}
