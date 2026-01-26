use std::{
    collections::HashMap,
    sync::{Arc, Mutex, mpsc::Receiver},
};

use rtmp::RtmpMediaData;

use crate::prelude::*;

#[derive(Debug, Clone, Default)]
pub(crate) struct RtmpInputsState(Arc<Mutex<HashMap<Ref<InputId>, RtmpInputConnectionState>>>);

#[derive(Debug)]
pub(crate) struct RtmpInputConnectionState {
    // audio/video decoder based on audioconfig/videoconfig
    pub url_path: Arc<str>,
    pub receiver: Option<Receiver<RtmpMediaData>>,
}

#[allow(unused)]
pub struct RtmpInputStateOptions {
    pub url_path: Arc<str>,
}

#[derive(Debug, thiserror::Error)]
pub enum RtmpServerError {
    #[error("Not registered URL path.")]
    NotRegisteredUrlPath,
    #[error("Input {0} is already registered.")]
    InputAlreadyRegistered(InputId),
    #[error("Input {0} is not registered.")]
    InputNotRegistered(InputId),
}

impl RtmpInputConnectionState {
    fn new(options: RtmpInputStateOptions) -> Self {
        Self {
            url_path: options.url_path,
            receiver: None,
        }
    }
}

impl RtmpInputsState {
    pub(crate) fn update(
        &self,
        url_path: Arc<str>,
        receiver: Receiver<RtmpMediaData>,
    ) -> Result<(), RtmpServerError> {
        let mut guard = self.0.lock().unwrap();
        let (_, input_state) = guard
            .iter_mut()
            .find(|(_, input)| input.url_path == url_path)
            .ok_or(RtmpServerError::NotRegisteredUrlPath)?;
        input_state.receiver = Some(receiver);
        Ok(())
    }

    #[allow(dead_code)]
    pub(crate) fn add_input(
        &self,
        input_ref: &Ref<InputId>,
        options: RtmpInputStateOptions,
    ) -> Result<(), RtmpServerError> {
        let mut guard = self.0.lock().unwrap();
        if guard.contains_key(input_ref) {
            return Err(RtmpServerError::InputAlreadyRegistered(
                input_ref.id().clone(),
            ));
        }
        guard.insert(input_ref.clone(), RtmpInputConnectionState::new(options));
        Ok(())
    }

    #[allow(dead_code)]
    pub(crate) fn remove_input(&self, input_ref: &Ref<InputId>) -> Result<(), RtmpServerError> {
        let mut guard = self.0.lock().unwrap();
        if guard.remove(input_ref).is_none() {
            return Err(RtmpServerError::InputNotRegistered(input_ref.id().clone()));
        }
        Ok(())
    }

    #[allow(dead_code)]
    pub(crate) fn take_receiver(
        &self,
        input_ref: &Ref<InputId>,
    ) -> Result<Option<Receiver<RtmpMediaData>>, RtmpServerError> {
        let mut guard = self.0.lock().unwrap();
        let input_state = guard
            .get_mut(input_ref)
            .ok_or_else(|| RtmpServerError::InputNotRegistered(input_ref.id().clone()))?;
        Ok(input_state.receiver.take())
    }
}
