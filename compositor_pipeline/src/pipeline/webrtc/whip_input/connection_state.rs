use std::sync::Arc;

use compositor_render::{Frame, InputId};
use crossbeam_channel::Sender;
use tracing::warn;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;

use crate::prelude::*;
use crate::{
    codecs::VideoDecoderOptions,
    pipeline::webrtc::{error::WhipServerError, peer_connection_recvonly::RecvonlyPeerConnection},
};

#[derive(Debug, Clone)]
pub(crate) struct WhipInputConnectionStateOptions {
    pub bearer_token: Arc<str>,
    pub video_preferences: Vec<VideoDecoderOptions>,
    pub frame_sender: Sender<PipelineEvent<Frame>>,
    pub input_samples_sender: Sender<PipelineEvent<InputAudioSamples>>,
}

#[derive(Debug, Clone)]
pub(crate) struct WhipInputConnectionState {
    pub bearer_token: Arc<str>,
    pub peer_connection: Option<RecvonlyPeerConnection>,
    pub video_preferences: Vec<VideoDecoderOptions>,
    pub frame_sender: Sender<PipelineEvent<Frame>>,
    pub input_samples_sender: Sender<PipelineEvent<InputAudioSamples>>,
}

impl WhipInputConnectionState {
    pub fn new(options: WhipInputConnectionStateOptions) -> Self {
        WhipInputConnectionState {
            bearer_token: options.bearer_token,
            peer_connection: None,
            video_preferences: options.video_preferences,
            frame_sender: options.frame_sender,
            input_samples_sender: options.input_samples_sender,
        }
    }

    pub fn maybe_replace_peer_connection(
        &mut self,
        input_id: &InputId,
        new_pc: RecvonlyPeerConnection,
    ) -> Result<(), WhipServerError> {
        // Deleting previous peer_connection on this input which was not in Connected state
        if let Some(peer_connection) = &self.peer_connection {
            if peer_connection.connection_state() == RTCPeerConnectionState::Connected {
                return Err(WhipServerError::InternalError(format!(
                      "Another stream is currently connected to the given input_id: {input_id:?}. \
                      Disconnect the existing stream before starting a new one, or check if the input_id is correct."
                  )));
            }
            if let Some(peer_connection) = self.peer_connection.take() {
                let input_id = input_id.clone();
                tokio::spawn(async move {
                    if let Err(err) = peer_connection.close().await {
                        warn!("Error while closing previous peer connection {input_id:?}: {err:?}")
                    }
                });
            }
        };
        self.peer_connection = Some(new_pc);
        Ok(())
    }
}
