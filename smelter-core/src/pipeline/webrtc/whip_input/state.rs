use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use axum::http::HeaderMap;
use crossbeam_channel::Sender;
use smelter_render::Frame;
use tracing::warn;
use tracing::{Instrument, error};
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;

use crate::{
    codecs::VideoDecoderOptions,
    pipeline::webrtc::{
        bearer_token::validate_token, error::WhipWhepServerError,
        peer_connection_recvonly::RecvonlyPeerConnection,
    },
    prelude::*,
};

#[derive(Debug, Clone, Default)]
pub(crate) struct WhipInputsState(Arc<Mutex<HashMap<InputId, WhipInputState>>>);

impl WhipInputsState {
    pub fn get_with<T, Func: FnOnce(&WhipInputState) -> Result<T, WhipWhepServerError>>(
        &self,
        input_id: &InputId,
        func: Func,
    ) -> Result<T, WhipWhepServerError> {
        let guard = self.0.lock().unwrap();
        match guard.get(input_id) {
            Some(input) => func(input),
            None => Err(WhipWhepServerError::NotFound(format!(
                "{input_id:?} not found"
            ))),
        }
    }

    pub fn get_mut_with<T, Func: FnOnce(&mut WhipInputState) -> Result<T, WhipWhepServerError>>(
        &self,
        input_id: &InputId,
        func: Func,
    ) -> Result<T, WhipWhepServerError> {
        let mut guard = self.0.lock().unwrap();
        match guard.get_mut(input_id) {
            Some(input) => func(input),
            None => Err(WhipWhepServerError::NotFound(format!(
                "{input_id:?} not found"
            ))),
        }
    }

    pub fn find_by_endpoint_id(
        &self,
        endpoint_id: &Arc<str>,
    ) -> Result<InputId, WhipWhepServerError> {
        let guard = self.0.lock().unwrap();
        let entry = guard
            .iter()
            .find(|(_, input)| input.endpoint_id == *endpoint_id);
        match entry {
            Some((input_id, _)) => Ok(input_id.clone()),
            None => Err(WhipWhepServerError::NotFound(format!(
                "{endpoint_id:?} not found"
            ))),
        }
    }

    pub fn add_input(
        &self,
        input_id: &InputId,
        options: WhipInputStateOptions,
    ) -> Result<(), WebrtcServerError> {
        let mut guard = self.0.lock().unwrap();
        let is_endpoint_id_in_use = guard
            .iter()
            .any(|(_, input)| input.endpoint_id == options.endpoint_id);
        if is_endpoint_id_in_use {
            return Err(WebrtcServerError::EndpointIdAlreadyInUse(
                options.endpoint_id,
            ));
        }
        let old_value = guard.insert(input_id.clone(), WhipInputState::new(options));
        if old_value.is_some() {
            error!(
                ?input_id,
                "Old WHIP input entry was overriden. This should not happen"
            )
        }
        Ok(())
    }

    // called on drop (when input is unregistered)
    pub fn ensure_input_closed(&self, input_id: &InputId) {
        let mut guard = self.0.lock().unwrap();
        let Some(session) = guard.remove(input_id).and_then(|input| input.session) else {
            return;
        };

        let input_id = input_id.clone();
        tokio::spawn(async move {
            if let Err(err) = session.peer_connection.close().await {
                error!("Cannot close peer_connection for {input_id:?}: {err:?}");
            };
        });
    }

    pub fn validate_session_id(
        &self,
        input_id: &InputId,
        session_id: &Arc<str>,
    ) -> Result<(), WhipWhepServerError> {
        let guard = self.0.lock().unwrap();
        let id_from_state = guard
            .get(input_id)
            .and_then(|input| input.session.as_ref())
            .map(|session| session.session_id.clone());

        match id_from_state {
            Some(id) if &id == session_id => Ok(()),
            _ => Err(WhipWhepServerError::Unauthorized(format!(
                "Session {session_id} is not active now"
            ))),
        }
    }

    pub async fn validate_token(
        &self,
        input_id: &InputId,
        headers: &HeaderMap,
    ) -> Result<(), WhipWhepServerError> {
        let bearer_token = match self.0.lock().unwrap().get_mut(input_id) {
            Some(input) => input.bearer_token.clone(),
            None => {
                return Err(WhipWhepServerError::NotFound(format!(
                    "{input_id:?} not found"
                )));
            }
        };

        validate_token(&bearer_token, headers.get("Authorization")).await
    }
}

#[derive(Debug, Clone)]
pub(crate) struct WhipInputStateOptions {
    pub bearer_token: Arc<str>,
    pub endpoint_id: Arc<str>,
    pub video_preferences: Vec<VideoDecoderOptions>,
    pub frame_sender: Sender<PipelineEvent<Frame>>,
    pub input_samples_sender: Sender<PipelineEvent<InputAudioSamples>>,
    pub jitter_buffer_options: RtpJitterBufferOptions,
}

#[derive(Debug, Clone)]
pub(crate) struct WhipInputState {
    pub bearer_token: Arc<str>,
    pub endpoint_id: Arc<str>,
    pub video_preferences: Vec<VideoDecoderOptions>,
    pub frame_sender: Sender<PipelineEvent<Frame>>,
    pub input_samples_sender: Sender<PipelineEvent<InputAudioSamples>>,
    pub session: Option<WhipInputSession>,
    pub jitter_buffer_options: RtpJitterBufferOptions,
}

#[derive(Debug, Clone)]
pub(crate) struct WhipInputSession {
    pub peer_connection: RecvonlyPeerConnection,
    pub session_id: Arc<str>,
}

impl WhipInputState {
    pub fn new(options: WhipInputStateOptions) -> Self {
        WhipInputState {
            bearer_token: options.bearer_token,
            endpoint_id: options.endpoint_id,
            video_preferences: options.video_preferences,
            frame_sender: options.frame_sender,
            input_samples_sender: options.input_samples_sender,
            session: None,
            jitter_buffer_options: options.jitter_buffer_options,
        }
    }

    pub fn maybe_replace_session(
        &mut self,
        session: WhipInputSession,
    ) -> Result<(), WhipWhepServerError> {
        // Deleting previous peer_connection on this input which was not in Connected state
        if let Some(session) = &self.session {
            if session.peer_connection.connection_state() == RTCPeerConnectionState::Connected {
                return Err(WhipWhepServerError::InternalError("Another stream is currently connected to this endpoint \
                      Disconnect the existing stream before starting a new one, or check if the session_id is correct.".to_string()));
            }
            if let Some(session) = self.session.take() {
                tokio::spawn(
                    async move {
                        let session_id = session.session_id;
                        if let Err(err) = session.peer_connection.close().await {
                            warn!("Error while closing {session_id} peer connection: {err:?}")
                        }
                    }
                    .instrument(tracing::Span::current()),
                );
            }
        };
        self.session = Some(session);
        Ok(())
    }
}
