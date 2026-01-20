use rtmp::{RtmpMediaData, RtmpServer, ServerConfig, server::RtmpConnection};
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
        let receiver = conn.receiver;

        info!(?url_path, "Received stream");
        let url_path_clone = url_path.clone();
        thread::spawn(move || {
            while let Ok(media_data) = receiver.recv() {
                match media_data {
                    RtmpMediaData::VideoConfig(video_config) => {
                        info!(?video_config, "video config")
                    }
                    RtmpMediaData::AudioConfig(audio_config) => {
                        info!(?audio_config, "audio config")
                    }
                    RtmpMediaData::Video(video) => info!(
                        data_len=?video.data.len(),
                        pts=?video.pts,
                        dts=?video.dts,
                        codec=?video.codec,
                        frame_type=?video.frame_type,
                        cts=?video.composition_time,
                        ?url_path,
                        "Received video"
                    ),
                    RtmpMediaData::Audio(audio) => info!(
                        data_len=?audio.data.len(),
                        pts=?audio.pts,
                        dts=?audio.dts,
                        codec=?audio.codec,
                        sound_rate=?audio.sound_rate,
                        channels=?audio.channels,
                        ?url_path,
                        "Received audio"
                    ),
                };
            }
            info!(url_path=?url_path_clone, "Stream connection closed");
        });
    });

    let server = RtmpServer::new(config, on_connection);

    server.run().unwrap();
}
