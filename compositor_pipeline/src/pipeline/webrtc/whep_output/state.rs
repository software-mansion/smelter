use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use axum::http::HeaderMap;
use compositor_render::OutputId;
use uuid::Uuid;

use crate::pipeline::webrtc::{
    bearer_token::validate_token,
    error::WhipWhepServerError,
    whep_output::{
        connection_state::{WhepOutputConnectionState, WhepOutputConnectionStateOptions},
        peer_connection::PeerConnection,
    },
};

#[derive(Debug, Clone, Default)]
pub(crate) struct WhepOutputsState(Arc<Mutex<HashMap<OutputId, WhepOutputConnectionState>>>);

impl WhepOutputsState {
    pub fn get_with<
        T,
        Func: FnOnce(&WhepOutputConnectionState) -> Result<T, WhipWhepServerError>,
    >(
        &self,
        output_id: &OutputId,
        func: Func,
    ) -> Result<T, WhipWhepServerError> {
        let guard = self.0.lock().unwrap();
        match guard.get(output_id) {
            Some(output) => func(output),
            None => Err(WhipWhepServerError::NotFound(format!(
                "{output_id:?} not found"
            ))),
        }
    }

    pub fn add_output(&self, output_id: &OutputId, options: WhepOutputConnectionStateOptions) {
        let mut guard = self.0.lock().unwrap();
        guard.insert(output_id.clone(), WhepOutputConnectionState::new(options));
    }

    pub fn add_session(
        &self,
        output_id: &OutputId,
        peer_connection: PeerConnection,
    ) -> Result<Arc<str>, WhipWhepServerError> {
        let mut guard = self.0.lock().unwrap();
        match guard.get_mut(output_id) {
            Some(output) => {
                let session_id: Arc<str> = Arc::from(Uuid::new_v4().to_string());
                output
                    .sessions
                    .insert(session_id.clone(), Arc::from(peer_connection));
                Ok(session_id)
            }
            None => Err(WhipWhepServerError::NotFound(format!(
                "{output_id:?} not found"
            ))),
        }
    }

    pub async fn remove_session(
        &self,
        output_id: &OutputId,
        session_id: &Arc<str>,
    ) -> Result<(), WhipWhepServerError> {
        let peer_connection = {
            let mut guard = self.0.lock().unwrap();
            let Some(output) = guard.get_mut(output_id) else {
                return Err(WhipWhepServerError::NotFound(format!(
                    "{output_id:?} not found"
                )));
            };
            let Some(pc) = output.sessions.remove(session_id) else {
                return Err(WhipWhepServerError::NotFound(format!(
                    "Session {session_id:?} not found for {output_id:?}"
                )));
            };
            pc
        };

        if let Err(e) = peer_connection.close().await {
            return Err(WhipWhepServerError::InternalError(format!(
                "Failed to close session {session_id:?}: {e}"
            )));
        }

        Ok(())
    }

    pub fn get_session(
        &self,
        output_id: &OutputId,
        session_id: &Arc<str>,
    ) -> Result<Arc<PeerConnection>, WhipWhepServerError> {
        let guard = self.0.lock().unwrap();
        match guard.get(output_id) {
            Some(output) => match output.sessions.get(session_id) {
                Some(pc) => Ok(pc.clone()),
                None => Err(WhipWhepServerError::NotFound(format!(
                    "Session {session_id:?} not found for {output_id:?}"
                ))),
            },
            None => Err(WhipWhepServerError::NotFound(format!(
                "{output_id:?} not found"
            ))),
        }
    }

    pub async fn validate_token(
        &self,
        output_id: &OutputId,
        headers: &HeaderMap,
    ) -> Result<(), WhipWhepServerError> {
        let bearer_token = match self.0.lock().unwrap().get_mut(output_id) {
            Some(output) => output.bearer_token.clone(),
            None => {
                return Err(WhipWhepServerError::NotFound(format!(
                    "{output_id:?} not found"
                )))
            }
        };

        match bearer_token {
            Some(token) => validate_token(&token, headers.get("Authorization")).await,
            None => Ok(()), // Bearer token not required, treat as validated
        }
    }
}
