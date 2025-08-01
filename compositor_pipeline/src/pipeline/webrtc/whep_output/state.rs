use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use axum::http::HeaderMap;
use compositor_render::OutputId;
use tracing::error;

use crate::pipeline::webrtc::{
    bearer_token::validate_token,
    error::WhipWhepServerError,
    whep_output::connection_state::{WhepOutputConnectionState, WhepOutputConnectionStateOptions},
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

    pub fn get_mut_with<
        T,
        Func: FnOnce(&mut WhepOutputConnectionState) -> Result<T, WhipWhepServerError>,
    >(
        &self,
        output_id: &OutputId,
        func: Func,
    ) -> Result<T, WhipWhepServerError> {
        let mut guard = self.0.lock().unwrap();
        match guard.get_mut(output_id) {
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

    // called on drop (when input is unregistered)
    pub fn ensure_output_closed(&self, output_id: &OutputId) {
        let mut guard = self.0.lock().unwrap();
        if let Some(input) = guard.remove(output_id) {
            if let Some(peer_connection) = input.peer_connection {
                let output_id = output_id.clone();
                tokio::spawn(async move {
                    if let Err(err) = peer_connection.close().await {
                        error!(
                            "Cannot close peer_connection for {:?}: {:?}",
                            output_id, err
                        );
                    };
                });
            }
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

        validate_token(&bearer_token, headers.get("Authorization")).await
    }
}
