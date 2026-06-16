use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use hang::moq_net::OriginConsumer;
use moq_native::{
    ServerConfig, ServerTlsConfig,
    moq_net::{Error, Origin, Session},
};
use tracing::{info, warn};

use crate::pipeline::moq::{
    certificate::load_or_create_self_signed_tls, connection::spawn_broadcast_handler,
    state::MoqInputsState,
};
use smelter_render::error::ErrorStack;

use crate::prelude::*;

pub struct MoqPipelineState {
    pub port: u16,
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
            inputs: MoqInputsState::default(),
            tls_config,
        }))
    }
}

static NEXT_SESSION_ID: AtomicU64 = AtomicU64::new(0);

type MoqSessions = Arc<Mutex<HashMap<u64, Arc<Mutex<Session>>>>>;
type WeakMoqSessions = Weak<Mutex<HashMap<u64, Arc<Mutex<Session>>>>>;
pub struct MoqServer {
    accept_task: tokio::task::JoinHandle<()>,
    sessions: MoqSessions,
}

impl Drop for MoqServer {
    fn drop(&mut self) {
        self.accept_task.abort();

        let mut sessions = self.sessions.lock().unwrap();
        for session in sessions.values() {
            session.lock().unwrap().close(Error::Cancel);
        }
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

    let server = match try_start_server(config).await {
        Ok(server) => server,
        Err(error) => return Err(InitPipelineError::MoqServerInitError(error)),
    };

    let moq_sessions: MoqSessions = Arc::new(Mutex::new(HashMap::new()));
    let accept_task = tokio::spawn(run_accept_loop(
        server,
        Arc::downgrade(&moq_sessions),
        state.inputs.clone(),
        ctx.clone(),
    ));

    info!(port, "MoQ server started");

    Ok(MoqServer {
        accept_task,
        sessions: moq_sessions,
    })
}

async fn try_start_server(config: ServerConfig) -> Result<moq_native::Server, anyhow::Error> {
    for _ in 0..4 {
        match config.clone().init() {
            Ok(server) => {
                return Ok(server);
            }
            Err(_) => {
                warn!("Failed to start MoQ server. Retrying ...");
            }
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    config.init()
}

async fn run_accept_loop(
    mut server: moq_native::Server,
    weak_sessions: WeakMoqSessions,
    moq_inputs: MoqInputsState,
    ctx: Arc<PipelineCtx>,
) {
    while let Some(request) = server.accept().await {
        if weak_sessions.clone().upgrade().is_none() {
            break;
        }

        let origin = Origin::random().produce();
        let consumer = origin.consume();
        let session = match request.with_consume(origin).ok().await {
            Ok(session) => session,
            Err(error) => {
                warn!(%error, "MoQ handshake failed.");
                continue;
            }
        };

        let moq_inputs = moq_inputs.clone();
        let ctx = ctx.clone();
        let weak_sessions = weak_sessions.clone();

        tokio::spawn(handle_session(
            session,
            consumer,
            weak_sessions,
            moq_inputs,
            ctx,
        ));
    }
    server.close().await;
}

async fn handle_session(
    session: Session,
    mut origin_consumer: OriginConsumer,
    weak_sessions: WeakMoqSessions,
    moq_inputs: MoqInputsState,
    ctx: Arc<PipelineCtx>,
) {
    info!(moq_version=?session.version(), "MoQ session established");
    let session = Arc::new(Mutex::new(session));

    let session_id = NEXT_SESSION_ID.fetch_add(1, Ordering::Relaxed);
    match weak_sessions.upgrade() {
        Some(moq_sessions) => {
            let mut guard = moq_sessions.lock().unwrap();
            guard.insert(session_id, session.clone());
        }
        None => return,
    }

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
                let session = session.clone();
                if let Err(err) = moq_inputs.get_mut_with(&input_ref, |input| {
                    input.ensure_no_active_connection(&input_ref)?;
                    input.connection_handle =
                        spawn_broadcast_handler(ctx, &input_ref, input, broadcast);
                    input.session = Some(session);
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

    info!("MoQ session closed");
    if let Some(moq_sessions) = weak_sessions.upgrade() {
        let mut guard = moq_sessions.lock().unwrap();
        guard.remove(&session_id);
    }
}
