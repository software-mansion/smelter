use bytes::{Buf, Bytes, BytesMut, TryGetError};
use std::io::Read;
use tracing::warn;

use crate::pipeline::decoder::BytestreamTransformer;
use crate::prelude::*;

pub(crate) struct H264AvccToAnnexB {
    config: H264AvcDecoderConfig,
    sps_pps: Bytes,
    /// The decoder needs the parameter sets before the first chunk it decodes,
    /// and again after every discontinuity.
    send_sps_pps: bool,
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
            sps_pps: sps_pps.freeze(),
            send_sps_pps: true,
        }
    }
}

impl BytestreamTransformer for H264AvccToAnnexB {
    /// Repacks data from AVCC to Annex-B
    fn transform(&mut self, chunk_data: bytes::Bytes) -> bytes::Bytes {
        let nalu_length_size = self.config.nalu_length_size;
        let mut data = BytesMut::new();
        if self.send_sps_pps {
            data.extend_from_slice(&self.sps_pps);
            self.send_sps_pps = false;
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
            if reader.read_exact(&mut nalu).is_err() {
                // Truncated/broken input - the declared NAL length exceeds the
                // remaining bytes. Drop the incomplete NAL instead of panicking.
                warn!("Dropping truncated H.264 NAL unit (expected {len} bytes).");
                break;
            }

            data.extend_from_slice(&[0, 0, 0, 1]);
            data.extend_from_slice(&nalu);
        }

        data.freeze()
    }

    fn on_discontinuity(&mut self) {
        self.send_sps_pps = true;
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
        try_split_to(&mut config_bytes, 3)?;

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
        Ok(try_split_to(data, nalu_length)?)
    }
}

/// Takes `len` bytes from the front of `data`. Unlike `Bytes::split_to` it reports the
/// shortage as an error instead of panicking, so a truncated config does not take down
/// the thread that parses it.
fn try_split_to(data: &mut Bytes, len: usize) -> Result<Bytes, TryGetError> {
    if data.len() < len {
        return Err(TryGetError {
            requested: len,
            available: data.len(),
        });
    }
    Ok(data.split_to(len))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal but complete AVCDecoderConfigurationRecord with one SPS and one PPS.
    fn avcc_config() -> Vec<u8> {
        let mut config = vec![
            0x01, 0x42, 0xC0, 0x1E, // version, profile, compatibility, level
            0xFF, // reserved bits + (nalu_length_size - 1) == 4
            0xE1, // reserved bits + number of SPS == 1
        ];
        config.extend_from_slice(&[0x00, 0x04, 0x67, 0x42, 0xC0, 0x1E]); // SPS
        config.extend_from_slice(&[0x01]); // number of PPS
        config.extend_from_slice(&[0x00, 0x03, 0x68, 0xCE, 0x38]); // PPS
        config
    }

    #[test]
    fn parse_complete_avcc_config() {
        let config = H264AvcDecoderConfig::parse(Bytes::from(avcc_config())).unwrap();
        assert_eq!(config.nalu_length_size, 4);
        assert_eq!(
            config.spss,
            vec![Bytes::from_static(&[0x67, 0x42, 0xC0, 0x1E])]
        );
        assert_eq!(config.ppss, vec![Bytes::from_static(&[0x68, 0xCE, 0x38])]);
    }

    #[test]
    fn parse_truncated_avcc_config() {
        let config = avcc_config();
        for len in 1..config.len() {
            let result = H264AvcDecoderConfig::parse(Bytes::copy_from_slice(&config[..len]));
            assert!(
                matches!(result, Err(H264AvcDecoderConfigError::NotEnoughBytes(_))),
                "expected NotEnoughBytes for a config truncated to {len} bytes"
            );
        }
    }

    #[test]
    fn parse_avcc_config_with_nalu_length_past_end() {
        // The SPS announces 16 bytes, but only 2 of them are present.
        let config = [0x01, 0x42, 0xC0, 0x1E, 0xFF, 0xE1, 0x00, 0x10, 0x67, 0x42];
        let result = H264AvcDecoderConfig::parse(Bytes::copy_from_slice(&config));
        assert!(matches!(
            result,
            Err(H264AvcDecoderConfigError::NotEnoughBytes(_))
        ));
    }
}
