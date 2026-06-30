use std::{sync::Arc, time::Duration};

use hang::moq_net::{OriginConsumer, OriginProducer};
use moq_native::{Request, ServerConfig, ServerTlsConfig, moq_net::Origin};
use smelter_render::error::ErrorStack;
use tracing::{Instrument, Level, info, span, warn};

use crate::{
    pipeline::moq::{
        MoqSession,
        input::connection::{BroadcastCtx, MoqEndpointKind, handle_broadcast},
        server::{certificate::load_or_create_self_signed_tls, state::MoqServerState},
    },
    queue::WeakQueueInput,
};

use crate::prelude::*;

mod certificate;
pub(crate) mod state;

pub(crate) use certificate::SelfSignedTlsError;

pub struct MoqPipelineState {
    pub port: u16,
    pub server_state: MoqServerState,
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
            server_state: MoqServerState::default(),
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
        Err(err) => return Err(InitPipelineError::MoqServerInitError(format!("{err}"))),
    };

    let accept_task = tokio::spawn(run_accept_loop(server, state.server_state.clone(), ctx));

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
        let (origin, input_ref) = match handle_incoming_connection(&request, &moq_inputs).await {
            Ok(oi) => oi,
            Err(err) => {
                warn!(
                    "MoQ connection rejected: {}",
                    ErrorStack::new(&err).into_string()
                );
                _ = request.close(err.http_status_code()).await;
                continue;
            }
        };
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

        tokio::spawn(async {
            if let Err(err) = handle_session(session, consumer, moq_inputs, ctx, input_ref).await {
                warn!(
                    "Failed to handle MoQ broadcast: {}",
                    ErrorStack::new(&err).into_string()
                );
            }
        });
    }
    server.close().await;
}

async fn handle_incoming_connection(
    request: &Request,
    moq_inputs: &MoqServerState,
) -> Result<(OriginProducer, Ref<InputId>), MoqServerError> {
    let Some(url) = request.url() else {
        return Err(MoqServerError::UrlNotFound);
    };

    let input_name_encoded = url.path().trim_start_matches('/');
    let input_name = match urlencoding::decode(input_name_encoded) {
        Ok(decoded) => decoded.into_owned(),
        Err(err) => return Err(MoqServerError::UrlDecodeFailed(err)),
    };

    let input_ref = moq_inputs.find_by_url(&input_name)?;

    let auth_token = url
        .query_pairs()
        .find(|(key, _value)| key == "token")
        .map(|(_key, value)| value);

    let Some(auth_token) = auth_token else {
        return Err(MoqServerError::MissingToken(input_ref.id().clone()));
    };
    moq_inputs.validate_auth_token(&input_ref, &auth_token)?;

    let origin = Origin::random().produce();
    Ok((origin, input_ref))
}

async fn handle_session(
    session: MoqSession,
    mut origin_consumer: OriginConsumer,
    moq_inputs: MoqServerState,
    ctx: Arc<PipelineCtx>,
    input_ref: Ref<InputId>,
) -> Result<(), MoqServerError> {
    info!(moq_version=?session.version(), "MoQ session established");

    // waiting for the first announced path from session
    let Some((path, Some(broadcast))) = origin_consumer.announced().await else {
        warn!("MoQ session closed before announcing a broadcast");
        return Ok(());
    };
    info!(%path, input_id=%input_ref, "MoQ broadcast announced");

    moq_inputs.get_mut_with(&input_ref, |input| {
        input.ensure_no_active_connection(&input_ref)?;
        let broadcast_ctx = BroadcastCtx {
            broadcast,
            decoders: input.decoders,
            should_close: input.should_close.clone(),
            endpoint_kind: MoqEndpointKind::Server,
        };
        let Some(handle) =
            start_broadcast_handler_task(ctx, &input_ref, input.queue_input.clone(), broadcast_ctx)
        else {
            return Err(MoqServerError::QueueDropped);
        };
        input.connection_task_handle = Some(handle);
        input.session = Some(session);
        Ok(())
    })
}

fn start_broadcast_handler_task(
    ctx: Arc<PipelineCtx>,
    input_ref: &Ref<InputId>,
    queue_input: WeakQueueInput,
    broadcast_ctx: BroadcastCtx,
) -> Option<tokio::task::JoinHandle<()>> {
    let input_ref = input_ref.clone();
    let rt = ctx.tokio_rt.clone();

    let span = span!(
        Level::INFO,
        "MoQ server input",
        input_id = input_ref.to_string()
    );

    let handle = rt.spawn(
        async move {
            let broadcast_result =
                handle_broadcast(ctx, input_ref.clone(), queue_input, broadcast_ctx).await;
            if let Err(error) = broadcast_result {
                warn!(
                    "Failed to receive broadcast: {}",
                    ErrorStack::new(&error).into_string()
                );
            }
        }
        .instrument(span),
    );

    Some(handle)
}
