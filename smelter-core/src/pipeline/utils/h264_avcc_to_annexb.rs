use std::slice;

use bytes::{Buf, Bytes, BytesMut};
use tracing::warn;

use crate::pipeline::decoder::BytestreamTransformer;
use crate::prelude::*;

const NALU_TYPE_IDR: u8 = 5;
const NALU_TYPE_SPS: u8 = 7;
const NALU_TYPE_PPS: u8 = 8;
const NALU_TYPE_AUD: u8 = 9;

/// Converts AVCC (length-prefixed) H.264 into Annex B (start-code-prefixed) for the decoders.
///
/// Each chunk passed to `transform` has to contain only whole NAL units; no state is carried
/// between calls, so a NAL unit split across chunks is dropped and the next chunk is misparsed.
/// Parameter sets are inserted only into the access unit of the first slice in a chunk, so a
/// chunk has to hold exactly one access unit. All current sources (RTMP tags, MP4/HLS samples,
/// MoQ frames) do.
///
/// Before the first slice after start or a discontinuity, and before every IDR, the latest known
/// SPS and PPS are inserted, skipping the ones the access unit already carries in-band. The
/// latest SPS/PPS come from avcC and are replaced by every in-band one, so a stream that changes
/// its parameters is not reverted to the avcC ones.
pub(crate) struct H264AvccToAnnexB {
    nalu_length_size: usize,
    /// Latest SPS, in Annex B.
    sps: Bytes,
    /// Latest PPS, in Annex B.
    pps: Bytes,
    /// Decoding can start at a non-IDR picture (recovery point, intra refresh), so parameter sets
    /// also go before the first picture after start or a discontinuity.
    send_sps_pps: bool,
}

impl H264AvccToAnnexB {
    pub fn new(config: H264AvcDecoderConfig) -> Self {
        Self {
            nalu_length_size: config.nalu_length_size,
            sps: to_annexb(&config.spss),
            pps: to_annexb(&config.ppss),
            send_sps_pps: true,
        }
    }

    /// Splits AVCC data into non-empty NAL units.
    fn split_nalus(&self, mut chunk_data: Bytes) -> Vec<Bytes> {
        let mut nalus = Vec::new();
        // The AVCC NALs are stored as: <length_size bytes long big endian encoded length><the NAL>.
        while let Ok(len) = chunk_data.try_get_uint(self.nalu_length_size) {
            let len = len as usize;
            if len > chunk_data.len() {
                // Truncated/broken input - the declared NAL length exceeds the
                // remaining bytes. Drop the incomplete NAL instead of panicking.
                warn!("Dropping truncated H.264 NAL unit (expected {len} bytes).");
                break;
            }
            let nalu = chunk_data.split_to(len);
            if !nalu.is_empty() {
                nalus.push(nalu);
            }
        }
        nalus
    }
}

impl BytestreamTransformer for H264AvccToAnnexB {
    /// Repacks data from AVCC to Annex-B
    fn transform(&mut self, chunk_data: bytes::Bytes) -> bytes::Bytes {
        let nalus = self.split_nalus(chunk_data);
        let nalu_type = |nalu: &Bytes| nalu[0] & 0x1F;

        // Coded slice types (non-IDR, data partitions, IDR).
        let first_slice = nalus
            .iter()
            .position(|nalu| matches!(nalu_type(nalu), 1..=5));

        // Whether the first slice needs SPS/PPS before it (first picture after start/discontinuity,
        // or IDR).
        let should_have_sps_pps =
            first_slice.is_some_and(|i| self.send_sps_pps || nalu_type(&nalus[i]) == NALU_TYPE_IDR);

        // Only parameter sets before the first slice can be used to decode it.
        let before_first_slice = &nalus[..first_slice.unwrap_or(0)];
        let has_sps = before_first_slice
            .iter()
            .any(|nalu| nalu_type(nalu) == NALU_TYPE_SPS);
        let has_pps = before_first_slice
            .iter()
            .any(|nalu| nalu_type(nalu) == NALU_TYPE_PPS);
        let insert_sps = should_have_sps_pps && !has_sps;
        let insert_pps = should_have_sps_pps && !has_pps;

        // SPS goes at the start of the access unit (after AUD), so it precedes any in-band PPS.
        // PPS goes right before the first slice, so it follows any in-band SPS.
        let sps_position = nalus
            .iter()
            .take_while(|nalu| nalu_type(nalu) == NALU_TYPE_AUD)
            .count();

        let nalus_len: usize = nalus.iter().map(|nalu| nalu.len() + 4).sum();
        let mut data = BytesMut::with_capacity(nalus_len + self.sps.len() + self.pps.len());
        for (i, nalu) in nalus.iter().enumerate() {
            if insert_sps && i == sps_position {
                data.extend_from_slice(&self.sps);
            }
            if insert_pps && Some(i) == first_slice {
                data.extend_from_slice(&self.pps);
            }
            data.extend_from_slice(&[0, 0, 0, 1]);
            data.extend_from_slice(nalu);
        }

        // Remember in-band SPS/PPS, so later insertions use the stream's current parameters.
        for nalu in &nalus {
            match nalu_type(nalu) {
                NALU_TYPE_SPS => self.sps = to_annexb(slice::from_ref(nalu)),
                NALU_TYPE_PPS => self.pps = to_annexb(slice::from_ref(nalu)),
                _ => {}
            }
        }

        if first_slice.is_some() {
            // If there was a slice (regardless if IDR or not) that means we either already
            // added SPS/PPS or they were already in-band
            self.send_sps_pps = false;
        }

        data.freeze()
    }

    fn on_discontinuity(&mut self) {
        self.send_sps_pps = true;
    }
}

/// Copies NAL units into a new Annex B buffer.
fn to_annexb(nalus: &[Bytes]) -> Bytes {
    let mut data = BytesMut::new();
    for nalu in nalus {
        data.extend_from_slice(&[0, 0, 0, 1]);
        data.extend_from_slice(nalu);
    }
    data.freeze()
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
    const NEW_SPS: &[u8] = &[0x67, 0x64, 0x00, 0x28];
    const NEW_PPS: &[u8] = &[0x68, 0xEE, 0x3C, 0x80];
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
        let result = transformer.transform(avcc(&[NEW_SPS, NEW_PPS, IDR]));
        assert_eq!(result, annexb(&[NEW_SPS, NEW_PPS, IDR]));
    }

    #[test]
    fn only_missing_parameter_set_is_inserted() {
        let result = transformer().transform(avcc(&[NEW_PPS, IDR]));
        assert_eq!(result, annexb(&[SPS, NEW_PPS, IDR]));

        let result = transformer().transform(avcc(&[NEW_SPS, IDR]));
        assert_eq!(result, annexb(&[NEW_SPS, PPS, IDR]));
    }

    #[test]
    fn in_band_parameter_sets_replace_stored_ones() {
        let mut transformer = transformer();
        transformer.transform(avcc(&[NEW_SPS, NEW_PPS, IDR]));
        transformer.transform(avcc(&[P_FRAME]));

        let result = transformer.transform(avcc(&[IDR]));
        assert_eq!(result, annexb(&[NEW_SPS, NEW_PPS, IDR]));

        transformer.on_discontinuity();
        let result = transformer.transform(avcc(&[P_FRAME]));
        assert_eq!(result, annexb(&[NEW_SPS, NEW_PPS, P_FRAME]));
    }

    #[test]
    fn parameter_sets_after_slice_do_not_prevent_insertion() {
        let mut transformer = transformer();
        let result = transformer.transform(avcc(&[IDR, NEW_SPS, NEW_PPS]));
        assert_eq!(result, annexb(&[SPS, PPS, IDR, NEW_SPS, NEW_PPS]));

        let result = transformer.transform(avcc(&[IDR]));
        assert_eq!(result, annexb(&[NEW_SPS, NEW_PPS, IDR]));
    }

    #[test]
    fn single_in_band_parameter_set_replaces_only_its_type() {
        let mut transformer = transformer();
        transformer.transform(avcc(&[NEW_PPS, IDR]));

        let result = transformer.transform(avcc(&[IDR]));
        assert_eq!(result, annexb(&[SPS, NEW_PPS, IDR]));
    }

    #[test]
    fn sps_is_inserted_after_aud_and_pps_before_slice() {
        let mut transformer = transformer();
        let result = transformer.transform(avcc(&[AUD, SEI, IDR]));
        assert_eq!(result, annexb(&[AUD, SPS, SEI, PPS, IDR]));
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
