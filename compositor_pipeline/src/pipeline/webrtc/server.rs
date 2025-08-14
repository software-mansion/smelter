use std::{net::SocketAddr, sync::Arc};

use axum::{
    routing::{delete, get, patch, post},
    Router,
};
use compositor_render::error::ErrorStack;
use reqwest::StatusCode;
use serde_json::json;
use tokio::{net::TcpListener, sync::oneshot};
use tower_http::cors::CorsLayer;
use tracing::{error, info};

use crate::{
    error::InitPipelineError,
    pipeline::{
        webrtc::{
            server::{
                create_whep_session::handle_create_whep_session,
                create_whip_session::handle_create_whip_session,
                new_whep_ice_candidates::handle_new_whep_ice_candidates,
                new_whip_ice_candidates::handle_new_whip_ice_candidates,
                terminate_whep_session::handle_terminate_whep_session,
                terminate_whip_session::handle_terminate_whip_session,
            },
            WhipWhepPipelineState, WhipWhepServerHandle, WhipWhepServerState,
        },
        PipelineCtx,
    },
};

mod create_whep_session;
mod create_whip_session;
mod new_whep_ice_candidates;
mod new_whip_ice_candidates;
mod terminate_whep_session;
mod terminate_whip_session;

pub struct WhipWhepServer {
    listener: TcpListener,
}

impl WhipWhepServer {
    pub fn spawn(
        ctx: Arc<PipelineCtx>,
        state: &WhipWhepPipelineState,
    ) -> Result<WhipWhepServerHandle, InitPipelineError> {
        let port = state.port;
        let state = WhipWhepServerState {
            ctx: ctx.clone(),
            inputs: state.inputs.clone(),
            outputs: state.outputs.clone(),
        };

        let (shutdown_sender, shutdown_receiver) = oneshot::channel();
        let (init_result_sender, init_result_receiver) = oneshot::channel();
        ctx.tokio_rt.spawn(async move {
            info!("Starting HTTP server for WHIP/WHEP on port {port}");
            match WhipWhepServer::new(port).await {
                Ok(server) => {
                    init_result_sender.send(Ok(())).unwrap();
                    server.run(state, shutdown_receiver).await;
                }
                Err(err) => init_result_sender.send(Err(err)).unwrap(),
            }
        });
        init_result_receiver.blocking_recv().unwrap()?;

        Ok(WhipWhepServerHandle {
            shutdown_sender: Some(shutdown_sender),
        })
    }

    async fn new(port: u16) -> Result<Self, InitPipelineError> {
        let addr = SocketAddr::from(([0, 0, 0, 0], port));
        match tokio::net::TcpListener::bind(addr).await {
            Ok(listener) => Ok(Self { listener }),
            Err(err) => Err(InitPipelineError::WhipWhepServerInitError(err)),
        }
    }

    async fn run(
        self,
        state: WhipWhepServerState,
        shutdown_signal_receiver: oneshot::Receiver<()>,
    ) {
        let app = Router::new()
            .route("/status", get((StatusCode::OK, axum::Json(json!({})))))
            .route("/whip/:id", post(handle_create_whip_session))
            .route(
                "/whip/:id/:session_id",
                patch(handle_new_whip_ice_candidates),
            )
            .route(
                "/whip/:id/:session_id",
                delete(handle_terminate_whip_session),
            )
            .route("/whep/:id", post(handle_create_whep_session))
            .route(
                "/whep/:id/:session_id",
                patch(handle_new_whep_ice_candidates),
            )
            .route(
                "/whep/:id/:session_id",
                delete(handle_terminate_whep_session),
            )
            .layer(CorsLayer::permissive())
            .with_state(state);

        if let Err(err) = axum::serve(self.listener, app)
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
}
