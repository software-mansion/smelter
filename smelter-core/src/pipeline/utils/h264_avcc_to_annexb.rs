use bytes::{Buf, Bytes, BytesMut};
use std::io::Read;

use crate::pipeline::decoder::BytestreamTransformer;
use crate::prelude::*;

pub(crate) struct H264AvccToAnnexB {
    config: H264AvcDecoderConfig,
    sps_pps: Option<Bytes>,
}

impl H264AvccToAnnexB {
    pub fn new(config: H264AvcDecoderConfig) -> Self {
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
}

impl BytestreamTransformer for H264AvccToAnnexB {
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
pub(crate) struct H264AvcDecoderConfig {
    pub nalu_length_size: usize,
    pub spss: Vec<Bytes>,
    pub ppss: Vec<Bytes>,
}

impl H264AvcDecoderConfig {
    pub fn parse(mut config_bytes: Bytes) -> Result<Self, H264AvcDecoderConfigError> {
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
