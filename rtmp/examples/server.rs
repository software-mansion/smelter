use rtmp::{RtmpServer, ServerConfig, server::RtmpConnection};
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

    let on_connection = Box::new(|conn: RtmpConnection| {
        let url_path = conn.url_path;
        let video_rx = conn.video_rx;

        info!(?url_path, "Received stream");
        thread::spawn(move || {
            while let Ok(data) = video_rx.recv() {
                info!(data_len=?data.len(), ?url_path, "Received bytes");
            }
            info!(?url_path, "End of stream");
        });
    });

    let server = RtmpServer::new(config, on_connection);

    server.run().unwrap();
}
