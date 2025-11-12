use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use axum::http::HeaderMap;
use smelter_render::OutputId;
use tracing::error;
use uuid::Uuid;

use crate::pipeline::webrtc::{
    bearer_token::validate_token,
    error::WhipWhepServerError,
    whep_output::{
        connection_state::{WhepOutputConnectionState, WhepOutputConnectionStateOptions},
        peer_connection::PeerConnection,
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

    pub fn add_session(
        &self,
        output_ref: &Ref<OutputId>,
        peer_connection: PeerConnection,
    ) -> Result<Arc<str>, WhipWhepServerError> {
        let mut guard = self.0.lock().unwrap();
        match guard.get_mut(output_ref) {
            Some(output) => {
                let session_id: Arc<str> = Arc::from(Uuid::new_v4().to_string());
                output.sessions.insert(session_id.clone(), peer_connection);
                Ok(session_id)
            }
            None => Err(WhipWhepServerError::NotFound(format!(
                "Output {output_ref} not found"
            ))),
        }
    }

    pub async fn remove_session(
        &self,
        output_ref: &Ref<OutputId>,
        session_id: &Arc<str>,
    ) -> Result<(), WhipWhepServerError> {
        let peer_connection = {
            let mut guard = self.0.lock().unwrap();
            let Some(output) = guard.get_mut(output_ref) else {
                return Err(WhipWhepServerError::NotFound(format!(
                    "Output {output_ref} not found"
                )));
            };
            let Some(pc) = output.sessions.remove(session_id) else {
                return Err(WhipWhepServerError::NotFound(format!(
                    "Session {session_id:?} not found for {output_ref:?}"
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
        output_ref: &Ref<OutputId>,
        session_id: &Arc<str>,
    ) -> Result<PeerConnection, WhipWhepServerError> {
        let guard = self.0.lock().unwrap();
        match guard.get(output_ref) {
            Some(output) => match output.sessions.get(session_id) {
                Some(pc) => Ok(pc.clone()),
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

    // called on drop (when output is unregistered)
    pub fn ensure_output_closed(&self, output_ref: &Ref<OutputId>) {
        let output = {
            let mut guard = self.0.lock().unwrap();
            guard.remove(output_ref)
        };

        if let Some(output_state) = output {
            for (session_id, pc) in output_state.sessions.into_iter() {
                let output_ref = output_ref.clone();
                tokio::spawn(async move {
                    if let Err(err) = pc.close().await {
                        error!(
                            ?output_ref,
                            ?session_id,
                            ?err,
                            "Cannot close peer_connection for WHEP output"
                        );
                    }
                });
            }
        }
    }
}
