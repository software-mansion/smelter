use flv::parser::RtmpParser;
use rtmp::{RtmpServer, ServerConfig, server::RtmpConnection};
use std::{fs, thread};
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
            let mut flv_parser = RtmpParser::new();
            while let Ok(data) = video_rx.recv() {
                info!(data_len=?data.len(), url_path=?url_path_clone, "Received video bytes");
                flv_parser.parse_video(data).unwrap();
            }
            let video_config = flv_parser.video_config().clone().unwrap();
            let video_data = flv_parser.video();

            let decoder_config = H264AvcDecoderConfig::parse(video_config.data).unwrap();
            let mut bsf = H264AvccToAnnexB::new(decoder_config);

            let mut annexb_data = BytesMut::new();
            for tag in video_data {
                let annexb_bytes = bsf.transform(tag.data);
                annexb_data.extend_from_slice(&annexb_bytes);
            }
            let annexb_data = annexb_data.freeze();

            fs::write("./test.h264", &annexb_data).unwrap();

            info!(url_path=?url_path_clone, "End of video stream");
        });

        thread::spawn(move || {
            while let Ok(data) = audio_rx.recv() {
                info!(data_len=?data.len(), ?url_path, "Received audio bytes");
            }
            info!(?url_path, "End of audo stream");
        });
    });

    let server = RtmpServer::new(config, on_connection);

    server.run().unwrap();
}

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

#[derive(Debug, Clone)]
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
