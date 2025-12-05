use rtmp_server::{RtmpServer, ServerConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let config = ServerConfig {
        port: 1935,
        use_ssl: false,
        certfile: None,
        keyfile: None,
        cacertfile: None,
        client_timeout_secs: 5,
    };

    let server = RtmpServer::new(config);

    server.run().await?;

    Ok(())
}
