use std::sync::Arc;

use compositor_render::Frame;
use crossbeam_channel::Receiver;
use tracing::warn;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;

use crate::pipeline::webrtc::error::WhipWhepServerError;
use crate::pipeline::webrtc::peer_connection_sendonly::SendonlyPeerConnection;
use crate::prelude::*;

#[derive(Debug, Clone)]
pub(crate) struct WhepOutputConnectionStateOptions {
    pub bearer_token: Arc<str>,
    pub video_encoder: Option<VideoEncoderOptions>,
    pub audio_encoder: Option<AudioEncoderOptions>,
    pub frame_receiver: Receiver<PipelineEvent<Frame>>,
    pub output_samples_receiver: Receiver<PipelineEvent<OutputAudioSamples>>,
}

#[derive(Debug, Clone)]
pub(crate) struct WhepOutputConnectionState {
    pub bearer_token: Arc<str>,
    pub peer_connection: Option<SendonlyPeerConnection>,
    pub video_encoder: Option<VideoEncoderOptions>,
    pub audio_encoder: Option<AudioEncoderOptions>,
    pub frame_receiver: Receiver<PipelineEvent<Frame>>,
    pub output_samples_receiver: Receiver<PipelineEvent<OutputAudioSamples>>,
}

impl WhepOutputConnectionState {
    pub fn new(options: WhepOutputConnectionStateOptions) -> Self {
        WhepOutputConnectionState {
            bearer_token: options.bearer_token,
            peer_connection: None,
            video_encoder: options.video_encoder,
            audio_encoder: options.audio_encoder,
            frame_receiver: options.frame_receiver,
            output_samples_receiver: options.output_samples_receiver,
        }
    }

    pub fn maybe_replace_peer_connection(
        &mut self,
        output_id: &OutputId,
        new_pc: SendonlyPeerConnection,
    ) -> Result<(), WhipWhepServerError> {
        if let Some(peer_connection) = &self.peer_connection {
            // Deleting previous peer_connection on this input which was not in Connected state
            if peer_connection.connection_state() == RTCPeerConnectionState::Connected {
                return Err(WhipWhepServerError::InternalError(format!(
                      "Another stream is currently connected to the given input_id: {output_id:?}. \
                      Disconnect the existing stream before starting a new one, or check if the input_id is correct."
                  )));
            }
            if let Some(peer_connection) = self.peer_connection.take() {
                let output_id = output_id.clone();
                tokio::spawn(async move {
                    if let Err(err) = peer_connection.close().await {
                        warn!("Error while closing previous peer connection {output_id:?}: {err:?}")
                    }
                });
            }
        };
        self.peer_connection = Some(new_pc);
        Ok(())
    }
}
