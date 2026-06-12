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

        // The single `Session` instance is shared via `Arc` between this map, the
        // input state, and the per-session task. Dropping the map's `Arc` no longer
        // closes the transport (the input state may still hold an `Arc`), so close
        // each session explicitly. This ends all tasks awaiting the session to close.
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
        let moq_sessions = match weak_sessions.clone().upgrade() {
            Some(s) => s,
            None => break,
        };
        let moq_inputs = moq_inputs.clone();
        let ctx = ctx.clone();
        tokio::spawn(async move {
            // Per-session origin: the session↔path link is known at the point of
            // matching. This is also the future home of authentication
            // (`request.url()` / `request.peer_identity()` before `.ok()`).
            let origin = Origin::random().produce();
            let mut announced = origin.consume();

            let session = match request.with_consume(origin).ok().await {
                Ok(session) => session,
                Err(err) => {
                    warn!("MoQ handshake failed: {err}");
                    return;
                }
            };

            // A single `Session` instance, never cloned: `Drop for Session` closes the
            // transport unless that clone was explicitly closed, so any stray live
            // clone dropping would kill the connection. Sharing one instance via `Arc`
            // structurally removes that hazard; closing is always explicit.
            let session = Arc::new(Mutex::new(session));
            info!(moq_version=?session.lock().unwrap().version(), "MoQ session established");

            let session_id = NEXT_SESSION_ID.fetch_add(1, Ordering::Relaxed);
            {
                let mut sessions = moq_sessions.lock().unwrap();
                sessions.insert(session_id, session.clone());
            }

            while let Some((path, broadcast)) = announced.announced().await {
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
                            let handle = spawn_broadcast_handler(ctx, &input_ref, input, broadcast);
                            input.broadcast_handle = handle;
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
            {
                let mut sessions = moq_sessions.lock().unwrap();
                sessions.remove(&session_id);
            }
        });
    }
    server.close().await;
}
