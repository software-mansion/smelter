use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, Weak},
    time::Duration,
};

use crossbeam_channel::Sender;
use smelter_render::{Frame, InputId};

use crate::{queue::video_input::VideoQueueInput, types::Ref};

use super::{SharedState, audio_queue::AudioQueueInput};

use crate::prelude::*;

pub(super) struct InnerQueueInput {
    pub(super) video: Option<VideoQueueInput>,
    pub(super) audio: Option<AudioQueueInput>,
    pending: VecDeque<(Option<VideoQueueInput>, Option<VideoQueueInput>)>,
    required: bool,
    ctx: Arc<PipelineCtx>,
    input_ref: Ref<InputId>,
}

impl InnerQueueInput {
    fn maybe_start_next_track(&mut self) {
        let video_done = self.video.map(|v| v.is_done()).unwrap_or(true);
        let audio_done = self.audio.map(|a| a.is_done()).unwrap_or(true);
        if video_done && audio_done {
            self.replace_track()
        }
    }

    fn replace_track(&mut self) {
        let Some((video, audio)) = self.pending.pop_front() else {
            return;
        };
        self.video = video;
        self.audio = audio;
    }

    fn queue_new_track(
        &mut self,
        video: Option<VideoTrackOptions>,
        audio: Option<AudioTrackOptions>,
    ) -> (Option<Sender<Frame>>, Option<Sender<Frame>>) {
        if video.is_none() && audio.is_none() {
            return (None, None);
        }
        let state = SharedState::default();
        let (video_input, video_sender) = match video {
            Some(video) => {
                let (video_input, video_sender) = VideoQueueInput::new(
                    &self.ctx,
                    &self.input_ref,
                    self.required,
                    video.offset,
                    state.clone(),
                );
                (Some(video_input), Some(video_sender))
            }
            None => (None, None),
        };
        let (audio_input, audio_sender) = match audio {
            Some(audio) => {
                let (audio_input, audio_sender) = VideoQueueInput::new(
                    &self.ctx,
                    &self.input_ref,
                    self.required,
                    audio.offset,
                    state.clone(),
                );
                (Some(audio_input), Some(audio_sender))
            }
            None => (None, None),
        };
        self.pending.push_back((video_input, audio_input));
        (video_sender, audio_sender)
    }
}

pub(crate) struct VideoTrackOptions {
    offset: Option<Duration>,
}
pub(crate) struct AudioTrackOptions {
    offset: Option<Duration>,
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
            required,
            video: None,
            audio: None,
            pending: VecDeque::new(),
        })))
    }

    pub fn queue_new_track(
        &mut self,
        video: Option<VideoTrackOptions>,
        audio: Option<AudioTrackOptions>,
    ) -> (Option<Sender<Frame>>, Option<Sender<Frame>>) {
        self.0.lock().unwrap().queue_new_track(video, audio)
    }

    pub fn pause(&self) {
        let mut guard = self.0.lock().unwrap();
        let pts = guard.ctx.queue_sync_point.elapsed();
        guard.video.as_mut().map(|v| v.pause(pts));
        guard.audio.as_mut().map(|a| a.pause(pts));
    }

    pub fn resume(&self) {
        let mut guard = self.0.lock().unwrap();
        let pts = guard.ctx.queue_sync_point.elapsed();
        guard.video.as_mut().map(|v| v.resume(pts));
        guard.audio.as_mut().map(|a| a.resume(pts));
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
    pub fn get_or_init(&self, offset: Duration) -> Duration {
        *self.0.lock().unwrap().get_or_insert(offset)
    }
}
