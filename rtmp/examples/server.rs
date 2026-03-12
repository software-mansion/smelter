use rtmp::{RtmpEvent, RtmpServer, RtmpServerConfig, RtmpServerConnection};
use std::thread;
use tracing::info;

fn main() {
    tracing_subscriber::fmt::init();

    let config = RtmpServerConfig {
        port: 1935,
        tls: None,
    };

    let on_connection = Box::new(|conn: RtmpServerConnection| {
        let app = conn.app().to_string();
        let stream_key = conn.stream_key().to_string();

        info!(?app, ?stream_key, "Received stream");
        thread::spawn(move || {
            for media_data in &conn {
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
