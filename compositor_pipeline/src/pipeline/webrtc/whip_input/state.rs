use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use compositor_render::InputId;
use tracing::error;
use webrtc::{peer_connection::RTCPeerConnection, rtp_transceiver::rtp_codec::RTPCodecType};

use crate::pipeline::webrtc::{
    error::WhipServerError, whip_input::connection::WhipInputConnectionState,
};

#[derive(Debug, Clone, Default)]
pub(crate) struct WhipInputState(Arc<Mutex<HashMap<InputId, WhipInputConnectionState>>>);

impl WhipInputState {
    pub fn get_input_connection_options(
        &self,
        input_id: InputId,
    ) -> Result<WhipInputConnectionState, WhipServerError> {
        let connections = self.0.lock().unwrap();
        match connections.get(&input_id) {
            Some(connection) => Ok(connection.clone()),
            None => Err(WhipServerError::NotFound(format!("{input_id:?} not found"))),
        }
    }

    pub async fn update_peer_connection(
        &self,
        input_id: InputId,
        peer_connection: Arc<RTCPeerConnection>,
    ) -> Result<(), WhipServerError> {
        let mut connections = self.0.lock().unwrap();
        if let Some(connection) = connections.get_mut(&input_id) {
            connection.peer_connection = Some(peer_connection);
            Ok(())
        } else {
            Err(WhipServerError::InternalError(format!(
                "Peer connection with input_id: {:?} does not exist",
                input_id.0
            )))
        }
    }

    pub fn get_time_elapsed_from_input_start(
        &self,
        input_id: InputId,
        track_kind: RTPCodecType,
    ) -> Option<Duration> {
        let mut connections = self.0.lock().unwrap();
        match connections.get_mut(&input_id) {
            Some(connection) => connection.get_or_initialize_elapsed_start_time(track_kind),
            None => {
                error!("{input_id:?} not found");
                None
            }
        }
    }

    pub fn add_input(&self, input_id: &InputId, input: WhipInputConnectionState) {
        let mut guard = self.0.lock().unwrap();
        guard.insert(input_id.clone(), input);
    }

    pub fn close_input(&self, input_id: &InputId) {
        let mut guard = self.0.lock().unwrap();
        if let Some(input) = guard.get_mut(input_id) {
            if let Some(peer_connection) = input.peer_connection.clone() {
                let input_id = input_id.clone();
                tokio::spawn(async move {
                    if let Err(err) = peer_connection.close().await {
                        error!("Cannot close peer_connection for {:?}: {:?}", input_id, err);
                    };
                });
            }
        }
        guard.remove(input_id);
    }

    pub fn take_peer_connection(&self, input_id: &InputId) -> Result<Option<Arc<RTCPeerConnection>>, WhipServerError> {
        let guard = self.0.lock().unwrap();
        match guard.get_mut(&input_id) {
            Some(connection) => Ok(connection.peer_connection.take()),
            None => Err(WhipServerError::NotFound(format!("{input_id:?} not found"))),
        }
    }
}
