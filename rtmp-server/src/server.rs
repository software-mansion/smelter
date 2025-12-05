use crate::{error::RtmpError, handshake::Handshake};
use std::{net::SocketAddr, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpListener,
    time::timeout,
};
use tracing::{error, info};

#[allow(dead_code)] // TODO add SSL/TLS
pub struct ServerConfig {
    pub port: u16,
    pub use_ssl: bool,
    pub cert_file: Option<String>,
    pub key_file: Option<String>,
    pub ca_cert_file: Option<String>,
    pub client_timeout_secs: u64,
}

pub struct RtmpServer {
    config: ServerConfig,
}

impl RtmpServer {
    pub fn new(config: ServerConfig) -> Self {
        Self { config }
    }

    pub async fn run(&self) -> Result<(), RtmpError> {
        let addr = SocketAddr::from(([0, 0, 0, 0], self.config.port));
        let listener = TcpListener::bind(addr).await?;

        info!("RTMP server listening on port {}", self.config.port);

        loop {
            match listener.accept().await {
                Ok((stream, peer_addr)) => {
                    info!("New connection from: {}", peer_addr);

                    let client_timeout = self.config.client_timeout_secs;

                    tokio::spawn(async move {
                        // use stream or tls_stream based on use_ssl field
                        let result = handle_client(stream, client_timeout).await;

                        if let Err(error) = result {
                            error!(?error, "Client handler error");
                        }
                    });
                }
                Err(error) => {
                    error!(?error, "Accept error");
                }
            }
        }
    }
}

async fn handle_client<S>(mut stream: S, timeout_secs: u64) -> Result<(), RtmpError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let timeout_duration = Duration::from_secs(timeout_secs);

    timeout(timeout_duration, Handshake::perform(&mut stream))
        .await
        .map_err(|_| RtmpError::Timeout)??;

    info!("RTMP handshake completed");

    // handle RTMP messages
    // from chunk read `app`` and `stream_key``

    Ok(())
}
