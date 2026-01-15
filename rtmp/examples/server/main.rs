use flv::{AudioTag, VideoTag, tag::PacketType};
use rtmp::{RtmpServer, ServerConfig, server::RtmpConnection};
use std::{
    io::Write,
    process::{Command, Stdio},
    thread,
};
use tracing::info;

use bytes::BytesMut;

use crate::h264_to_annexb::{H264AvcDecoderConfig, H264AvccToAnnexB};

mod aac_to_adts;
mod h264_to_annexb;

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
        let audio_rx = conn.audio_rx;

        info!(?url_path, "Received stream");
        let url_path_clone = url_path.clone();
        thread::spawn(move || {
            let mut ffplay = Command::new("ffplay")
                .args(["-autoexit", "-f", "h264", "-i", "-"])
                .stdin(Stdio::piped())
                .spawn()
                .unwrap();
            let mut ffplay_stdin = ffplay.stdin.take().unwrap();
            let mut bsf = H264AvccToAnnexB::default();
            while let Ok(data) = video_rx.recv() {
                info!(data_len=?data.len(), url_path=?url_path_clone, "Received video bytes");
                let v_tag = VideoTag::parse(data).unwrap();
                match v_tag.packet_type {
                    PacketType::Config => {
                        let avcc_config = H264AvcDecoderConfig::parse(v_tag.data).unwrap();
                        bsf = H264AvccToAnnexB::new(avcc_config);
                    }
                    PacketType::Data => {
                        let annexb_data = bsf.transform(v_tag.data);
                        ffplay_stdin.write_all(&annexb_data).unwrap();
                    }
                }
            }
            info!(url_path=?url_path_clone, "End of video stream");
            drop(ffplay_stdin);
            ffplay.wait().unwrap();
        });

        thread::spawn(move || {
            let mut ffplay = Command::new("ffplay")
                .args(["-autoexit", "-f", "aac", "-i", "-"])
                .stdin(Stdio::piped())
                .spawn()
                .unwrap();
            let mut ffplay_stdin = ffplay.stdin.take().unwrap();

            let mut object_type = 0u8;
            let mut frequency_index = 0u8;
            let mut channel_config = 0u8;
            while let Ok(data) = audio_rx.recv() {
                info!(data_len=?data.len(), ?url_path, "Received audio bytes");
                let a_tag = AudioTag::parse(data).unwrap();
                match a_tag.packet_type {
                    PacketType::Config => {
                        let asc = &a_tag.data;
                        object_type = (asc[0] & 0xF8) >> 3;
                        frequency_index = ((asc[0] & 0x07) << 1) | ((asc[1] & 0x80) >> 7);
                        channel_config = (asc[1] & 0x78) >> 3;
                    }
                    PacketType::Data => {
                        let mut adts_header = [0u8; 7];
                        adts_header[0] = 0xFF; // 8 sync bits
                        adts_header[1] = 0xF0 | 0x01; // 4 sync bits, mpeg version, layer and antipresence bit

                        let adts_object_type = (object_type - 1) & 0x03;
                        let adts_frequency = frequency_index & 0x0F;
                        let adts_channel = channel_config & 0x07;

                        adts_header[2] = (adts_object_type << 6)
                            | (adts_frequency << 2)
                            | ((adts_channel & 0x04) >> 2);

                        let length = ((a_tag.data.len() as u16) + 7u16) & 0x1F_FF;

                        adts_header[3] =
                            (adts_channel & 0x03) << 6 | ((length >> 11) & 0x00_03) as u8;
                        adts_header[4] = ((length & 0x07_F8) >> 3) as u8;
                        adts_header[5] = ((length & 0x00_07) as u8) << 5 | 0x1F;
                        adts_header[6] = 0xFC;

                        let mut adts_data = BytesMut::new();
                        adts_data.extend_from_slice(&adts_header);
                        adts_data.extend_from_slice(&a_tag.data);
                        let adts_data = adts_data.freeze();

                        ffplay_stdin.write_all(&adts_data).unwrap();
                    }
                }
            }

            info!(?url_path, "End of audo stream");
            drop(ffplay_stdin);
            ffplay.wait().unwrap();
        });
    });

    let server = RtmpServer::new(config, on_connection);

    server.run().unwrap();
}
