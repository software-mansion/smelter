use std::sync::{Arc, atomic::AtomicBool};

use crate::{
    pipeline::{
        input::Input,
        moq::{
            MoqSession,
            input::connection::{BroadcastCtx, MoqEndpointKind, handle_broadcast},
        },
    },
    queue::{QueueInput, WeakQueueInput},
};
use hang::moq_net::{Origin, OriginConsumer};
use moq_native::ClientConfig;
use smelter_render::error::ErrorStack;
use tracing::{Instrument, Level, info, span, warn};
use url::Url;

use crate::prelude::*;

pub struct MoqClientInput {
    should_close: Arc<AtomicBool>,
    _session: MoqSession,
}

impl MoqClientInput {
    pub fn new_input(
        ctx: Arc<PipelineCtx>,
        input_ref: Ref<InputId>,
        options: MoqClientInputOptions,
    ) -> Result<(Input, InputInitInfo, QueueInput), InputInitError> {
        ctx.stats_sender.send(StatsEvent::NewInput {
            input_ref: input_ref.clone(),
            kind: InputProtocolKind::MoqClient,
        });

        let queue_input = QueueInput::new(&ctx, &input_ref, options.queue_options);

        let (session, consumer) = Self::connect(&ctx, &options.endpoint_url)?;
        let should_close = Arc::new(AtomicBool::new(false));

        Self::start_broadcast_handler_task(
            ctx,
            input_ref,
            consumer,
            options.broadcast_path,
            should_close.clone(),
            options.decoders,
            queue_input.downgrade(),
        );

        Ok((
            Input::MoqClient(MoqClientInput {
                should_close,
                _session: session,
            }),
            InputInitInfo::Other,
            queue_input,
        ))
    }

    fn connect(
        ctx: &Arc<PipelineCtx>,
        url: &str,
    ) -> Result<(MoqSession, OriginConsumer), MoqClientError> {
        let url = Url::parse(url).map_err(|err| MoqClientError::InvalidUrl(Arc::from(url), err))?;

        if !matches!(url.scheme(), "https" | "http") {
            return Err(MoqClientError::InvalidScheme(url.scheme().to_string()));
        }

        let mut config = ClientConfig::default();
        config.tls.disable_verify = Some(ctx.moq_disable_tls_verification);
        let client = config
            .init()
            .map_err(|err| MoqClientError::ClientInitFailed(format!("{err}")))?;

        let origin = Origin::random().produce();
        let consumer = origin.consume();
        let client = client.with_consume(origin);

        let session = ctx
            .tokio_rt
            .block_on(client.connect(url))
            .map_err(|err| MoqClientError::ConnectFailed(format!("{err}")))?;
        let session = MoqSession::new(session, ctx.tokio_rt.clone());
        info!(moq_version = ?session.version(), "MoQ client session established");
        Ok((session, consumer))
    }

    fn start_broadcast_handler_task(
        ctx: Arc<PipelineCtx>,
        input_ref: Ref<InputId>,
        mut consumer: OriginConsumer,
        broadcast_path: Arc<str>,
        should_close: Arc<AtomicBool>,
        decoders: MoqInputDecoders,
        queue_input: WeakQueueInput,
    ) {
        let rt = ctx.tokio_rt.clone();

        let span = span!(
            Level::INFO,
            "MoQ client input",
            input_id = input_ref.to_string()
        );
        rt.spawn(
            async move {
                let broadcast = loop {
                    if should_close.load(std::sync::atomic::Ordering::Relaxed) {
                        return;
                    }

                    let Some((path, Some(broadcast))) = consumer.announced().await else {
                        warn!(%broadcast_path, "MoQ session closed before announcing required broadcast.");
                        return;
                    };

                    if path.as_str().trim_start_matches("/") == broadcast_path.as_ref().trim_start_matches("/") {
                        break broadcast;
                    }
                };

                let broadcast_ctx = BroadcastCtx {
                    broadcast,
                    decoders,
                    should_close,
                    endpoint_kind: MoqEndpointKind::Client,
                };
                let broadcast_result = handle_broadcast(ctx, input_ref, queue_input, broadcast_ctx).await;
                if let Err(err) = broadcast_result {
                    warn!(
                        "Failed to receive broadcast: {}",
                        ErrorStack::new(&err).into_string()
                    );
                }
            }
            .instrument(span)
        );
    }
}

impl Drop for MoqClientInput {
    fn drop(&mut self) {
        self.should_close
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}
