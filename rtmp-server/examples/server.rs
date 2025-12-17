use rtmp_server::{RtmpServer, ServerConfig};
use std::thread;
use tracing::info;

fn main() {
    tracing_subscriber::fmt::init();

    let config = ServerConfig {
        port: 1935,
        use_ssl: false,
        cert_file: None,
        key_file: None,
        ca_cert_file: None,
        client_timeout_secs: 30,
    };

    let mut server = RtmpServer::new(config);

    server.on_connection(|conn, video_rx, _audio_rx| {
        if conn.app == "app" && conn.stream_key == "stream_key" {
            let stream_key = conn.stream_key.clone();
            info!(?stream_key, "Received stream");

            thread::spawn(move || {
                while let Ok(data) = video_rx.recv() {
                    info!(data_len=?data.len(), ?stream_key, "Received bytes");
                }
                info!(?stream_key, "End of stream");
            });

            return true;
        }

        false
    });

    server.run().unwrap();
}
