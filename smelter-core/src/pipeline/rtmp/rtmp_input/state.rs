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
    pub decoders: RtmpServerInputDecoders,
    pub buffer: InputBuffer,
    pub connection_handle: Option<JoinHandle<()>>,
}

pub(crate) struct RtmpInputStateOptions {
    pub app: Arc<str>,
    pub stream_key: Arc<str>,
    pub frame_sender: Sender<PipelineEvent<Frame>>,
    pub input_samples_sender: Sender<PipelineEvent<InputAudioSamples>>,
    pub decoders: RtmpServerInputDecoders,
    pub buffer: InputBuffer,
}

impl RtmpInputState {
    fn new(options: RtmpInputStateOptions) -> Self {
        Self {
            app: options.app,
            stream_key: options.stream_key,
            frame_sender: options.frame_sender,
            input_samples_sender: options.input_samples_sender,
            decoders: options.decoders,
            buffer: options.buffer,
            connection_handle: None,
        }
    }
}

impl RtmpInputsState {
    pub fn get_mut_with<T, Func: FnOnce(&mut RtmpInputState) -> Result<T, RtmpServerError>>(
        &self,
        input_ref: &Ref<InputId>,
        func: Func,
    ) -> Result<T, RtmpServerError> {
        let mut guard = self.0.lock().unwrap();
        match guard.get_mut(input_ref) {
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
            .find(|(_, input)| &input.app == app && &input.stream_key == stream_key)
            .ok_or_else(|| RtmpServerError::NotRegisteredAppStreamKeyPair {
                app: app.clone(),
                stream_key: stream_key.clone(),
            })?;
        Ok(input_ref.clone())
    }
}

impl RtmpInputState {
    pub fn ensure_no_active_connection(
        &self,
        input_ref: &Ref<InputId>,
    ) -> Result<(), RtmpServerError> {
        match &self.connection_handle {
            Some(handle) if !handle.is_finished() => Err(RtmpServerError::ConnectionAlreadyActive(
                input_ref.id().clone(),
            )),
            _ => Ok(()),
        }
    }
}
