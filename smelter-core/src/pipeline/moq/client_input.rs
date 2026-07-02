use std::sync::{Arc, atomic::AtomicBool};

use crate::{
    InputInitInfo, PipelineCtx, Ref,
    error::InputInitError,
    pipeline::{
        input::Input,
        moq::{MoqSession, connection::start_broadcast_handler_task},
    },
    prelude::MoqClientInputOptions,
    queue::{QueueInput, WeakQueueInput},
};
use hang::moq_net::{Origin, OriginConsumer};
use moq_native::ClientConfig;
use smelter_render::InputId;
use tracing::{info, warn};
use url::Url;

use crate::prelude::*;

pub struct MoqClientInput {
    pub queue_input: WeakQueueInput,
    pub decoders: MoqInputDecoders,
    pub should_close: Arc<AtomicBool>,
    pub session: Option<MoqSession>,
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

        let mut input = Self {
            queue_input: queue_input.downgrade(),
            decoders: options.decoders,
            should_close: Arc::new(false.into()),
            session: None,
        };
        let consumer = input.connect(&ctx, &options.url)?;

        spawn_broadcast_handler(
            ctx,
            input_ref.clone(),
            input.queue_input.clone(),
            input.decoders,
            input.should_close.clone(),
            consumer,
            options.broadcast_path,
        );

        Ok((Input::MoqClient(input), InputInitInfo::Other, queue_input))
    }

    fn connect(
        &mut self,
        ctx: &Arc<PipelineCtx>,
        url: &str,
    ) -> Result<OriginConsumer, MoqClientError> {
        let url = Url::parse(url).map_err(|err| MoqClientError::InvalidUrl(Arc::from(url), err))?;

        if url.scheme() != "https" {
            return Err(MoqClientError::InvalidScheme(url.scheme().to_string()));
        }

        let mut config = ClientConfig::default();
        // TODO: (@jbrs) TLS certificate verification MUST be handled properly before this is used in
        // production. Disabling it allows man-in-the-middle attacks.
        config.tls.disable_verify = Some(true);
        let client = config.init().map_err(MoqClientError::ClientInitFailed)?;

        let origin = Origin::random().produce();
        let consumer = origin.consume();
        let client = client.with_consume(origin);

        let session = ctx
            .tokio_rt
            .block_on(client.connect(url))
            .map_err(MoqClientError::ConnectFailed)?;
        let session = MoqSession::new(session, ctx.tokio_rt.clone());
        info!(moq_version = ?session.version(), "MoQ client session established");
        self.session = Some(session);
        Ok(consumer)
    }
}

impl Drop for MoqClientInput {
    fn drop(&mut self) {
        self.should_close
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

fn spawn_broadcast_handler(
    ctx: Arc<PipelineCtx>,
    input_ref: Ref<InputId>,
    queue_input: WeakQueueInput,
    decoders: MoqInputDecoders,
    should_close: Arc<AtomicBool>,
    mut consumer: OriginConsumer,
    broadcast_path: Arc<str>,
) {
    let rt = ctx.tokio_rt.clone();
    rt.spawn(async move {
        let broadcast = loop {
            if should_close.load(std::sync::atomic::Ordering::Relaxed) {
                return;
            }

            let Some((path, Some(broadcast))) = consumer.announced().await else {
                warn!(%broadcast_path, "MoQ session closed before announcing required broadcast.");
                return;
            };

            if path.as_str() == broadcast_path.as_ref() {
                break broadcast;
            }
        };

        if start_broadcast_handler_task(
            ctx,
            &input_ref,
            queue_input,
            decoders,
            should_close,
            broadcast,
        )
        .is_none()
        {
            warn!("Unable to spawn broadcast handler, input queue was dropped.")
        }
    });
}

// XXX: Be sure to remove that
// let span = span!(
//     Level::INFO,
//     "MoQ client input",
//     input_id = input_ref.to_string()
// );
//
// let handle = ctx.tokio_rt.spawn(
//     async move {
//         // waiting for the first announced path from the relay
//         let Some((path, Some(_broadcast))) = consumer.announced().await else {
//             warn!("MoQ session closed before announcing a broadcast");
//             return;
//         };
//         info!(%path, "MoQ broadcast announced");
//         todo!("broadcast handling")
//     }
//     .instrument(span),
// );
