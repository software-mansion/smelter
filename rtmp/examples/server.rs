use rtmp::{RtmpServer, RtmpStreamData, ServerConfig, server::RtmpConnection};
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
                    RtmpStreamData::VideoConfig(video_config) => {
                        info!(?video_config, "video config")
                    }
                    RtmpStreamData::AudioConfig(audio_config) => {
                        info!(?audio_config, "audio config")
                    }
                    RtmpStreamData::Video(video) => info!(
                        data_len=?video.data.len(),
                        pts=?video.pts,
                        dts=?video.dts,
                        codec=?video.codec,
                        frame_type=?video.frame_type,
                        cts=?video.composition_time,
                        ?app,
                        ?stream_key,
                        "Received video"
                    ),
                    RtmpStreamData::Audio(audio) => info!(
                        data_len=?audio.data.len(),
                        pts=?audio.pts,
                        dts=?audio.dts,
                        codec=?audio.codec,
                        sound_rate=?audio.sound_rate,
                        channels=?audio.channels,
                        ?app,
                        ?stream_key,
                        "Received audio"
                    ),
                    RtmpStreamData::Metadata(data) => {
                        info!("Metadata received");
                        println!("{data:#?}");
                    }
                };
            }
            info!(?app, ?stream_key, "Stream connection closed");
        });
    });

    let _server = RtmpServer::start(config, on_connection).unwrap();
    thread::park()
}
