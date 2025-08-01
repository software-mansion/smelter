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
pub(crate) struct WhipInputsState(Arc<Mutex<WhipInputsStateInner>>);

#[derive(Debug, Clone, Default)]
struct WhipInputsStateInner {
    prefixes: HashMap<InputId, Arc<str>>,
    inputs: HashMap<InputId, WhipInputConnectionState>,
}

impl WhipInputsStateInner {
    pub fn get(&self, input_id: InputId) -> Option<&WhipInputConnectionState> {
        let input_id = self.prefixed_input_id(input_id);
        self.inputs.get(&input_id)
    }

    pub fn get_mut(&mut self, input_id: InputId) -> Option<&mut WhipInputConnectionState> {
        let input_id = self.prefixed_input_id(input_id);
        self.inputs.get_mut(&input_id)
    }

    pub fn insert(
        &mut self,
        input_id: InputId,
        input_id_prefix: Option<Arc<str>>,
        state: WhipInputConnectionState,
    ) {
        if let Some(prefix) = input_id_prefix {
            self.prefixes.insert(input_id.clone(), prefix);
        }

        self.inputs.insert(input_id, state);
    }

    pub fn remove(&mut self, input_id: InputId) -> Option<WhipInputConnectionState> {
        let old_input_id = input_id.clone();
        let input_id = self.prefixed_input_id(input_id);
        self.prefixes.remove(&old_input_id);
        self.inputs.remove(&input_id)
    }

    fn prefixed_input_id(&self, input_id: InputId) -> InputId {
        match self.prefixes.get(&input_id) {
            Some(prefix) => InputId([prefix.clone(), input_id.0].concat().into()),
            None => input_id,
        }
    }
}

impl WhipInputsState {
    pub fn get_with<T, Func: FnOnce(&WhipInputConnectionState) -> Result<T, WhipServerError>>(
        &self,
        input_id: &InputId,
        func: Func,
    ) -> Result<T, WhipServerError> {
        let guard = self.0.lock().unwrap();
        match guard.get(input_id.clone()) {
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
        match guard.get_mut(input_id.clone()) {
            Some(input) => func(input),
            None => Err(WhipServerError::NotFound(format!("{input_id:?} not found"))),
        }
    }

    pub fn add_input(
        &self,
        input_id: &InputId,
        input_id_prefix: Option<Arc<str>>,
        options: WhipInputConnectionStateOptions,
    ) {
        let mut guard = self.0.lock().unwrap();
        guard.insert(
            input_id.clone(),
            input_id_prefix,
            WhipInputConnectionState::new(options),
        );
    }

    // called on drop (when input is unregistered)
    pub fn ensure_input_closed(&self, input_id: &InputId) {
        let mut guard = self.0.lock().unwrap();
        if let Some(input) = guard.remove(input_id.clone()) {
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
        let bearer_token = match self.0.lock().unwrap().get_mut(input_id.clone()) {
            Some(input) => input.bearer_token.clone(),
            None => return Err(WhipServerError::NotFound(format!("{input_id:?} not found"))),
        };

        validate_token(&bearer_token, headers.get("Authorization")).await
    }
}
