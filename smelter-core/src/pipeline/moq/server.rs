use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use moq_native::moq_net::{Origin, OriginConsumer, OriginProducer, Session};
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

static NEXT_SESSION_ID: AtomicU64 = AtomicU64::new(0);

type MoqSessions = Arc<Mutex<HashMap<u64, Session>>>;
pub struct MoqServer {
    accept_task: tokio::task::JoinHandle<()>,
    announce_task: tokio::task::JoinHandle<()>,
    sessions: MoqSessions,
}

impl Drop for MoqServer {
    fn drop(&mut self) {
        self.accept_task.abort();
        self.announce_task.abort();
        let mut sessions = self.sessions.lock().unwrap();
        sessions.clear();
    }
}

pub async fn spawn_moq_server(
    ctx: Arc<PipelineCtx>,
    state: &Arc<MoqPipelineState>,
) -> Result<MoqServer, InitPipelineError> {
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

    let moq_sessions: MoqSessions = Arc::new(Mutex::new(HashMap::new()));
    let accept_task = tokio::spawn(run_accept_loop(server, moq_sessions.clone()));

    let origin_consumer = state.origin.consume();
    let moq_inputs = state.inputs.clone();
    let announce_task = tokio::spawn(run_announce_loop(origin_consumer, moq_inputs, ctx.clone()));

    info!(port, "MoQ server started");

    Ok(MoqServer {
        accept_task,
        announce_task,
        sessions: moq_sessions,
    })
}

async fn run_accept_loop(mut server: moq_native::Server, moq_sessions: MoqSessions) {
    while let Some(request) = server.accept().await {
        let moq_sessions = moq_sessions.clone();
        tokio::spawn(async move {
            match request.ok().await {
                Ok(session) => {
                    info!("MoQ session established");
                    debug!(moq_version=?session.version());
                    let session_id = NEXT_SESSION_ID.fetch_add(1, Ordering::Relaxed);
                    {
                        let mut sessions = moq_sessions.lock().unwrap();
                        sessions.insert(session_id, session.clone());
                    }
                    let _ = session.closed().await;
                    info!("MoQ session closed");
                    {
                        let mut sessions = moq_sessions.lock().unwrap();
                        sessions.remove(&session_id);
                    }
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
