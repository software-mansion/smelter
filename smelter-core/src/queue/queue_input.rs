use std::{
    sync::{Arc, Mutex, Weak},
    time::Duration,
};

use crossbeam_channel::{SendError, Sender, bounded};
use smelter_render::{Frame, InputId};

use crate::types::Ref;

use super::{SharedState, audio_queue::AudioQueueInput, video_queue::VideoQueueInput};

use crate::prelude::*;

pub(super) struct InnerQueueInput {
    pub(super) video: Option<VideoQueueInput>,
    pub(super) audio: Option<AudioQueueInput>,
    video_sender: Option<Sender<PipelineEvent<Frame>>>,
    audio_sender: Option<Sender<PipelineEvent<InputAudioSamples>>>,
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
    pub fn new(
        has_video: bool,
        has_audio: bool,
        required: bool,
        offset: Option<Duration>,
        ctx: &Arc<PipelineCtx>,
        input_ref: &Ref<InputId>,
    ) -> Self {
        let shared_state = SharedState::default();
        let sync_point = ctx.queue_sync_point;
        let event_emitter = &ctx.event_emitter;

        let (video, video_sender) = if has_video {
            let (sender, receiver) = bounded(5);
            let input = VideoQueueInput::new(
                receiver,
                required,
                offset,
                sync_point,
                shared_state.clone(),
                input_ref.id(),
                event_emitter,
            );
            (Some(input), Some(sender))
        } else {
            (None, None)
        };

        let (audio, audio_sender) = if has_audio {
            let (sender, receiver) = bounded(5);
            let input = AudioQueueInput::new(
                receiver,
                required,
                offset,
                sync_point,
                shared_state,
                input_ref.id(),
                event_emitter,
            );
            (Some(input), Some(sender))
        } else {
            (None, None)
        };

        Self(Arc::new(Mutex::new(InnerQueueInput {
            video,
            audio,
            video_sender,
            audio_sender,
        })))
    }

    pub fn downgrade(&self) -> WeakQueueInput {
        WeakQueueInput(Arc::downgrade(&self.0))
    }

    pub fn has_video(&self) -> bool {
        self.0.lock().unwrap().video.is_some()
    }

    pub fn has_audio(&self) -> bool {
        self.0.lock().unwrap().audio.is_some()
    }
}

impl WeakQueueInput {
    pub fn video<F, R>(&self, f: F) -> Option<R>
    where
        F: FnOnce(&mut VideoQueueInput) -> R,
    {
        let arc = self.0.upgrade()?;
        let mut inner = arc.lock().unwrap();
        let video = inner.video.as_mut()?;
        Some(f(video))
    }

    pub fn audio<F, R>(&self, f: F) -> Option<R>
    where
        F: FnOnce(&mut AudioQueueInput) -> R,
    {
        let arc = self.0.upgrade()?;
        let mut inner = arc.lock().unwrap();
        let audio = inner.audio.as_mut()?;
        Some(f(audio))
    }

    pub fn send_video(&self, event: PipelineEvent<Frame>) -> Result<(), SendError<()>> {
        let arc = self.0.upgrade().ok_or(SendError(()))?;
        let sender = {
            let inner = arc.lock().unwrap();
            inner.video_sender.as_ref().ok_or(SendError(()))?.clone()
        };
        sender.send(event).map_err(|_| SendError(()))
    }

    pub fn send_audio(&self, event: PipelineEvent<InputAudioSamples>) -> Result<(), SendError<()>> {
        let arc = self.0.upgrade().ok_or(SendError(()))?;
        let sender = {
            let inner = arc.lock().unwrap();
            inner.audio_sender.as_ref().ok_or(SendError(()))?.clone()
        };
        sender.send(event).map_err(|_| SendError(()))
    }
}
