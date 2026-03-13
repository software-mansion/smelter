use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use axum::http::HeaderMap;
use smelter_render::OutputId;
use tokio::sync::broadcast;

use crate::pipeline::webrtc::{
    bearer_token::validate_token,
    error::WhipWhepServerError,
    whep_output::{
        peer_connection::{PeerConnection, WeakPeerConnection},
        track_task_audio::WhepAudioTrackThreadHandle,
        track_task_video::WhepVideoTrackThreadHandle,
    },
};

use crate::prelude::*;

#[derive(Debug, Clone, Default)]
pub(crate) struct WhepOutputsState(Arc<Mutex<HashMap<Ref<OutputId>, WhepOutputConnectionState>>>);

impl WhepOutputsState {
    pub fn get_with<
        T,
        Func: FnOnce(&WhepOutputConnectionState) -> Result<T, WhipWhepServerError>,
    >(
        &self,
        output_ref: &Ref<OutputId>,
        func: Func,
    ) -> Result<T, WhipWhepServerError> {
        let guard = self.0.lock().unwrap();
        match guard.get(output_ref) {
            Some(output) => func(output),
            None => Err(WhipWhepServerError::NotFound(format!(
                "Output {output_ref} not found"
            ))),
        }
    }

    pub fn find_by_endpoint_id(
        &self,
        endpoint_id: &Arc<str>,
    ) -> Result<Ref<OutputId>, WhipWhepServerError> {
        let guard = self.0.lock().unwrap();
        let entry = guard
            .iter()
            .find(|(output_ref, _)| &output_ref.id().0 == endpoint_id);
        match entry {
            Some((output_ref, _)) => Ok(output_ref.clone()),
            None => Err(WhipWhepServerError::NotFound(format!(
                "Output {endpoint_id} not found"
            ))),
        }
    }

    pub fn add_output(&self, output_id: &Ref<OutputId>, options: WhepOutputConnectionStateOptions) {
        let mut guard = self.0.lock().unwrap();
        guard.insert(output_id.clone(), WhepOutputConnectionState::new(options));
    }

    pub fn remove_output(&self, output_ref: &Ref<OutputId>) {
        self.0.lock().unwrap().remove(output_ref);
    }

    pub fn add_session(
        &self,
        output_ref: &Ref<OutputId>,
        session_id: &Arc<str>,
        peer_connection: PeerConnection,
    ) -> Result<(), WhipWhepServerError> {
        let mut guard = self.0.lock().unwrap();
        match guard.get_mut(output_ref) {
            Some(output) => {
                output.sessions.insert(session_id.clone(), peer_connection);
                Ok(())
            }
            None => Err(WhipWhepServerError::NotFound(format!(
                "Output {output_ref} not found"
            ))),
        }
    }

    pub fn remove_session(
        &self,
        output_ref: &Ref<OutputId>,
        session_id: &Arc<str>,
    ) -> Result<(), WhipWhepServerError> {
        let mut guard = self.0.lock().unwrap();
        let Some(output) = guard.get_mut(output_ref) else {
            return Err(WhipWhepServerError::NotFound(format!(
                "Output {output_ref} not found"
            )));
        };
        if output.sessions.remove(session_id).is_none() {
            return Err(WhipWhepServerError::NotFound(format!(
                "Session {session_id:?} not found for {output_ref:?}"
            )));
        };

        Ok(())
    }

    pub fn get_session(
        &self,
        output_ref: &Ref<OutputId>,
        session_id: &Arc<str>,
    ) -> Result<WeakPeerConnection, WhipWhepServerError> {
        let guard = self.0.lock().unwrap();
        match guard.get(output_ref) {
            Some(output) => match output.sessions.get(session_id) {
                Some(pc) => Ok(pc.downgrade()),
                None => Err(WhipWhepServerError::NotFound(format!(
                    "Session {session_id:?} not found for {output_ref:?}"
                ))),
            },
            None => Err(WhipWhepServerError::NotFound(format!(
                "Output {output_ref} not found"
            ))),
        }
    }

    pub async fn validate_token(
        &self,
        output_ref: &Ref<OutputId>,
        headers: &HeaderMap,
    ) -> Result<(), WhipWhepServerError> {
        let bearer_token = match self.0.lock().unwrap().get_mut(output_ref) {
            Some(output) => output.bearer_token.clone(),
            None => {
                return Err(WhipWhepServerError::NotFound(format!(
                    "Output {output_ref} not found"
                )));
            }
        };

        match bearer_token {
            Some(token) => validate_token(&token, headers.get("Authorization")).await,
            None => Ok(()), // Bearer token not required, treat as validated
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct WhepOutputConnectionStateOptions {
    pub bearer_token: Option<Arc<str>>,
    pub video_options: Option<WhepVideoConnectionOptions>,
    pub audio_options: Option<WhepAudioConnectionOptions>,
}

#[derive(Debug)]
pub(crate) struct WhepOutputConnectionState {
    pub bearer_token: Option<Arc<str>>,
    pub sessions: HashMap<Arc<str>, PeerConnection>,
    pub video_options: Option<WhepVideoConnectionOptions>,
    pub audio_options: Option<WhepAudioConnectionOptions>,
}

#[derive(Debug, Clone)]
pub(crate) struct WhepVideoConnectionOptions {
    pub encoder: VideoEncoderOptions,
    pub receiver: Arc<broadcast::Receiver<EncodedOutputEvent>>,
    pub track_thread_handle: WhepVideoTrackThreadHandle,
}

#[derive(Debug, Clone)]
pub(crate) struct WhepAudioConnectionOptions {
    pub encoder: AudioEncoderOptions,
    pub receiver: Arc<broadcast::Receiver<EncodedOutputEvent>>,
    pub track_thread_handle: WhepAudioTrackThreadHandle,
}

impl WhepOutputConnectionState {
    pub fn new(options: WhepOutputConnectionStateOptions) -> Self {
        WhepOutputConnectionState {
            bearer_token: options.bearer_token,
            sessions: HashMap::new(),
            video_options: options.video_options,
            audio_options: options.audio_options,
        }
    }
}
