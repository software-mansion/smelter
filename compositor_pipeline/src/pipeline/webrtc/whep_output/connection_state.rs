use std::sync::Arc;
use tokio::sync::broadcast;

use crate::pipeline::rtp::RtpPacket;
use crate::pipeline::webrtc::peer_connection_sendonly::SendonlyPeerConnection;
use crate::prelude::*;

#[derive(Debug, Clone)]
pub(crate) struct WhepOutputConnectionStateOptions {
    pub bearer_token: Arc<str>,
    pub video_encoder: Option<VideoEncoderOptions>,
    pub audio_encoder: Option<AudioEncoderOptions>,
    pub video_receiver: Option<Arc<broadcast::Receiver<RtpPacket>>>,
    pub audio_receiver: Option<Arc<broadcast::Receiver<RtpPacket>>>,
}

#[derive(Debug, Clone)]
pub(crate) struct WhepOutputConnectionState {
    pub bearer_token: Arc<str>,
    pub peer_connection: Option<SendonlyPeerConnection>,  //TODO maybe not necessary
    pub video_encoder: Option<VideoEncoderOptions>,
    pub audio_encoder: Option<AudioEncoderOptions>,
    pub video_receiver: Option<Arc<broadcast::Receiver<RtpPacket>>>,
    pub audio_receiver: Option<Arc<broadcast::Receiver<RtpPacket>>>,
}

impl WhepOutputConnectionState {
    pub fn new(options: WhepOutputConnectionStateOptions) -> Self {
        WhepOutputConnectionState {
            bearer_token: options.bearer_token,
            peer_connection: None,
            video_encoder: options.video_encoder,
            audio_encoder: options.audio_encoder,
            video_receiver: options.video_receiver,
            audio_receiver: options.audio_receiver,
        }
    }

    // pub fn maybe_replace_peer_connection(
    //     &mut self,
    //     output_id: &OutputId,
    //     new_pc: SendonlyPeerConnection,
    // ) -> Result<(), WhipWhepServerError> {
    //     if let Some(peer_connection) = &self.peer_connection {
    //         // Deleting previous peer_connection on this input which was not in Connected state
    //         if peer_connection.connection_state() == RTCPeerConnectionState::Connected {
    //             return Err(WhipWhepServerError::InternalError(format!(
    //                   "Another stream is currently connected to the given input_id: {output_id:?}. \
    //                   Disconnect the existing stream before starting a new one, or check if the input_id is correct."
    //               )));
    //         }
    //         if let Some(peer_connection) = self.peer_connection.take() {
    //             let output_id = output_id.clone();
    //             tokio::spawn(async move {
    //                 if let Err(err) = peer_connection.close().await {
    //                     warn!("Error while closing previous peer connection {output_id:?}: {err:?}")
    //                 }
    //             });
    //         }
    //     };
    //     self.peer_connection = Some(new_pc);
    //     Ok(())
    // }
}
