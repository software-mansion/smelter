use flv::{AudioTag, VideoTag, tag::PacketType};
use rtmp::{RtmpServer, ServerConfig, server::RtmpConnection};
use std::{
    io::Write,
    process::{Command, Stdio},
    thread,
};
use tracing::info;

use bytes::{Buf, Bytes, BytesMut};
use std::io::Read;

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

#[derive(Default)]
struct H264AvccToAnnexB {
    config: H264AvcDecoderConfig,
    sps_pps: Option<Bytes>,
}

impl H264AvccToAnnexB {
    fn new(config: H264AvcDecoderConfig) -> Self {
        let mut sps_pps = BytesMut::new();
        sps_pps.extend(
            config
                .spss
                .iter()
                .flat_map(|sps| [0, 0, 0, 1].iter().chain(sps)),
        );
        sps_pps.extend(
            config
                .ppss
                .iter()
                .flat_map(|pps| [0, 0, 0, 1].iter().chain(pps)),
        );

        Self {
            config,
            sps_pps: Some(sps_pps.freeze()),
        }
    }

    /// Repacks data from AVCC to Annex-B
    fn transform(&mut self, chunk_data: bytes::Bytes) -> bytes::Bytes {
        let nalu_length_size = self.config.nalu_length_size;
        let mut data = BytesMut::new();
        if let Some(sps_pps) = self.sps_pps.take() {
            data.extend_from_slice(&sps_pps);
        }

        let mut reader = chunk_data.reader();

        // The AVCC NALs are stored as: <length_size bytes long big endian encoded length><the NAL>.
        // we need to convert this into Annex B, in which NALs are separated by
        // [0, 0, 0, 1]. `nalu_length_size` is at most 4 bytes long.
        loop {
            let mut len = [0u8; 4];

            if reader.read_exact(&mut len[4 - nalu_length_size..]).is_err() {
                break;
            }

            let len = u32::from_be_bytes(len);

            let mut nalu = BytesMut::zeroed(len as usize);
            reader.read_exact(&mut nalu).unwrap();

            data.extend_from_slice(&[0, 0, 0, 1]);
            data.extend_from_slice(&nalu);
        }

        data.freeze()
    }
}

#[derive(Debug, Clone, Default)]
struct H264AvcDecoderConfig {
    nalu_length_size: usize,
    spss: Vec<Bytes>,
    ppss: Vec<Bytes>,
}

impl H264AvcDecoderConfig {
    fn parse(mut config_bytes: Bytes) -> Result<Self, H264AvcDecoderConfigError> {
        let is_avcc = config_bytes.try_get_u8()? == 0x1;
        if !is_avcc {
            return Err(H264AvcDecoderConfigError::NotAVCC);
        }

        // Skip not needed information
        config_bytes = config_bytes.slice(3..);

        let nalu_length_size = (config_bytes.try_get_u8()? & 3) as usize + 1;

        let sps_num = config_bytes.try_get_u8()? & 0x1F;
        let spss = (0..sps_num)
            .map(|_| Self::parse_nalu(&mut config_bytes))
            .collect::<Result<_, _>>()?;

        let pps_num = config_bytes.try_get_u8()?;
        let ppss = (0..pps_num)
            .map(|_| Self::parse_nalu(&mut config_bytes))
            .collect::<Result<_, _>>()?;

        Ok(Self {
            nalu_length_size,
            spss,
            ppss,
        })
    }

    fn parse_nalu(data: &mut Bytes) -> Result<Bytes, H264AvcDecoderConfigError> {
        let nalu_length = data.try_get_u16()? as usize;
        let contents = data.slice(0..nalu_length);
        *data = data.slice(nalu_length..);
        Ok(contents)
    }
}

#[derive(Debug, thiserror::Error)]
enum H264AvcDecoderConfigError {
    #[error("Incorrect AVCDecoderConfig. Expected more bytes.")]
    NotEnoughBytes(#[from] bytes::TryGetError),

    #[error("Not AVCC")]
    NotAVCC,
}
