use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use moq_native::{
    ServerConfig, ServerTlsConfig,
    moq_net::{Origin, OriginConsumer, OriginProducer, Session},
};
use tracing::{debug, info, warn};

use crate::pipeline::moq::{
    certificate::load_or_create_self_signed_tls, connection::spawn_broadcast_handler,
    state::MoqInputsState,
};
use smelter_render::error::ErrorStack;

use crate::prelude::*;

pub struct MoqPipelineState {
    pub port: u16,
    pub origin: OriginProducer,
    pub inputs: MoqInputsState,
    pub tls_config: ServerTlsConfig,
}

impl MoqPipelineState {
    pub fn new(
        port: u16,
        tls_config: Option<ServerTlsConfig>,
    ) -> Result<Arc<Self>, InitPipelineError> {
        let tls_config = match tls_config {
            Some(tc) => tc,
            None => load_or_create_self_signed_tls()?,
        };

        Ok(Arc::new(Self {
            port,
            origin: Origin::random().produce(),
            inputs: MoqInputsState::default(),
            tls_config,
        }))
    }
}

static NEXT_SESSION_ID: AtomicU64 = AtomicU64::new(0);

type MoqSessions = Arc<Mutex<HashMap<u64, Session>>>;
type WeakMoqSessions = Weak<Mutex<HashMap<u64, Session>>>;
pub struct MoqServer {
    accept_task: tokio::task::JoinHandle<()>,
    announce_task: tokio::task::JoinHandle<()>,
    sessions: MoqSessions,
}

impl Drop for MoqServer {
    fn drop(&mut self) {
        self.accept_task.abort();
        self.announce_task.abort();

        // Each session task in the `run_accept_loop` holds `Arc` reference to the sessions map.
        // This clears the map, which closes every session and ends all tasks awaiting
        // for the corresponding session to close.
        self.sessions.lock().unwrap().clear();
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

    let server = match try_start_server(config).await {
        Ok(server) => server.with_consume(state.origin.clone()),
        Err(error) => return Err(InitPipelineError::MoqServerInitError(error)),
    };

    let moq_sessions: MoqSessions = Arc::new(Mutex::new(HashMap::new()));
    let accept_task = tokio::spawn(run_accept_loop(server, Arc::downgrade(&moq_sessions)));

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

async fn try_start_server(config: ServerConfig) -> Result<moq_native::Server, anyhow::Error> {
    for _ in 0..4 {
        match config.clone().init() {
            Ok(server) => {
                return Ok(server);
            }
            Err(error) => {
                warn!("Failed to start MoQ server. Retrying ...");
                debug!(%error, "Failed to start MoQ server.");
            }
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    config.init()
}

async fn run_accept_loop(mut server: moq_native::Server, weak_sessions: WeakMoqSessions) {
    while let Some(request) = server.accept().await {
        let moq_sessions = match weak_sessions.clone().upgrade() {
            Some(s) => s,
            None => break,
        };
        tokio::spawn(async move {
            match request.ok().await {
                Ok(session) => {
                    info!(moq_version=?session.version(), "MoQ session established");
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
    server.close().await;
}

async fn run_announce_loop(
    mut origin_consumer: OriginConsumer,
    moq_inputs: MoqInputsState,
    ctx: Arc<PipelineCtx>,
) {
    while let Some((path, broadcast)) = origin_consumer.announced().await {
        match broadcast {
            Some(broadcast) => {
                info!(%path, "MoQ broadcast announced");
                let input_ref = match moq_inputs.find_by_broadcast_path(&path) {
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
                info!(%path, "MoQ broadcast unannounced");
            }
        }
    }
}
