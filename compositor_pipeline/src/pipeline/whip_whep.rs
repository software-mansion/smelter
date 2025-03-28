use crate::error::InitPipelineError;
use axum::{
    routing::{delete, get, patch, post},
    Router,
};
use compositor_render::{error::ErrorStack, InputId};
use error::WhipServerError;
use reqwest::StatusCode;
use serde_json::json;

use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::oneshot::{self, Sender};
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

use super::{input::whip::DecodedDataSender, PipelineCtx};

pub async fn run_whip_whep_server(
    pipeline_ctx: Arc<PipelineCtx>,
    whip_inputs_state: WhipInputState,
    shutdown_signal_receiver: oneshot::Receiver<()>,
    init_result_sender: Sender<Result<(), InitPipelineError>>,
) {
    let port = pipeline_ctx.whip_whep_server_port;
    let app = Router::new()
        .route("/status", get((StatusCode::OK, axum::Json(json!({})))))
        .route("/whip/:id", post(handle_create_whip_session))
        .route("/session/:id", patch(handle_new_whip_ice_candidates))
        .route("/session/:id", delete(handle_terminate_whip_session))
        .layer(CorsLayer::permissive())
        .with_state(WhipWhepState {
            pipeline_ctx,
            inputs: whip_inputs_state,
        });

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => {
            init_result_sender.send(Ok(())).unwrap();
            listener
        }
        Err(err) => {
            init_result_sender
                .send(Err(InitPipelineError::WhipWhepServerInitError(err)))
                .unwrap();
            return;
        }
    };

    if let Err(err) = axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            if shutdown_signal_receiver.await.is_err() {
                error!("Channel closed before sending shutdown signal.");
            }
        })
        .await
    {
        error!(
            "WHIP WHEP server exited with an error {}",
            ErrorStack::new(&err).into_string()
        );
    };
}

#[derive(Debug, Clone)]
pub struct WhipInputConnectionState {
    pub bearer_token: Option<String>,
    pub peer_connection: Option<Arc<RTCPeerConnection>>,
    pub start_time_video: Option<Instant>,
    pub start_time_audio: Option<Instant>,
    pub decoded_data_sender: DecodedDataSender,
}

impl WhipInputConnectionState {
    pub fn get_or_initialize_elapsed_start_time(
        &mut self,
        track_kind: RTPCodecType,
    ) -> Option<Duration> {
        match track_kind {
            RTPCodecType::Video => {
                let start_time = self.start_time_video.get_or_insert_with(Instant::now);
                Some(start_time.elapsed())
            }
            RTPCodecType::Audio => {
                let start_time = self.start_time_audio.get_or_insert_with(Instant::now);
                Some(start_time.elapsed())
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct WhipWhepState {
    pub pipeline_ctx: Arc<PipelineCtx>,
    pub inputs: WhipInputState,
}

impl WhipWhepState {
    pub fn new(pipeline_ctx: &Arc<PipelineCtx>) -> Arc<Self> {
        Self {
            pipeline_ctx: pipeline_ctx.clone(),
            inputs: WhipInputState::new(),
        }
        .into()
    }
}

#[derive(Debug, Clone)]
pub struct WhipInputState(Arc<Mutex<HashMap<InputId, WhipInputConnectionState>>>);

impl Default for WhipInputState {
    fn default() -> Self {
        Self::new()
    }
}

impl WhipInputState {
    pub fn new() -> Self {
        Self(Arc::from(Mutex::new(HashMap::new())))
    }

    pub fn get_input_connection_options(
        &self,
        input_id: InputId,
    ) -> Result<WhipInputConnectionState, WhipServerError> {
        let connections = self.0.lock().unwrap();
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
}
