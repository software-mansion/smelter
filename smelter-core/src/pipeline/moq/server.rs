use std::{sync::Arc, time::Duration};

use hang::moq_net::OriginConsumer;
use moq_native::{
    ServerConfig, ServerTlsConfig,
    moq_net::{Origin, Session},
};
use smelter_render::error::ErrorStack;
use tracing::{info, warn};

use crate::pipeline::moq::{
    certificate::load_or_create_self_signed_tls, connection::spawn_broadcast_handler,
    state::MoqServerState,
};

use crate::prelude::*;

pub struct MoqSession {
    session: Session,
    rt: Arc<tokio::runtime::Runtime>,
}

impl MoqSession {
    fn new(session: Session, rt: Arc<tokio::runtime::Runtime>) -> Self {
        Self { session, rt }
    }

    fn session(&self) -> &Session {
        &self.session
    }
}

impl Drop for MoqSession {
    fn drop(&mut self) {
        let _guard = self.rt.enter();
        self.session.close(hang::moq_net::Error::Cancel);
        tracing::info!("MoQ session closed!");
    }
}

pub struct MoqPipelineState {
    pub port: u16,
    pub inputs: MoqServerState,
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
            inputs: MoqServerState::default(),
            tls_config,
        }))
    }
}

pub struct MoqServer {
    accept_task: tokio::task::JoinHandle<()>,
}

impl Drop for MoqServer {
    fn drop(&mut self) {
        self.accept_task.abort();
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

    let accept_task = tokio::spawn(run_accept_loop(server, state.inputs.clone(), ctx));

    info!(port, "MoQ server started");

    Ok(MoqServer { accept_task })
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
    moq_inputs: MoqServerState,
    ctx: Arc<PipelineCtx>,
) {
    while let Some(request) = server.accept().await {
        let Some(url) = request.url() else {
            if let Err(error) = request.close(400).await {
                warn!(%error, "Error while rejecting MoQ connection.");
            }
            warn!("Rejected MoQ connection, unable to extract URL from the request.");
            continue;
        };

        let path = url.path().trim_start_matches('/');
        let decoded = match urlencoding::decode(path) {
            Ok(decoded) => decoded.into_owned(),
            Err(error) => {
                if let Err(error) = request.close(400).await {
                    warn!(%error, "Error while rejecting MoQ connection.");
                }
                warn!(%error, "Rejected MoQ connection, unable to decode URL path.");
                continue;
            }
        };

        let input_ref = match moq_inputs.find_by_path(&decoded) {
            Ok(input_ref) => input_ref,
            Err(error) => {
                if let Err(error) = request.close(404).await {
                    warn!(%error, "Error while rejecting MoQ connection.");
                }
                warn!(
                    "Rejected MoQ connection, no input matched the URL path: {}",
                    ErrorStack::new(&error).into_string()
                );
                continue;
            }
        };

        let origin = Origin::random().produce();
        let consumer = origin.consume();
        let session = match request.with_consume(origin).ok().await {
            Ok(session) => MoqSession::new(session, ctx.tokio_rt.clone()),
            Err(error) => {
                warn!(%error, "MoQ handshake failed.");
                continue;
            }
        };

        let moq_inputs = moq_inputs.clone();
        let ctx = ctx.clone();

        tokio::spawn(handle_session(
            session, consumer, moq_inputs, ctx, input_ref,
        ));
    }
    server.close().await;
}

async fn handle_session(
    session: MoqSession,
    mut origin_consumer: OriginConsumer,
    moq_inputs: MoqServerState,
    ctx: Arc<PipelineCtx>,
    input_ref: Ref<InputId>,
) {
    info!(moq_version=?session.session().version(), "MoQ session established");

    let Some((path, Some(broadcast))) = origin_consumer.announced().await else {
        warn!("MoQ session closed before announcing a broadcast");
        return;
    };
    info!(%path, input_id=%input_ref, "MoQ broadcast announced");

    if let Err(err) = moq_inputs.get_mut_with(&input_ref, |input| {
        input.ensure_no_active_connection(&input_ref)?;
        match spawn_broadcast_handler(ctx, &input_ref, input, broadcast) {
            Some(handle) => {
                input.connection_handle = Some(handle);
                input.session = Some(session);
            }
            None => {
                warn!("Failed to handle MoQ broadcast, input queue was dropped.");
            }
        }
        Ok(())
    }) {
        warn!(
            "Failed to handle MoQ broadcast: {}",
            ErrorStack::new(&err).into_string()
        );
    }
}
