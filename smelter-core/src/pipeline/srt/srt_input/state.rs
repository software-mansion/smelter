use std::{sync::Arc, thread::JoinHandle, time::Duration};

use crate::QueueInputOptions;
use crate::queue::WeakQueueInput;

use crate::prelude::*;

#[derive(Debug)]
pub(crate) struct SrtInputState {
    pub stream_id: Arc<str>,
    pub queue_input: WeakQueueInput,
    pub video: Option<SrtInputVideoOptions>,
    pub audio: Option<SrtInputAudioOptions>,
    #[allow(dead_code)]
    pub queue_options: QueueInputOptions,
    pub offset: Option<Duration>,
    pub encryption: Option<SrtInputEncryption>,
    pub first_connection: bool,
    pub connection_handle: Option<JoinHandle<()>>,
}

pub(crate) struct SrtInputStateOptions {
    pub stream_id: Arc<str>,
    pub queue_input: WeakQueueInput,
    pub video: Option<SrtInputVideoOptions>,
    pub audio: Option<SrtInputAudioOptions>,
    pub queue_options: QueueInputOptions,
    pub offset: Option<Duration>,
    pub encryption: Option<SrtInputEncryption>,
}

impl SrtInputState {
    pub fn new(options: SrtInputStateOptions) -> Self {
        Self {
            stream_id: options.stream_id,
            queue_input: options.queue_input,
            video: options.video,
            audio: options.audio,
            queue_options: options.queue_options,
            offset: options.offset,
            encryption: options.encryption,
            first_connection: true,
            connection_handle: None,
        }
    }

    pub fn ensure_no_active_connection(
        &self,
        input_ref: &Ref<InputId>,
    ) -> Result<(), SrtServerError> {
        match &self.connection_handle {
            Some(handle) if !handle.is_finished() => Err(SrtServerError::ConnectionAlreadyActive(
                input_ref.id().clone(),
            )),
            _ => Ok(()),
        }
    }
}
