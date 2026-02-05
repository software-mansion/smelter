use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    thread::JoinHandle,
};

use crossbeam_channel::Sender;
use tracing::error;

use crate::pipeline::utils::input_buffer::InputBuffer;

use crate::prelude::*;

#[derive(Debug, Clone, Default)]
pub(crate) struct RtmpInputsState(Arc<Mutex<HashMap<Ref<InputId>, RtmpInputState>>>);

#[derive(Debug)]
pub(crate) struct RtmpInputState {
    pub app: Arc<str>,
    pub stream_key: Arc<str>,
    pub frame_sender: Sender<PipelineEvent<Frame>>,
    pub input_samples_sender: Sender<PipelineEvent<InputAudioSamples>>,
    pub video_decoders: RtmpServerInputVideoDecoders,
    pub buffer: InputBuffer,
    pub connection_handle: Option<JoinHandle<()>>,
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
            connection_handle: None,
        }
    }
}

impl RtmpInputsState {
    pub fn get_with<T, Func: FnOnce(&RtmpInputState) -> Result<T, RtmpServerError>>(
        &self,
        input_ref: &Ref<InputId>,
        func: Func,
    ) -> Result<T, RtmpServerError> {
        let guard = self.0.lock().unwrap();
        match guard.get(input_ref) {
            Some(input) => func(input),
            None => Err(RtmpServerError::InputNotFound(input_ref.id().clone())),
        }
    }

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
        app: &Arc<str>,
        stream_key: &Arc<str>,
    ) -> Result<Ref<InputId>, RtmpServerError> {
        let guard = self.0.lock().unwrap();
        let (input_ref, _) = guard
            .iter()
            .find(|(_, input)| input.app == *app && input.stream_key == *stream_key)
            .ok_or(RtmpServerError::InvalidAppStreamKeyPair)?;
        Ok(input_ref.clone())
    }

    pub(crate) fn has_active_connection(&self, input_ref: &Ref<InputId>) -> bool {
        let mut guard = self.0.lock().unwrap();
        if let Some(input) = guard.get_mut(input_ref) {
            if let Some(handle) = &input.connection_handle
                && !handle.is_finished()
            {
                return true;
            }
            input.connection_handle = None;
        }
        false
    }

    pub(crate) fn set_connection_handle(
        &self,
        input_ref: &Ref<InputId>,
        handle: JoinHandle<()>,
    ) -> Result<(), RtmpServerError> {
        let mut guard = self.0.lock().unwrap();
        let input = guard
            .get_mut(input_ref)
            .ok_or(RtmpServerError::InputNotFound(input_ref.id().clone()))?;
        input.connection_handle = Some(handle);
        Ok(())
    }
}
