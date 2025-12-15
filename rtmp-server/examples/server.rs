use rtmp_server::{RtmpServer, ServerConfig, StreamEvent};
use tokio::sync::mpsc;
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let config = ServerConfig {
        port: 1935,
        use_ssl: false,
        cert_file: None,
        key_file: None,
        ca_cert_file: None,
        client_timeout_secs: 5,
    };

    let (tx, mut rx) = mpsc::channel::<StreamEvent>(1000);
    let server = RtmpServer::new(config, Some(tx));

    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            match event {
                StreamEvent::Connected { app, stream_key } => {
                    info!(?app, ?stream_key, "Connected");
                }
                StreamEvent::VideoConfig {
                    codec,
                    timestamp,
                    config_data,
                } => {
                    info!(?codec, ?timestamp, ?config_data, "VideoConfig");
                }
                StreamEvent::Video {
                    codec,
                    pts,
                    dts,
                    is_keyframe,
                    payload,
                } => {
                    info!(?codec, ?pts, ?dts, ?is_keyframe, ?payload, "Video");
                }
                StreamEvent::AudioConfig {
                    codec,
                    timestamp,
                    config_data,
                    sample_rate,
                    channels,
                } => {
                    info!(
                        ?codec,
                        ?timestamp,
                        ?sample_rate,
                        ?channels,
                        ?config_data,
                        "AudioConfig"
                    );
                }
                StreamEvent::Audio {
                    codec,
                    timestamp,
                    payload,
                    sample_rate,
                    channels,
                } => {
                    info!(
                        ?codec,
                        ?timestamp,
                        ?sample_rate,
                        ?channels,
                        ?payload,
                        "Audio"
                    );
                }
                StreamEvent::Metadata { data } => {
                    info!(?data, "Metadata");
                }
                StreamEvent::Disconnected => {
                    info!("Disconnected");
                    break;
                }
            }
        }
    });

    server.run().await?;
    Ok(())
}
