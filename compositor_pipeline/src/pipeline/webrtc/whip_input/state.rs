use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use axum::http::HeaderMap;
use compositor_render::InputId;
use tracing::error;

use crate::pipeline::webrtc::{
    bearer_token::validate_token,
    error::WhipServerError,
    whip_input::connection_state::{WhipInputConnectionState, WhipInputConnectionStateOptions},
};

#[derive(Debug, Clone, Default)]
pub(crate) struct WhipInputsState(Arc<Mutex<HashMap<InputId, WhipInputConnectionState>>>);

impl WhipInputsState {
    pub fn get_with<T, Func: FnOnce(&WhipInputConnectionState) -> Result<T, WhipServerError>>(
        &self,
        input_id: &InputId,
        func: Func,
    ) -> Result<T, WhipServerError> {
        let guard = self.0.lock().unwrap();
        match guard.get(input_id) {
            Some(input) => func(input),
            None => Err(WhipServerError::NotFound(format!("{input_id:?} not found"))),
        }
    }

    pub fn get_mut_with<
        T,
        Func: FnOnce(&mut WhipInputConnectionState) -> Result<T, WhipServerError>,
    >(
        &self,
        input_id: &InputId,
        func: Func,
    ) -> Result<T, WhipServerError> {
        let mut guard = self.0.lock().unwrap();
        match guard.get_mut(input_id) {
            Some(input) => func(input),
            None => Err(WhipServerError::NotFound(format!("{input_id:?} not found"))),
        }
    }

    pub fn add_input(&self, input_id: &InputId, options: WhipInputConnectionStateOptions) {
        let mut guard = self.0.lock().unwrap();
        guard.insert(input_id.clone(), WhipInputConnectionState::new(options));
    }

    // called on drop (when input is unregistered)
    pub fn ensure_input_closed(&self, input_id: &InputId) {
        let mut guard = self.0.lock().unwrap();
        if let Some(input) = guard.remove(input_id) {
            if let Some(peer_connection) = input.peer_connection {
                let input_id = input_id.clone();
                tokio::spawn(async move {
                    if let Err(err) = peer_connection.close().await {
                        error!("Cannot close peer_connection for {:?}: {:?}", input_id, err);
                    };
                });
            }
        }
    }

    pub async fn validate_token(
        &self,
        input_id: &InputId,
        headers: &HeaderMap,
    ) -> Result<(), WhipServerError> {
        let bearer_token = match self.0.lock().unwrap().get_mut(input_id) {
            Some(input) => input.bearer_token.clone(),
            None => return Err(WhipServerError::NotFound(format!("{input_id:?} not found"))),
        };

        validate_token(&bearer_token, headers.get("Authorization")).await
    }
}
