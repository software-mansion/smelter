use std::{
    collections::HashMap,
    sync::{Arc, Mutex, Weak},
};

use crossbeam_channel::Sender;

use crate::prelude::*;

#[derive(Debug, Clone)]
pub(crate) struct RtmpInputsState {
    connections: Arc<Mutex<HashMap<Ref<InputId>, RtmpInputConnectionState>>>,
    ctx: Arc<Mutex<Option<Weak<PipelineCtx>>>>,
}

impl Default for RtmpInputsState {
    fn default() -> Self {
        Self {
            connections: Arc::new(Mutex::new(HashMap::new())),
            ctx: Arc::new(Mutex::new(None)),
        }
    }
}

#[derive(Debug)]
pub(crate) struct RtmpInputConnectionState {
    // audio/video decoder based on audioconfig/videoconfig
    pub app: Arc<str>,
    pub stream_key: Arc<str>,
    pub frame_sender: Sender<PipelineEvent<Frame>>,
    pub input_samples_sender: Sender<PipelineEvent<InputAudioSamples>>,
    pub video_decoders: RtmpServerInputVideoDecoders,
}

pub(crate) struct RtmpInputConnectionInfo {
    pub input_ref: Ref<InputId>,
    pub frame_sender: Sender<PipelineEvent<Frame>>,
    pub input_samples_sender: Sender<PipelineEvent<InputAudioSamples>>,
    pub video_decoders: RtmpServerInputVideoDecoders,
    pub ctx: Arc<PipelineCtx>,
}

pub struct RtmpInputStateOptions {
    pub app: Arc<str>,
    pub stream_key: Arc<str>,
    pub frame_sender: Sender<PipelineEvent<Frame>>,
    pub input_samples_sender: Sender<PipelineEvent<InputAudioSamples>>,
    pub video_decoders: RtmpServerInputVideoDecoders,
    pub ctx: Weak<PipelineCtx>,
}

impl RtmpInputConnectionState {
    fn new(options: RtmpInputStateOptions) -> Self {
        Self {
            app: options.app,
            stream_key: options.stream_key,
            frame_sender: options.frame_sender,
            input_samples_sender: options.input_samples_sender,
            video_decoders: options.video_decoders,
        }
    }
}

impl RtmpInputsState {
    pub(crate) fn set_ctx(&self, ctx: Weak<PipelineCtx>) {
        *self.ctx.lock().unwrap() = Some(ctx);
    }

    pub(crate) fn ctx(&self) -> Option<Arc<PipelineCtx>> {
        self.ctx
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|ctx| ctx.upgrade())
    }

    pub(crate) fn update(
        &self,
        app: Arc<str>,
        stream_key: Arc<str>,
    ) -> Result<RtmpInputConnectionInfo, RtmpServerError> {
        let guard = self.connections.lock().unwrap();
        let (input_ref, input_state) = guard
            .iter()
            .find(|(_, input)| input.app == app && input.stream_key == stream_key)
            .ok_or(RtmpServerError::InvalidAppStreamKeyPair)?;
        let ctx = self
            .ctx()
            .ok_or(RtmpServerError::PipelineContextUnavailable)?;
        Ok(RtmpInputConnectionInfo {
            input_ref: input_ref.clone(),
            frame_sender: input_state.frame_sender.clone(),
            input_samples_sender: input_state.input_samples_sender.clone(),
            video_decoders: input_state.video_decoders.clone(),
            ctx,
        })
    }

    pub(crate) fn add_input(
        &self,
        input_ref: &Ref<InputId>,
        options: RtmpInputStateOptions,
    ) -> Result<(), RtmpServerError> {
        let mut guard = self.connections.lock().unwrap();
        if guard.contains_key(input_ref) {
            return Err(RtmpServerError::InputAlreadyRegistered(
                input_ref.id().clone(),
            ));
        }
        guard.insert(input_ref.clone(), RtmpInputConnectionState::new(options));
        Ok(())
    }

    #[allow(dead_code)]
    pub(crate) fn remove_input(&self, input_ref: &Ref<InputId>) -> Result<(), RtmpServerError> {
        let mut guard = self.connections.lock().unwrap();
        if guard.remove(input_ref).is_none() {
            return Err(RtmpServerError::InputNotRegistered(input_ref.id().clone()));
        }
        Ok(())
    }

    // #[allow(dead_code)]
    // pub(crate) fn take_receiver(
    //     &self,
    //     input_ref: &Ref<InputId>,
    // ) -> Result<Option<Receiver<RtmpMediaData>>, RtmpServerError> {
    //     let mut guard = self.0.lock().unwrap();
    //     let input_state = guard
    //         .get_mut(input_ref)
    //         .ok_or_else(|| RtmpServerError::InputNotRegistered(input_ref.id().clone()))?;
    //     Ok(input_state.receiver.take())
    // }
}
