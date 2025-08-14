use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use axum::http::HeaderMap;
use tracing::{error, warn};
use uuid::Uuid;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;

use crate::pipeline::webrtc::{
    bearer_token::validate_token,
    error::WhipWhepServerError,
    peer_connection_recvonly::RecvonlyPeerConnection,
    whip_input::connection_state::{WhipInputConnectionState, WhipInputConnectionStateOptions},
};

#[derive(Debug, Clone, Default)]
pub(crate) struct WhipInputsState(Arc<Mutex<HashMap<Arc<str>, WhipInputConnectionState>>>);

impl WhipInputsState {
    pub fn get_with<
        T,
        Func: FnOnce(&WhipInputConnectionState) -> Result<T, WhipWhepServerError>,
    >(
        &self,
        session_id: &Arc<str>,
        func: Func,
    ) -> Result<T, WhipWhepServerError> {
        let guard = self.0.lock().unwrap();
        match guard.get(session_id) {
            Some(input) => func(input),
            None => Err(WhipWhepServerError::NotFound(format!(
                "{session_id:?} not found"
            ))),
        }
    }

    pub fn get_mut_with<
        T,
        Func: FnOnce(&mut WhipInputConnectionState) -> Result<T, WhipWhepServerError>,
    >(
        &self,
        session_id: &Arc<str>,
        func: Func,
    ) -> Result<T, WhipWhepServerError> {
        let mut guard = self.0.lock().unwrap();
        match guard.get_mut(session_id) {
            Some(input) => func(input),
            None => Err(WhipWhepServerError::NotFound(format!(
                "{session_id:?} not found"
            ))),
        }
    }

    pub fn add_input(&self, session_id: Arc<str>, options: WhipInputConnectionStateOptions) {
        let mut guard = self.0.lock().unwrap();
        guard.insert(session_id, WhipInputConnectionState::new(options));
    }

    pub fn add_session(
        &self,
        input_id: &Arc<str>,
        peer_connection: Arc<RecvonlyPeerConnection>,
    ) -> Result<Arc<str>, WhipWhepServerError> {
        let mut guard = self.0.lock().unwrap();
        match guard.get_mut(input_id) {
            Some(input) => {
                let session_id: Arc<str> = Arc::from(Uuid::new_v4().to_string());
                input.sessions.insert(session_id.clone(), peer_connection);
                Ok(session_id)
            }
            None => Err(WhipWhepServerError::NotFound(format!(
                "{input_id:?} not found"
            ))),
        }
    }

    pub fn get_session(
        &self,
        input_id: &Arc<str>,
        session_id: &Arc<str>,
    ) -> Result<Arc<RecvonlyPeerConnection>, WhipWhepServerError> {
        let guard = self.0.lock().unwrap();
        match guard.get(input_id) {
            Some(input) => match input.sessions.get(session_id) {
                Some(pc) => Ok(pc.clone()),
                None => Err(WhipWhepServerError::NotFound(format!(
                    "Session {session_id:?} not found for {input_id:?}"
                ))),
            },
            None => Err(WhipWhepServerError::NotFound(format!(
                "{input_id:?} not found"
            ))),
        }
    }

    pub fn maybe_replace_peer_connection(
        &mut self,
        input_id: &Arc<str>,
        session_id: &Arc<str>,
        new_pc: Arc<RecvonlyPeerConnection>,
    ) -> Result<(), WhipWhepServerError> {
        // Deleting previous peer_connection on this input which was not in Connected state
        if let Ok(peer_connection) = &self.get_session(input_id, session_id) {
            if peer_connection.connection_state() == RTCPeerConnectionState::Connected {
                return Err(WhipWhepServerError::InternalError(format!(
                      "Another stream is currently connected to the given session_id: {input_id:?}. \
                      Disconnect the existing stream before starting a new one, or check if the session_id is correct."
                  )));
            }
            let session_id = input_id.clone();
            let pc_to_close = peer_connection.clone();

            tokio::spawn(async move {
                if let Err(err) = pc_to_close.close().await {
                    warn!("Error while closing previous peer connection {session_id:?}: {err:?}")
                }
            });
        };
        self.get_mut_with(input_id, |input| {
            if let Some(pc_slot) = input.sessions.get_mut(session_id) {
                *pc_slot = new_pc;
            }
            Ok(())
        })?;
        Ok(())
    }

    // called on drop (when input is unregistered)
    pub fn ensure_input_closed(&self, input_id: &Arc<str>, session_id: &Arc<str>) {
        let mut guard = self.0.lock().unwrap();
        if let Some(input) = guard.remove(input_id) {
            if let Some(peer_connection) = input.sessions.get(session_id).cloned() {
                let input_id = input_id.clone();
                tokio::spawn(async move {
                    if let Err(err) = peer_connection.close().await {
                        error!("Cannot close peer_connection for {input_id:?}: {err:?}");
                    };
                });
            }
        }
    }

    // TODO consider if one bearer_token per input is best approche
    pub async fn validate_token(
        &self,
        input_id: &Arc<str>,
        headers: &HeaderMap,
    ) -> Result<(), WhipWhepServerError> {
        let bearer_token = match self.0.lock().unwrap().get_mut(input_id) {
            Some(input) => input.bearer_token.clone(),
            None => {
                return Err(WhipWhepServerError::NotFound(format!(
                    "{input_id:?} not found"
                )))
            }
        };

        validate_token(&bearer_token, headers.get("Authorization")).await
    }
}
