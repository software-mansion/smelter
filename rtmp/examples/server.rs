use rtmp::{RtmpConnection, RtmpEvent, RtmpServer, ServerConfig};
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
        let app = conn.app;
        let stream_key = conn.stream_key;
        let receiver = conn.receiver;

        info!(?app, ?stream_key, "Received stream");
        thread::spawn(move || {
            while let Ok(media_data) = receiver.recv() {
                match media_data {
                    RtmpEvent::H264Config(video_config) => {
                        info!(?video_config, "video config")
                    }
                    RtmpEvent::AacConfig(audio_config) => {
                        info!(?audio_config, "audio config")
                    }
                    RtmpEvent::H264Data(video) => {
                        info!(?video, ?app, ?stream_key, "Received video")
                    }
                    RtmpEvent::AacData(audio) => info!(?audio, ?app, ?stream_key, "Received audio"),
                    RtmpEvent::Metadata(data) => {
                        info!("Metadata received");
                        println!("{data:#?}");
                    }
                    _ => {
                        info!("Raw packets");
                        println!("{media_data:#?}");
                    }
                };
            }
            info!(?app, ?stream_key, "Stream connection closed");
        });
    });

    let _server = RtmpServer::start(config, on_connection).unwrap();
    thread::park()
}
