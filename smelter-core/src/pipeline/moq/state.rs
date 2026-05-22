use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use tokio::task::JoinHandle;
use tracing::error;

use crate::queue::WeakQueueInput;

use crate::prelude::*;

#[derive(Debug, Clone, Default)]
pub(crate) struct MoqInputsState(Arc<Mutex<HashMap<Ref<InputId>, MoqInputState>>>);

#[derive(Debug)]
pub(crate) struct MoqInputState {
    pub broadcast_path: Arc<str>,
    pub queue_input: WeakQueueInput,
    pub decoders: MoqServerInputDecoders,
    pub connection_handle: Option<JoinHandle<()>>,
}

pub(crate) struct MoqInputStateOptions {
    pub broadcast_path: Arc<str>,
    pub queue_input: WeakQueueInput,
    pub decoders: MoqServerInputDecoders,
}

impl MoqInputState {
    fn new(options: MoqInputStateOptions) -> Self {
        Self {
            broadcast_path: options.broadcast_path,
            queue_input: options.queue_input,
            decoders: options.decoders,
            connection_handle: None,
        }
    }
}

impl MoqInputsState {
    pub fn get_mut_with<T, F: FnOnce(&mut MoqInputState) -> Result<T, MoqServerError>>(
        &self,
        input_ref: &Ref<InputId>,
        f: F,
    ) -> Result<T, MoqServerError> {
        let mut guard = self.0.lock().unwrap();
        match guard.get_mut(input_ref) {
            Some(input) => f(input),
            None => Err(MoqServerError::InputNotFound(input_ref.id().clone())),
        }
    }

    pub(crate) fn add_input(
        &self,
        input_ref: &Ref<InputId>,
        options: MoqInputStateOptions,
    ) -> Result<(), MoqServerError> {
        let mut guard = self.0.lock().unwrap();
        if guard.contains_key(input_ref) {
            return Err(MoqServerError::InputAlreadyRegistered(
                input_ref.id().clone(),
            ));
        }
        if let Some((existing_ref, _)) = guard
            .iter()
            .find(|(_, input)| input.broadcast_path == options.broadcast_path)
        {
            return Err(MoqServerError::BroadcastPathAlreadyUsed {
                broadcast_path: options.broadcast_path,
                existing_input: existing_ref.id().clone(),
            });
        }
        guard.insert(input_ref.clone(), MoqInputState::new(options));
        Ok(())
    }

    pub(crate) fn remove_input(&self, input_ref: &Ref<InputId>) {
        let mut guard = self.0.lock().unwrap();
        match guard.remove(input_ref) {
            Some(mut input) => {
                if let Some(handle) = input.connection_handle.take() {
                    handle.abort();
                }
            }
            None => {
                error!(?input_ref, "Failed to remove MoQ input, ID not found");
            }
        }
    }

    pub(crate) fn find_by_broadcast_path(
        &self,
        broadcast_path: &str,
    ) -> Result<Ref<InputId>, MoqServerError> {
        let guard = self.0.lock().unwrap();
        let (input_ref, _) = guard
            .iter()
            .find(|(_, input)| input.broadcast_path.as_ref() == broadcast_path)
            .ok_or_else(|| MoqServerError::BroadcastPathNotFound(Arc::from(broadcast_path)))?;
        Ok(input_ref.clone())
    }
}

impl MoqInputState {
    pub fn ensure_no_active_connection(
        &self,
        input_ref: &Ref<InputId>,
    ) -> Result<(), MoqServerError> {
        match &self.connection_handle {
            Some(handle) if !handle.is_finished() => Err(MoqServerError::BroadcastAlreadyActive(
                input_ref.id().clone(),
            )),
            _ => Ok(()),
        }
    }
}
