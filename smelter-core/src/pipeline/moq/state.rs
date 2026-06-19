use std::{
    collections::HashMap,
    sync::{Arc, Mutex, atomic::AtomicBool},
};

use tokio::task::JoinHandle;
use tracing::error;

use crate::{
    pipeline::moq::server::MoqSession, queue::WeakQueueInput, utils::authentication::validate_token,
};

use crate::prelude::*;

#[derive(Clone, Default)]
pub(crate) struct MoqServerState(Arc<Mutex<HashMap<Ref<InputId>, MoqInputState>>>);

pub(crate) struct MoqInputState {
    pub queue_input: WeakQueueInput,
    pub auth_token: Arc<str>,
    pub decoders: MoqServerInputDecoders,
    pub should_close: Arc<AtomicBool>,
    pub connection_task_handle: Option<JoinHandle<()>>,
    pub session: Option<MoqSession>,
}

pub(crate) struct MoqInputStateOptions {
    pub queue_input: WeakQueueInput,
    pub auth_token: Arc<str>,
    pub decoders: MoqServerInputDecoders,
}

impl MoqInputState {
    fn new(options: MoqInputStateOptions) -> Self {
        Self {
            queue_input: options.queue_input,
            auth_token: options.auth_token,
            decoders: options.decoders,
            should_close: Arc::new(false.into()),
            connection_task_handle: None,
            session: None,
        }
    }
}

impl MoqServerState {
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
        guard.insert(input_ref.clone(), MoqInputState::new(options));
        Ok(())
    }

    pub(crate) fn remove_input(&self, input_ref: &Ref<InputId>) {
        let mut guard = self.0.lock().unwrap();
        match guard.remove(input_ref) {
            Some(input) => {
                input
                    .should_close
                    .store(true, std::sync::atomic::Ordering::Relaxed);
            }
            None => {
                error!(?input_ref, "Failed to remove MoQ input, ID not found");
            }
        }
    }

    pub(crate) fn find_by_url(&self, path: &str) -> Result<Ref<InputId>, MoqServerError> {
        let guard = self.0.lock().unwrap();
        guard
            .keys()
            .find(|input_ref| input_ref.id().0.as_ref() == path)
            .cloned()
            .ok_or_else(|| MoqServerError::PathNotFound(Arc::from(path)))
    }

    pub(super) fn validate_auth_token(
        &self,
        input_ref: &Ref<InputId>,
        provided_token: &str,
    ) -> Result<(), MoqServerError> {
        let expected_token = {
            let guard = self.0.lock().unwrap();
            let input = guard
                .get(input_ref)
                .ok_or(MoqServerError::InputNotFound(input_ref.id().clone()))?;
            input.auth_token.clone()
        };

        match validate_token(&expected_token, provided_token) {
            true => Ok(()),
            false => Err(MoqServerError::InvalidToken(input_ref.id().clone())),
        }
    }
}

impl MoqInputState {
    pub fn ensure_no_active_connection(
        &self,
        input_ref: &Ref<InputId>,
    ) -> Result<(), MoqServerError> {
        match &self.connection_task_handle {
            Some(handle) if !handle.is_finished() => Err(MoqServerError::BroadcastAlreadyActive(
                input_ref.id().clone(),
            )),
            _ => Ok(()),
        }
    }
}
