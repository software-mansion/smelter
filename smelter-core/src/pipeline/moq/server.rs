use std::sync::Arc;
use std::time::Duration;

use moq_lite::{Origin, OriginConsumer, OriginProducer};
use moq_native::{ServerConfig, ServerTlsConfig};
use smelter_render::error::ErrorStack;
use tracing::{debug, info, warn};

use crate::pipeline::moq::{connection::spawn_broadcast_handler, state::MoqInputsState};

use crate::prelude::*;

pub struct MoqPipelineState {
    pub port: u16,
    pub origin: OriginProducer,
    pub inputs: MoqInputsState,
    pub tls_config: ServerTlsConfig,
}

impl MoqPipelineState {
    pub fn new(port: u16, tls_config: ServerTlsConfig) -> Arc<Self> {
        Arc::new(Self {
            port,
            origin: Origin::random().produce(),
            inputs: MoqInputsState::default(),
            tls_config,
        })
    }
}

pub struct MoqServerHandle {
    tasks: Vec<tokio::task::JoinHandle<()>>,
}

impl Drop for MoqServerHandle {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}

pub async fn spawn_moq_server(
    ctx: Arc<PipelineCtx>,
    state: &Arc<MoqPipelineState>,
) -> Result<MoqServerHandle, InitPipelineError> {
    let port = state.port;

    let mut config = ServerConfig::default();
    config.bind = Some(format!("[::]:{port}"));
    config.tls = state.tls_config.clone();

    let mut server_result: Option<Result<moq_native::Server, anyhow::Error>> = None;
    for _ in 0..5 {
        match config.clone().init() {
            Ok(server) => {
                server_result = Some(Ok(server));
                break;
            }
            Err(error) => {
                warn!("Failed to start MoQ server. Retrying ...");
                server_result = Some(Err(error));
            }
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    let server = match server_result.unwrap() {
        Ok(server) => server.with_consume(state.origin.clone()),
        Err(error) => return Err(InitPipelineError::MoqServerInitError(error)),
    };

    let accept_task = tokio::spawn(run_accept_loop(server));

    let origin_consumer = state.origin.consume();
    let moq_inputs = state.inputs.clone();
    let announce_task = tokio::spawn(run_announce_loop(origin_consumer, moq_inputs, ctx.clone()));

    info!(port, "MoQ server started");

    Ok(MoqServerHandle {
        tasks: vec![accept_task, announce_task],
    })
}

async fn run_accept_loop(mut server: moq_native::Server) {
    while let Some(request) = server.accept().await {
        tokio::spawn(async move {
            match request.ok().await {
                Ok(session) => {
                    info!("MoQ session established");
                    debug!(moq_version=?session.version());
                    let _ = session.closed().await;
                    info!("MoQ session closed");
                }
                Err(err) => {
                    warn!("MoQ handshake failed: {err}");
                }
            }
        });
    }
}

async fn run_announce_loop(
    mut origin_consumer: OriginConsumer,
    moq_inputs: MoqInputsState,
    ctx: Arc<PipelineCtx>,
) {
    while let Some((path, broadcast)) = origin_consumer.announced().await {
        let path_str = path.to_string();
        match broadcast {
            Some(broadcast) => {
                info!(path = %path_str, "MoQ broadcast announced");
                let input_ref = match moq_inputs.find_by_broadcast_path(&path_str) {
                    Ok(r) => r,
                    Err(err) => {
                        warn!(
                            "MoQ broadcast path not matched: {}",
                            ErrorStack::new(&err).into_string()
                        );
                        continue;
                    }
                };

                let ctx = ctx.clone();
                let moq_inputs = moq_inputs.clone();
                if let Err(err) = moq_inputs.get_mut_with(&input_ref, |input| {
                    input.ensure_no_active_connection(&input_ref)?;
                    let handle = spawn_broadcast_handler(ctx, &input_ref, input, broadcast);
                    input.connection_handle = handle;
                    Ok(())
                }) {
                    warn!(
                        "Failed to handle MoQ broadcast: {}",
                        ErrorStack::new(&err).into_string()
                    );
                }
            }
            None => {
                info!(path = %path_str, "MoQ broadcast unannounced");
            }
        }
    }
}
