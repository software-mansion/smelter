use std::sync::Arc;

use moq_native::moq_lite::Origin;
use tracing::{info, warn};
use url::Url;

use crate::{
    pipeline::{input::Input, moq::connection::handle_broadcast},
    queue::QueueInput,
};

use crate::prelude::*;

pub struct MoqClientInput(tokio::task::JoinHandle<()>);

impl Drop for MoqClientInput {
    fn drop(&mut self) {
        self.0.abort();
    }
}

impl MoqClientInput {
    pub fn new_input(
        ctx: Arc<PipelineCtx>,
        input_ref: Ref<InputId>,
        options: MoqClientInputOptions,
    ) -> Result<(Input, InputInitInfo, QueueInput), InputInitError> {
        let queue_input = QueueInput::new(&ctx, &input_ref, options.queue_options);
        let decoders = options.decoders;
        let url: Url = options.url.parse().map_err(|err: url::ParseError| {
            MoqClientError::ConnectionError(anyhow::anyhow!(err))
        })?;
        let broadcast_path = options.broadcast_path;

        let task_ctx = ctx.clone();
        let task_input_ref = input_ref.clone();
        let task_queue_input = queue_input.clone();

        let task = ctx.tokio_rt.spawn(async move {
            if let Err(err) = run_moq_client(
                task_ctx,
                task_input_ref.clone(),
                decoders,
                task_queue_input,
                url,
                broadcast_path,
            )
            .await
            {
                warn!(
                    input_id = %task_input_ref,
                    "MoQ client error: {err:#}",
                );
            }
        });

        Ok((
            Input::MoqClient(Self(task)),
            InputInitInfo::Other,
            queue_input,
        ))
    }
}

async fn run_moq_client(
    ctx: Arc<PipelineCtx>,
    input_ref: Ref<InputId>,
    decoders: MoqInputDecoders,
    queue_input: QueueInput,
    url: Url,
    broadcast_path: Arc<str>,
) -> anyhow::Result<()> {
    let mut config = moq_native::ClientConfig::default();

    // TODO: (@jbrs) This is fine for the experimental, however will need to be addressed in the
    // "complete" version.
    config.tls.disable_verify = Some(true);

    let origin = Origin::random().produce();
    let mut origin_consumer = origin.consume();

    let client = config.init()?.with_consume(origin);
    let session = client.connect(url).await?;

    info!(input_id = %input_ref, "MoQ client connected to relay");

    tokio::spawn(async move {
        let _ = session.closed().await;
    });

    while let Some((path, broadcast)) = origin_consumer.announced().await {
        let path_str = path.to_string();
        if path_str == broadcast_path.as_ref()
            && let Some(broadcast) = broadcast
        {
            info!(
                input_id = %input_ref,
                path = %path_str,
                "MoQ client received broadcast"
            );
            handle_broadcast(ctx, input_ref, decoders, queue_input, broadcast).await;
            return Ok(());
        }
    }

    Err(anyhow::anyhow!(
        "MoQ relay closed without announcing the requested broadcast"
    ))
}
