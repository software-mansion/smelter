use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use axum::http::HeaderMap;
use tracing::error;

use crate::pipeline::webrtc::{
    bearer_token::validate_token,
    error::WhipServerError,
    whip_input::connection_state::{WhipInputConnectionState, WhipInputConnectionStateOptions},
};

#[derive(Debug, Clone, Default)]
pub(crate) struct WhipInputsState(Arc<Mutex<HashMap<Arc<str>, WhipInputConnectionState>>>);

impl WhipInputsState {
    pub fn get_with<T, Func: FnOnce(&WhipInputConnectionState) -> Result<T, WhipServerError>>(
        &self,
        session_id: &Arc<str>,
        func: Func,
    ) -> Result<T, WhipServerError> {
        let guard = self.0.lock().unwrap();
        match guard.get(session_id) {
            Some(input) => func(input),
            None => Err(WhipServerError::NotFound(format!(
                "{session_id:?} not found"
            ))),
        }
    }

    pub fn get_mut_with<
        T,
        Func: FnOnce(&mut WhipInputConnectionState) -> Result<T, WhipServerError>,
    >(
        &self,
        session_id: &Arc<str>,
        func: Func,
    ) -> Result<T, WhipServerError> {
        let mut guard = self.0.lock().unwrap();
        match guard.get_mut(session_id) {
            Some(input) => func(input),
            None => Err(WhipServerError::NotFound(format!(
                "{session_id:?} not found"
            ))),
        }
    }

    pub fn add_input(&self, session_id: Arc<str>, options: WhipInputConnectionStateOptions) {
        let mut guard = self.0.lock().unwrap();
        guard.insert(session_id, WhipInputConnectionState::new(options));
    }

    // called on drop (when input is unregistered)
    pub fn ensure_input_closed(&self, session_id: &Arc<str>) {
        let mut guard = self.0.lock().unwrap();
        if let Some(input) = guard.remove(session_id) {
            if let Some(peer_connection) = input.peer_connection {
                let session_id = session_id.clone();
                tokio::spawn(async move {
                    if let Err(err) = peer_connection.close().await {
                        error!("Cannot close peer_connection for {session_id:?}: {err:?}");
                    };
                });
            }
        }
    }

    pub async fn validate_token(
        &self,
        session_id: &Arc<str>,
        headers: &HeaderMap,
    ) -> Result<(), WhipServerError> {
        let bearer_token = match self.0.lock().unwrap().get_mut(session_id) {
            Some(input) => input.bearer_token.clone(),
            None => {
                return Err(WhipServerError::NotFound(format!(
                    "{session_id:?} not found"
                )))
            }
        };

        validate_token(&bearer_token, headers.get("Authorization")).await
    }
}
