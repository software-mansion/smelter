use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use crate::{
    pipeline::{
        input::Input,
        moq::{
            client::MoqConnection,
            input::connection::{BroadcastCtx, MoqConnectionError, handle_broadcast},
        },
    },
    queue::{QueueInput, WeakQueueInput},
};
use moq_net::origin;
use smelter_render::error::ErrorStack;
use tracing::{Instrument, Level, Span, debug, span, warn};

use crate::prelude::*;

pub struct MoqClientInput {
    should_close: Arc<AtomicBool>,
    _client: MoqConnection,
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

        let queue_input = QueueInput::new(&ctx, &input_ref, options.queue_options.clone());

        let (client, consumer) = MoqConnection::new_subscriber(&ctx, &options.endpoint_url)?;
        let should_close = Arc::new(AtomicBool::new(false));

        ctx.spawn_async_task(|ctx| {
            Self::run_broadcast_task(
                ctx,
                input_ref,
                consumer,
                options,
                should_close.clone(),
                queue_input.downgrade(),
            )
            .instrument(Span::current())
        });

        Ok((
            Input::MoqClient(MoqClientInput {
                should_close,
                _client: client,
            }),
            InputInitInfo::Other,
            queue_input,
        ))
    }

    async fn run_broadcast_task(
        ctx: Arc<PipelineCtx>,
        input_ref: Ref<InputId>,
        consumer: origin::Consumer,
        options: MoqClientInputOptions,
        should_close: Arc<AtomicBool>,
        queue_input: WeakQueueInput,
    ) {
        let broadcast_path = options.broadcast_path;
        let path = broadcast_path.trim_start_matches('/');
        // Each iteration handles one announcement of the broadcast, so a publisher
        // restart or a relay reconnect starts a new queue track group.
        while !should_close.load(Ordering::Relaxed) {
            // Fails once the origin closes, which happens when the input is dropped.
            let broadcast = match consumer.routed_broadcast(path).await {
                Ok(broadcast) => broadcast,
                Err(err) => {
                    debug!(%broadcast_path, %err, "Stopped waiting for MoQ broadcast");
                    return;
                }
            };

            let broadcast_result = handle_broadcast(
                ctx.clone(),
                input_ref.clone(),
                queue_input.clone(),
                BroadcastCtx {
                    broadcast: broadcast.clone(),
                    decoder_options: options.decoder_options,
                    buffer: options.buffer,
                    should_close: should_close.clone(),
                },
            )
            .await;

            match broadcast_result {
                Ok(()) => (),
                Err(MoqConnectionError::InputUnregistered) => return,
                Err(err) => warn!(
                    "Failed to receive broadcast: {}",
                    ErrorStack::new(&err).into_string()
                ),
            }
            // Block until broadcast is closed, so the next loop does not pick the same one
            // again.
            broadcast.closed().await;
        }
    }
}

impl Drop for MoqClientInput {
    fn drop(&mut self) {
        self.should_close.store(true, Ordering::Relaxed);
    }
}
