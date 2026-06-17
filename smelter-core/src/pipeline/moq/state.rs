use std::{
    collections::HashMap,
    sync::{Arc, Mutex, atomic::AtomicBool},
};

use tokio::task::JoinHandle;
use tracing::error;

use crate::{pipeline::moq::server::MoqSession, queue::WeakQueueInput};

use crate::prelude::*;

#[derive(Clone, Default)]
pub(crate) struct MoqServerState(Arc<Mutex<HashMap<Ref<InputId>, MoqInputState>>>);

pub(crate) struct MoqInputState {
    pub queue_input: WeakQueueInput,
    pub decoders: MoqServerInputDecoders,
    pub should_close: Arc<AtomicBool>,
    pub connection_handle: Option<JoinHandle<()>>,
    pub session: Option<MoqSession>,
}

pub(crate) struct MoqInputStateOptions {
    pub queue_input: WeakQueueInput,
    pub decoders: MoqServerInputDecoders,
}

impl MoqInputState {
    fn new(options: MoqInputStateOptions) -> Self {
        Self {
            queue_input: options.queue_input,
            decoders: options.decoders,
            should_close: Arc::new(false.into()),
            connection_handle: None,
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

    pub(crate) fn find_by_path(&self, path: &str) -> Result<Ref<InputId>, MoqServerError> {
        let guard = self.0.lock().unwrap();
        guard
            .keys()
            .find(|input_ref| input_ref.id().0.as_ref() == path)
            .cloned()
            .ok_or_else(|| MoqServerError::BroadcastPathNotFound(Arc::from(path)))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queue::WeakQueueInput;

    fn register(state: &MoqServerState, id: &str) -> Ref<InputId> {
        let input_ref = Ref::new(&InputId(Arc::from(id)));
        state
            .add_input(
                &input_ref,
                MoqInputStateOptions {
                    queue_input: WeakQueueInput::dangling(),
                    decoders: MoqServerInputDecoders { h264: None },
                },
            )
            .unwrap();
        input_ref
    }

    #[test]
    fn find_by_path_matches_weird_ids() {
        let state = MoqServerState::default();
        let input_ref = register(&state, "my input/2");

        assert_eq!(state.find_by_path("my input/2").unwrap(), input_ref);

        assert!(matches!(
            state.find_by_path("nope"),
            Err(MoqServerError::BroadcastPathNotFound(_))
        ));
        assert!(matches!(
            state.find_by_path(""),
            Err(MoqServerError::BroadcastPathNotFound(_))
        ));
    }

    #[test]
    fn client_encoded_path_resolves() {
        let state = MoqServerState::default();
        let input_ref = register(&state, "my input/2");

        let decoded = urlencoding::decode("my%20input%2F2").unwrap();
        assert_eq!(decoded, "my input/2");
        assert_eq!(state.find_by_path(&decoded).unwrap(), input_ref);
    }
}
