use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use crossbeam_channel::Sender;
use tracing::error;

use crate::{
    pipeline::{decoder::DecoderThreadHandle, utils::input_buffer::InputBuffer},
    prelude::*,
};

#[derive(Debug, Clone, Default)]
pub(crate) struct RtmpInputsState(Arc<Mutex<HashMap<Ref<InputId>, RtmpInputState>>>);

#[derive(Debug, Clone)]
pub(crate) struct RtmpInputState {
    // audio/video decoder based on audioconfig/videoconfig
    pub app: Arc<str>,
    pub stream_key: Arc<str>,
    pub frame_sender: Sender<PipelineEvent<Frame>>,
    pub input_samples_sender: Sender<PipelineEvent<InputAudioSamples>>,
    pub video_decoders: RtmpServerInputVideoDecoders,
    pub buffer: InputBuffer,
    pub video_decoder_handle: Option<DecoderThreadHandle>,
    pub audio_decoder_handle: Option<DecoderThreadHandle>,
}

pub(crate) struct RtmpInputStateOptions {
    pub app: Arc<str>,
    pub stream_key: Arc<str>,
    pub frame_sender: Sender<PipelineEvent<Frame>>,
    pub input_samples_sender: Sender<PipelineEvent<InputAudioSamples>>,
    pub video_decoders: RtmpServerInputVideoDecoders,
    pub buffer: InputBuffer,
}

impl RtmpInputState {
    fn new(options: RtmpInputStateOptions) -> Self {
        Self {
            app: options.app,
            stream_key: options.stream_key,
            frame_sender: options.frame_sender,
            input_samples_sender: options.input_samples_sender,
            video_decoders: options.video_decoders,
            buffer: options.buffer,
            video_decoder_handle: None,
            audio_decoder_handle: None,
        }
    }
}

impl RtmpInputsState {
    pub(crate) fn add_input(
        &self,
        input_ref: &Ref<InputId>,
        options: RtmpInputStateOptions,
    ) -> Result<(), RtmpServerError> {
        let mut guard = self.0.lock().unwrap();
        if guard.contains_key(input_ref) {
            return Err(RtmpServerError::InputAlreadyRegistered(
                input_ref.id().clone(),
            ));
        }
        guard.insert(input_ref.clone(), RtmpInputState::new(options));
        Ok(())
    }

    pub(crate) fn remove_input(&self, input_ref: &Ref<InputId>) {
        let mut guard = self.0.lock().unwrap();
        if guard.remove(input_ref).is_none() {
            error!(?input_ref, "Failed to remove input, ID not found");
        }
    }

    pub(crate) fn find_by_app_stream_key(
        &self,
        app: Arc<str>,
        stream_key: Arc<str>,
    ) -> Result<(Ref<InputId>, RtmpInputState), RtmpServerError> {
        let guard = self.0.lock().unwrap();
        let (input_ref, input_state) = guard
            .iter()
            .find(|(_, input)| input.app == app && input.stream_key == stream_key)
            .ok_or(RtmpServerError::InvalidAppStreamKeyPair)?;
        Ok((input_ref.clone(), input_state.clone()))
    }

    pub(crate) fn get(&self, input_ref: &Ref<InputId>) -> Result<RtmpInputState, RtmpServerError> {
        let guard = self.0.lock().unwrap();
        let input_state = guard
            .get(input_ref)
            .ok_or(RtmpServerError::InputNotRegistered(input_ref.id().clone()))?;
        Ok(input_state.clone())
    }

    pub(crate) fn set_video_decoder_handle(
        &self,
        input_ref: &Ref<InputId>,
        handle: DecoderThreadHandle,
    ) -> Result<(), RtmpServerError> {
        let mut guard = self.0.lock().unwrap();
        let state = guard
            .get_mut(input_ref)
            .ok_or(RtmpServerError::InputNotRegistered(input_ref.id().clone()))?;
        state.video_decoder_handle = Some(handle);
        Ok(())
    }

    pub(crate) fn video_chunk_sender(
        &self,
        input_ref: &Ref<InputId>,
    ) -> Result<Option<Sender<PipelineEvent<EncodedInputChunk>>>, RtmpServerError> {
        let guard = self.0.lock().unwrap();
        let state = guard
            .get(input_ref)
            .ok_or(RtmpServerError::InputNotRegistered(input_ref.id().clone()))?;
        Ok(state
            .video_decoder_handle
            .as_ref()
            .map(|handle| handle.chunk_sender.clone()))
    }

    pub(crate) fn set_audio_decoder_handle(
        &self,
        input_ref: &Ref<InputId>,
        handle: DecoderThreadHandle,
    ) -> Result<(), RtmpServerError> {
        let mut guard = self.0.lock().unwrap();
        let state = guard
            .get_mut(input_ref)
            .ok_or(RtmpServerError::InputNotRegistered(input_ref.id().clone()))?;
        state.audio_decoder_handle = Some(handle);
        Ok(())
    }

    pub(crate) fn audio_chunk_sender(
        &self,
        input_ref: &Ref<InputId>,
    ) -> Result<Option<Sender<PipelineEvent<EncodedInputChunk>>>, RtmpServerError> {
        let guard = self.0.lock().unwrap();
        let state = guard
            .get(input_ref)
            .ok_or(RtmpServerError::InputNotRegistered(input_ref.id().clone()))?;
        Ok(state
            .audio_decoder_handle
            .as_ref()
            .map(|handle| handle.chunk_sender.clone()))
    }
}
