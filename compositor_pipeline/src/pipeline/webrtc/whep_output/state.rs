use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use axum::http::HeaderMap;
use compositor_render::OutputId;

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

    pub fn add_output(&self, output_id: &OutputId, options: WhepOutputConnectionStateOptions) {
        let mut guard = self.0.lock().unwrap();
        guard.insert(output_id.clone(), WhepOutputConnectionState::new(options));
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
