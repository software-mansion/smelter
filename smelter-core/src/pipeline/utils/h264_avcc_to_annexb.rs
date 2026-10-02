use bytes::{Buf, Bytes, BytesMut};
use tracing::warn;

use crate::pipeline::decoder::BytestreamTransformer;
use crate::prelude::*;

const NALU_TYPE_IDR: u8 = 5;
const NALU_TYPE_SPS: u8 = 7;
const NALU_TYPE_PPS: u8 = 8;

/// Converts AVCC (length-prefixed) H.264 into Annex B (start-code-prefixed) for the decoders.
///
/// Each chunk passed to `transform` has to contain only whole NAL units; no state is carried
/// between calls, so a NAL unit split across chunks is dropped and the next chunk is misparsed.
/// Parameter sets are inserted only before the first slice of a chunk, so a chunk has to hold
/// exactly one access unit. All current sources (RTMP tags, MP4/HLS samples, MoQ frames) do.
///
/// Parameter sets from avcC are inserted before the first slice after start or a discontinuity,
/// and before every IDR, unless the access unit already carries both SPS and PPS in-band.
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
    fn transform(&mut self, mut chunk_data: bytes::Bytes) -> bytes::Bytes {
        let mut data = BytesMut::with_capacity(chunk_data.len() + self.sps_pps.len());
        let mut has_sps = false;
        let mut has_pps = false;
        let mut first_slice_seen = false;

        // The AVCC NALs are stored as: <length_size bytes long big endian encoded length><the NAL>.
        // we need to convert this into Annex B, in which NALs are separated by [0, 0, 0, 1].
        while let Ok(len) = chunk_data.try_get_uint(self.config.nalu_length_size) {
            let len = len as usize;
            if len > chunk_data.len() {
                // Truncated/broken input - the declared NAL length exceeds the
                // remaining bytes. Drop the incomplete NAL instead of panicking.
                warn!("Dropping truncated H.264 NAL unit (expected {len} bytes).");
                break;
            }
            let nalu = chunk_data.split_to(len);
            let Some(header) = nalu.first() else {
                continue;
            };

            match header & 0x1F {
                NALU_TYPE_SPS => has_sps = true,
                NALU_TYPE_PPS => has_pps = true,
                // Coded slice types (non-IDR, data partitions, IDR).
                nalu_type @ 1..=5 if !first_slice_seen => {
                    first_slice_seen = true;
                    let needs_sps_pps = self.send_sps_pps || nalu_type == NALU_TYPE_IDR;
                    if needs_sps_pps && !(has_sps && has_pps) {
                        data.extend_from_slice(&self.sps_pps);
                    }
                    self.send_sps_pps = false;
                }
                _ => {}
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
    /// Parses an AVCDecoderConfigurationRecord (avcC box body, without the box header).
    /// Profile-specific trailing fields (chroma format, bit depths) are ignored.
    pub fn parse(mut config_bytes: Bytes) -> Result<Self, H264AvcDecoderConfigError> {
        let is_avcc = config_bytes.try_get_u8()? == 0x1;
        if !is_avcc {
            return Err(H264AvcDecoderConfigError::NotAVCC);
        }

        // Skip profile, profile compatibility and level.
        config_bytes.try_get_uint(3)?;

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
        if nalu_length > data.len() {
            return Err(bytes::TryGetError {
                requested: nalu_length,
                available: data.len(),
            }
            .into());
        }
        Ok(data.split_to(nalu_length))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPS: &[u8] = &[0x67, 0x42, 0x00, 0x1E];
    const PPS: &[u8] = &[0x68, 0xCE, 0x38, 0x80];
    const AUD: &[u8] = &[0x09, 0xF0];
    const SEI: &[u8] = &[0x06, 0x05, 0x01];
    const IDR: &[u8] = &[0x65, 0x88, 0x80];
    const P_FRAME: &[u8] = &[0x41, 0x9A, 0x02];

    fn transformer() -> H264AvccToAnnexB {
        H264AvccToAnnexB::new(H264AvcDecoderConfig {
            nalu_length_size: 4,
            spss: vec![Bytes::from_static(SPS)],
            ppss: vec![Bytes::from_static(PPS)],
        })
    }

    fn avcc(nalus: &[&[u8]]) -> Bytes {
        let mut data = BytesMut::new();
        for nalu in nalus {
            data.extend_from_slice(&(nalu.len() as u32).to_be_bytes());
            data.extend_from_slice(nalu);
        }
        data.freeze()
    }

    fn annexb(nalus: &[&[u8]]) -> Bytes {
        let mut data = BytesMut::new();
        for nalu in nalus {
            data.extend_from_slice(&[0, 0, 0, 1]);
            data.extend_from_slice(nalu);
        }
        data.freeze()
    }

    #[test]
    fn first_chunk_gets_sps_pps_even_without_idr() {
        let mut transformer = transformer();
        let result = transformer.transform(avcc(&[P_FRAME]));
        assert_eq!(result, annexb(&[SPS, PPS, P_FRAME]));

        let result = transformer.transform(avcc(&[P_FRAME]));
        assert_eq!(result, annexb(&[P_FRAME]));
    }

    #[test]
    fn every_idr_gets_sps_pps() {
        let mut transformer = transformer();
        transformer.transform(avcc(&[IDR]));
        transformer.transform(avcc(&[P_FRAME]));

        let result = transformer.transform(avcc(&[IDR]));
        assert_eq!(result, annexb(&[SPS, PPS, IDR]));
    }

    #[test]
    fn in_band_sps_pps_are_not_duplicated() {
        let mut transformer = transformer();
        let in_band_sps: &[u8] = &[0x67, 0x64, 0x00, 0x28];
        let in_band_pps: &[u8] = &[0x68, 0xEE, 0x3C, 0x80];

        let result = transformer.transform(avcc(&[in_band_sps, in_band_pps, IDR]));
        assert_eq!(result, annexb(&[in_band_sps, in_band_pps, IDR]));

        // Only SPS in-band is not enough, both are inserted.
        let result = transformer.transform(avcc(&[in_band_sps, IDR]));
        assert_eq!(result, annexb(&[in_band_sps, SPS, PPS, IDR]));
    }

    #[test]
    fn sps_pps_are_inserted_after_aud_and_sei() {
        let mut transformer = transformer();
        let result = transformer.transform(avcc(&[AUD, SEI, IDR]));
        assert_eq!(result, annexb(&[AUD, SEI, SPS, PPS, IDR]));
    }

    #[test]
    fn sps_pps_are_resent_after_discontinuity() {
        let mut transformer = transformer();
        transformer.transform(avcc(&[IDR]));
        transformer.transform(avcc(&[P_FRAME]));
        transformer.on_discontinuity();

        let result = transformer.transform(avcc(&[P_FRAME]));
        assert_eq!(result, annexb(&[SPS, PPS, P_FRAME]));
    }

    #[test]
    fn chunk_without_slice_defers_sps_pps() {
        let mut transformer = transformer();
        let result = transformer.transform(avcc(&[SEI]));
        assert_eq!(result, annexb(&[SEI]));

        let result = transformer.transform(avcc(&[P_FRAME]));
        assert_eq!(result, annexb(&[SPS, PPS, P_FRAME]));
    }

    #[test]
    fn only_first_slice_in_chunk_triggers_insertion() {
        let mut transformer = transformer();
        transformer.transform(avcc(&[IDR]));

        let second_idr_slice: &[u8] = &[0x65, 0x44, 0x01];
        let result = transformer.transform(avcc(&[IDR, second_idr_slice]));
        assert_eq!(result, annexb(&[SPS, PPS, IDR, second_idr_slice]));
    }

    #[test]
    fn empty_nalus_are_skipped() {
        let mut transformer = transformer();
        let result = transformer.transform(avcc(&[&[], IDR, &[]]));
        assert_eq!(result, annexb(&[SPS, PPS, IDR]));
    }

    #[test]
    fn truncated_nalu_is_dropped() {
        let mut transformer = transformer();
        let mut data = BytesMut::from(&avcc(&[IDR])[..]);
        data.extend_from_slice(&[0, 0, 0, 10, 0x41, 0x9A]);

        let result = transformer.transform(data.freeze());
        assert_eq!(result, annexb(&[SPS, PPS, IDR]));
    }

    #[test]
    fn huge_nalu_length_is_dropped() {
        let mut transformer = transformer();
        let result = transformer.transform(Bytes::from_static(&[0xFF, 0xFF, 0xFF, 0xF0, 0x65]));
        assert!(result.is_empty());
    }

    #[test]
    fn truncated_length_prefix_is_dropped() {
        let mut transformer = transformer();
        let mut data = BytesMut::from(&avcc(&[IDR])[..]);
        data.extend_from_slice(&[0, 0]);

        let result = transformer.transform(data.freeze());
        assert_eq!(result, annexb(&[SPS, PPS, IDR]));
    }

    #[test]
    fn two_byte_nalu_length() {
        let mut transformer = H264AvccToAnnexB::new(H264AvcDecoderConfig {
            nalu_length_size: 2,
            spss: vec![Bytes::from_static(SPS)],
            ppss: vec![Bytes::from_static(PPS)],
        });
        let data = Bytes::from_static(&[0, 3, 0x65, 0x88, 0x80, 0, 3, 0x41, 0x9A, 0x02]);

        let result = transformer.transform(data);
        assert_eq!(result, annexb(&[SPS, PPS, IDR, P_FRAME]));
    }

    #[test]
    fn parse_config() {
        let config = Bytes::from_static(&[
            0x01, 0x42, 0x00, 0x1E, 0xFF, // version, profile, compat, level, length size
            0xE1, 0x00, 0x04, 0x67, 0x42, 0x00, 0x1E, // 1 SPS
            0x01, 0x00, 0x04, 0x68, 0xCE, 0x38, 0x80, // 1 PPS
        ]);

        let config = H264AvcDecoderConfig::parse(config).unwrap();
        assert_eq!(config.nalu_length_size, 4);
        assert_eq!(config.spss, vec![Bytes::from_static(SPS)]);
        assert_eq!(config.ppss, vec![Bytes::from_static(PPS)]);
    }

    #[test]
    fn parse_config_rejects_non_avcc() {
        let config = Bytes::from_static(&[0x00, 0x00, 0x00, 0x01, 0x67]);
        assert!(matches!(
            H264AvcDecoderConfig::parse(config),
            Err(H264AvcDecoderConfigError::NotAVCC)
        ));
    }

    #[test]
    fn parse_config_rejects_short_header() {
        let config = Bytes::from_static(&[0x01, 0x42]);
        assert!(matches!(
            H264AvcDecoderConfig::parse(config),
            Err(H264AvcDecoderConfigError::NotEnoughBytes(_))
        ));
    }

    #[test]
    fn parse_config_rejects_sps_length_past_end() {
        let config = Bytes::from_static(&[0x01, 0x42, 0x00, 0x1E, 0xFF, 0xE1, 0xFF, 0xFF, 0x67]);
        assert!(matches!(
            H264AvcDecoderConfig::parse(config),
            Err(H264AvcDecoderConfigError::NotEnoughBytes(_))
        ));
    }

    #[test]
    fn parse_config_rejects_pps_length_past_end() {
        let config = Bytes::from_static(&[
            0x01, 0x42, 0x00, 0x1E, 0xFF, // header
            0xE1, 0x00, 0x04, 0x67, 0x42, 0x00, 0x1E, // 1 SPS
            0x01, 0x00, 0x10, 0x68, // 1 PPS, truncated
        ]);
        assert!(matches!(
            H264AvcDecoderConfig::parse(config),
            Err(H264AvcDecoderConfigError::NotEnoughBytes(_))
        ));
    }
}
