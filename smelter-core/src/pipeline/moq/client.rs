use std::{sync::Arc, time::Duration};

use moq_net::origin;
use moq_tokio::{Client, Connection};
use tracing::info;
use url::Url;

use crate::prelude::*;

/// How long the subscriber waits for the first session before registration fails.
const INITIAL_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// MoQ connection to a relay. Dropping it closes the session.
///
/// A subscriber redials indefinitely after the session drops; a publisher does not.
pub(super) struct MoqConnection {
    connection: Connection,
    rt: tokio::runtime::Handle,
}

impl MoqConnection {
    /// Connects to `url` and returns a consumer of broadcasts announced by the relay.
    pub fn new_subscriber(
        ctx: &Arc<PipelineCtx>,
        url: &str,
    ) -> Result<(Self, origin::Consumer), MoqClientError> {
        let (client, url, producer) = create_client(ctx, url, true)?;
        let consumer = producer.consume();
        let connection = ctx
            .tokio_rt
            .block_on(async {
                let connect = client.with_subscriber(producer).connect(url).established();
                tokio::time::timeout(INITIAL_CONNECT_TIMEOUT, connect).await
            })
            .map_err(|_| {
                MoqClientError::ConnectFailed(format!(
                    "timed out after {INITIAL_CONNECT_TIMEOUT:?}"
                ))
            })?
            .map_err(|err| MoqClientError::ConnectFailed(format!("{err}")))?;
        info!(moq_version = ?connection.version(), "MoQ subscriber session established");
        let connection = Self {
            connection,
            rt: ctx.tokio_rt.clone(),
        };
        Ok((connection, consumer))
    }

    /// Connects to `url` and returns a producer whose broadcasts are announced to the relay.
    pub fn new_publisher(
        ctx: &Arc<PipelineCtx>,
        url: &str,
    ) -> Result<(Self, origin::Producer), MoqClientError> {
        let (client, url, producer) = create_client(ctx, url, false)?;
        let connection = ctx
            .tokio_rt
            .block_on(
                client
                    .with_publisher(producer.consume())
                    .connect(url)
                    .established(),
            )
            .map_err(|err| MoqClientError::ConnectFailed(format!("{err}")))?;
        info!(moq_version = ?connection.version(), "MoQ publisher session established");
        let connection = Self {
            connection,
            rt: ctx.tokio_rt.clone(),
        };
        Ok((connection, producer))
    }
}

impl Drop for MoqConnection {
    fn drop(&mut self) {
        let _guard = self.rt.enter();
        self.connection.abort(moq_net::Error::Cancel);
        info!("MoQ session closed!");
    }
}

/// Validates `url` and creates a client that has not dialed yet, plus an origin for it.
fn create_client(
    ctx: &Arc<PipelineCtx>,
    url: &str,
    reconnect: bool,
) -> Result<(Client, Url, origin::Producer), MoqClientError> {
    let url = Url::parse(url).map_err(|err| MoqClientError::InvalidUrl(Arc::from(url), err))?;
    if !matches!(url.scheme(), "https" | "http") {
        return Err(MoqClientError::InvalidScheme(url.scheme().to_string()));
    }

    let mut config = moq_tokio::connect::Config::default();
    config.once = Some(!reconnect);
    // Zero means retrying forever.
    config.backoff.timeout = Duration::ZERO;
    config.tls.insecure = Some(ctx.moq_disable_tls_verification);

    // Binds the QUIC socket and spawns the origin driver on the pipeline runtime.
    let _guard = ctx.tokio_rt.enter();
    let client = config
        .init(moq_tokio::quic::Config::default())
        .map_err(|err| MoqClientError::ClientInitFailed(format!("{err}")))?;
    let producer = moq_tokio::origin::spawn();
    Ok((client, url, producer))
}
