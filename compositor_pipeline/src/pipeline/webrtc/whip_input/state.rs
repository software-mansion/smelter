use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use axum::http::HeaderMap;
use tracing::error;

use crate::pipeline::webrtc::{
    bearer_token::validate_token,
    error::WhipWhepServerError,
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
        endpoint_id: &Arc<str>,
        func: Func,
    ) -> Result<T, WhipWhepServerError> {
        let guard = self.0.lock().unwrap();
        match guard.get(endpoint_id) {
            Some(input) => func(input),
            None => Err(WhipWhepServerError::NotFound(format!(
                "{endpoint_id:?} not found"
            ))),
        }
    }

    pub fn get_mut_with<
        T,
        Func: FnOnce(&mut WhipInputConnectionState) -> Result<T, WhipWhepServerError>,
    >(
        &self,
        endpoint_id: &Arc<str>,
        func: Func,
    ) -> Result<T, WhipWhepServerError> {
        let mut guard = self.0.lock().unwrap();
        match guard.get_mut(endpoint_id) {
            Some(input) => func(input),
            None => Err(WhipWhepServerError::NotFound(format!(
                "{endpoint_id:?} not found"
            ))),
        }
    }

    pub fn add_input(&self, session_id: Arc<str>, options: WhipInputConnectionStateOptions) {
        let mut guard = self.0.lock().unwrap();
        guard.insert(session_id, WhipInputConnectionState::new(options));
    }

    // called on drop (when input is unregistered)
    pub fn ensure_input_closed(&self, endpoint_id: &Arc<str>) {
        let mut guard = self.0.lock().unwrap();
        if let Some(input) = guard.remove(endpoint_id) {
            if let Some(peer_connection) = input.peer_connection {
                let endpoint_id = endpoint_id.clone();
                tokio::spawn(async move {
                    if let Err(err) = peer_connection.close().await {
                        error!("Cannot close peer_connection for {endpoint_id:?}: {err:?}");
                    };
                });
            }
        }
    }

    pub fn validate_session_id(
        &self,
        endpoint_id: &Arc<str>,
        session_id: &Arc<str>,
    ) -> Result<(), WhipWhepServerError> {
        let guard = self.0.lock().unwrap();
        if let Some(input) = guard.get(endpoint_id) {
            if input.current_session_id != Some(session_id.clone()) {
                return Err(WhipWhepServerError::Unauthorized(format!(
                    "Session {session_id} is not active now"
                )));
            }
        }
        Ok(())
    }

    pub async fn validate_token(
        &self,
        endpoint_id: &Arc<str>,
        headers: &HeaderMap,
    ) -> Result<(), WhipWhepServerError> {
        let bearer_token = match self.0.lock().unwrap().get_mut(endpoint_id) {
            Some(input) => input.bearer_token.clone(),
            None => {
                return Err(WhipWhepServerError::NotFound(format!(
                    "{endpoint_id:?} not found"
                )))
            }
        };

        validate_token(&bearer_token, headers.get("Authorization")).await
    }
}
