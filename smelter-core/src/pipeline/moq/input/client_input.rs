use std::sync::{Arc, atomic::AtomicBool};

use crate::{
    pipeline::{
        input::Input,
        moq::{
            MoqSession, client_config,
            input::connection::{BroadcastCtx, handle_broadcast},
        },
    },
    queue::{QueueInput, WeakQueueInput},
};
use hang::moq_net::{BroadcastConsumer, Origin, OriginConsumer};
use smelter_render::error::ErrorStack;
use tracing::{Instrument, Level, Span, info, span, warn};
use url::Url;

use crate::prelude::*;

struct BroadcastOptions {
    broadcast_path: Arc<str>,
    decoder_options: MoqInputDecoders,
    buffer: LiveInputBufferOptions,
}

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
        let _span = span!(
            Level::INFO,
            "MoQ client input",
            input_id = input_ref.to_string()
        )
        .entered();

        ctx.stats_sender.send(StatsEvent::NewInput {
            input_ref: input_ref.clone(),
            kind: InputProtocolKind::MoqClient,
        });

        let MoqClientInputOptions {
            endpoint_url,
            broadcast_path,
            decoder_options,
            queue_options,
            buffer,
        } = options;
        let queue_input = QueueInput::new(&ctx, &input_ref, queue_options);

        let (session, consumer) = Self::connect(&ctx, &endpoint_url)?;
        let should_close = Arc::new(AtomicBool::new(false));

        let broadcast_options = BroadcastOptions {
            broadcast_path,
            decoder_options,
            buffer,
        };
        Self::start_broadcast_handler_task(
            ctx,
            input_ref,
            consumer,
            broadcast_options,
            should_close.clone(),
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

        let client = client_config(&url, ctx.moq_disable_tls_verification)
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
        consumer: OriginConsumer,
        options: BroadcastOptions,
        should_close: Arc<AtomicBool>,
        queue_input: WeakQueueInput,
    ) {
        let rt = ctx.tokio_rt.clone();
        let BroadcastOptions {
            broadcast_path,
            decoder_options,
            buffer,
        } = options;

        rt.spawn(
            async move {
                let Some(broadcast) =
                    wait_for_broadcast(consumer, broadcast_path, &should_close).await
                else {
                    return;
                };

                let broadcast_ctx = BroadcastCtx {
                    broadcast,
                    decoder_options,
                    buffer,
                    should_close,
                };
                let broadcast_result =
                    handle_broadcast(ctx, input_ref, queue_input, broadcast_ctx).await;
                if let Err(err) = broadcast_result {
                    warn!(
                        "Failed to receive broadcast: {}",
                        ErrorStack::new(&err).into_string()
                    );
                }
            }
            .instrument(Span::current()),
        );
    }
}

impl Drop for MoqClientInput {
    fn drop(&mut self) {
        self.should_close
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

async fn wait_for_broadcast(
    mut consumer: OriginConsumer,
    broadcast_path: Arc<str>,
    should_close: &Arc<AtomicBool>,
) -> Option<BroadcastConsumer> {
    loop {
        if should_close.load(std::sync::atomic::Ordering::Relaxed) {
            return None;
        }

        let Some((path, Some(broadcast))) = consumer.announced().await else {
            warn!(%broadcast_path, "MoQ session closed before announcing required broadcast.");
            return None;
        };

        let expected_path = broadcast_path.as_ref().trim_start_matches("/");
        let incoming_path = path.as_str().trim_start_matches("/");
        if incoming_path == expected_path {
            return Some(broadcast);
        }
    }
}
