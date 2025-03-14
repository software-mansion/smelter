use crate::error::InitPipelineError;
use axum::{
    routing::{delete, get, patch, post},
    Router,
};
use compositor_render::InputId;
use error::WhipServerError;
use reqwest::StatusCode;
use serde_json::json;

use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::{
    mpsc,
    oneshot::{self, Sender},
};
use tower_http::cors::CorsLayer;
use tracing::error;
use webrtc::{
    peer_connection::{peer_connection_state::RTCPeerConnectionState, RTCPeerConnection},
    rtp_transceiver::rtp_codec::RTPCodecType,
};
use whip_handlers::{
    create_whip_session::handle_create_whip_session,
    new_whip_ice_candidates::handle_new_whip_ice_candidates,
    terminate_whip_session::handle_terminate_whip_session,
};

pub mod bearer_token;
pub mod error;
mod init_peer_connection;
mod whip_handlers;

use crate::queue::PipelineEvent;

use super::{
    input::whip::{depayloader::Depayloader, DecoderChannels},
    EncodedChunk, PipelineCtx,
};

pub async fn run_whip_whep_server(
    port: u16,
    state: Arc<PipelineCtx>,
    shutdown_signal_receiver: oneshot::Receiver<()>,
    init_result_sender: Sender<Result<(), InitPipelineError>>,
) {
    let app = Router::new()
        .route("/status", get((StatusCode::OK, axum::Json(json!({})))))
        .route("/whip/:id", post(handle_create_whip_session))
        .route("/session/:id", patch(handle_new_whip_ice_candidates))
        .route("/session/:id", delete(handle_terminate_whip_session))
        .layer(CorsLayer::permissive())
        .with_state(state.clone());

    let Ok(listener) = tokio::net::TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], port))).await
    else {
        if let Err(err) = init_result_sender.send(Err(InitPipelineError::WhipWhepServerInitError)) {
            error!("Cannot send init WHIP WHEP server result {err:?}");
        }
        return;
    };

    if let Err(err) = init_result_sender.send(Ok(())) {
        error!("Cannot send init WHIP WHEP server result {err:?}");
    };

    if let Err(err) = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(shutdown_signal_receiver))
        .await
    {
        error!("Cannot start WHIP WHEP server: {err:?}");
    };
}

async fn shutdown_signal(receiver: oneshot::Receiver<()>) {
    if let Err(err) = receiver.await {
        error!(
            "Error while receiving WHIP WHEP server shutdown signal {:?}",
            err
        );
    }
}

#[derive(Debug, Clone)]
pub struct WhipInputConnectionOptions {
    pub video_sender: mpsc::Sender<PipelineEvent<EncodedChunk>>,
    pub audio_sender: mpsc::Sender<PipelineEvent<EncodedChunk>>,
    pub bearer_token: Option<String>,
    pub peer_connection: Option<Arc<RTCPeerConnection>>,
    pub start_time_vid: Option<Instant>,
    pub start_time_aud: Option<Instant>,
    pub decoder_channels: DecoderChannels,
    pub depayloader: Arc<Mutex<Depayloader>>,
}

impl WhipInputConnectionOptions {
    pub fn get_or_initialize_elapsed_start_time(
        &mut self,
        track_kind: RTPCodecType,
    ) -> Option<Duration> {
        match track_kind {
            RTPCodecType::Video => {
                let start_time = self.start_time_vid.get_or_insert_with(Instant::now);
                Some(start_time.elapsed())
            }
            RTPCodecType::Audio => {
                let start_time = self.start_time_aud.get_or_insert_with(Instant::now);
                Some(start_time.elapsed())
            }
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct WhipWhepState {
    pub input_connections: Arc<Mutex<HashMap<InputId, WhipInputConnectionOptions>>>,
    pub stun_servers: Arc<Vec<String>>,
}

impl WhipWhepState {
    pub fn new(stun_servers: Arc<Vec<String>>) -> Arc<Self> {
        Arc::new(WhipWhepState {
            input_connections: Arc::from(Mutex::new(HashMap::new())),
            stun_servers,
        })
    }

    pub fn get_input_connection_options(
        &self,
        input_id: InputId,
    ) -> Result<WhipInputConnectionOptions, WhipServerError> {
        let connections = self.input_connections.lock().unwrap();
        if let Some(connection) = connections.get(&input_id) {
            if let Some(peer_connection) = connection.peer_connection.clone() {
                if peer_connection.connection_state() == RTCPeerConnectionState::Connected {
                    return Err(WhipServerError::InternalError(format!(
                        "Another stream is currently connected to the given input_id: {input_id:?}. \
                        Disconnect the existing stream before starting a new one, or check if the input_id is correct."
                    )));
                }
            }
            Ok(connection.clone())
        } else {
            Err(WhipServerError::NotFound(format!("{input_id:?} not found")))
        }
    }

    pub async fn update_peer_connection(
        &self,
        input_id: InputId,
        peer_connection: Arc<RTCPeerConnection>,
    ) -> Result<(), WhipServerError> {
        let mut connections = self.input_connections.lock().unwrap();
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
        let mut connections = self.input_connections.lock().unwrap();
        match connections.get_mut(&input_id) {
            Some(connection) => connection.get_or_initialize_elapsed_start_time(track_kind),
            None => {
                error!("{input_id:?} not found");
                None
            }
        }
    }
}
