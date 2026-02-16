use bytes::{BufMut, Bytes, BytesMut};

const START_CODE_3: [u8; 3] = [0, 0, 1];
const START_CODE_4: [u8; 4] = [0, 0, 0, 1];

const NALU_TYPE_SPS: u8 = 7;
const NALU_TYPE_PPS: u8 = 8;

/// Splits Annex B byte stream into individual NALUs (without start codes).
fn split_annexb_nalus(data: &[u8]) -> Vec<&[u8]> {
    let mut nalus = Vec::new();
    let mut i = 0;

    while i < data.len() {
        let nalu_start = if data[i..].starts_with(&START_CODE_4) {
            i + 4
        } else if data[i..].starts_with(&START_CODE_3) {
            i + 3
        } else {
            i += 1;
            continue;
        };

        let mut nalu_end = nalu_start + 1;
        while nalu_end < data.len() {
            if data[nalu_end..].starts_with(&START_CODE_4)
                || data[nalu_end..].starts_with(&START_CODE_3)
            {
                break;
            }
            nalu_end += 1;
        }

        nalus.push(&data[nalu_start..nalu_end]);
        i = nalu_end;
    }

    nalus
}

/// Converts Annex B NALUs to AVCC format (4-byte length prefix per NALU).
/// `data` needs to include whole NALUs
pub(crate) fn annexb_to_avcc(data: &[u8]) -> Bytes {
    let nalus = split_annexb_nalus(data);
    let mut out = BytesMut::new();

    for nalu in &nalus {
        let nalu_type = nalu[0] & 0x1F;
        // Skip SPS/PPS from the data stream - they belong in the config
        if nalu_type == NALU_TYPE_SPS || nalu_type == NALU_TYPE_PPS {
            continue;
        }
        out.put_u32(nalu.len() as u32);
        out.extend_from_slice(nalu);
    }

    out.freeze()
}

/// Builds an AVCDecoderConfigurationRecord from Annex B data containing SPS and PPS.
/// Returns `None` if no SPS or PPS is found.
pub(crate) fn build_avc_decoder_config(data: &[u8]) -> Option<Bytes> {
    let nalus = split_annexb_nalus(data);

    let mut sps_list: Vec<&[u8]> = Vec::new();
    let mut pps_list: Vec<&[u8]> = Vec::new();

    for nalu in &nalus {
        if nalu.is_empty() {
            continue;
        }
        let nalu_type = nalu[0] & 0x1F;
        match nalu_type {
            NALU_TYPE_SPS => sps_list.push(nalu),
            NALU_TYPE_PPS => pps_list.push(nalu),
            _ => {}
        }
    }

    let sps = sps_list.first()?;
    if pps_list.is_empty() {
        return None;
    }

    // AVCDecoderConfigurationRecord structure:
    // - u8  configurationVersion = 1
    // - u8  AVCProfileIndication
    // - u8  profile_compatibility
    // - u8  AVCLevelIndication
    // - u8  lengthSizeMinusOne (0xFC | 3) = 0xFF (4-byte NALU lengths)
    // - u8  numOfSequenceParameterSets (0xE0 | count)
    // - for each SPS: u16 spsLength, sps bytes
    // - u8  numOfPictureParameterSets
    // - for each PPS: u16 ppsLength, pps bytes
    let mut buf = BytesMut::new();
    buf.put_u8(1); // configurationVersion
    buf.put_u8(sps[1]); // AVCProfileIndication
    buf.put_u8(sps[2]); // profile_compatibility
    buf.put_u8(sps[3]); // AVCLevelIndication
    buf.put_u8(0xFF); // lengthSizeMinusOne = 3 (4 bytes)

    buf.put_u8(0xE0 | sps_list.len() as u8);
    for sps in &sps_list {
        buf.put_u16(sps.len() as u16);
        buf.extend_from_slice(sps);
    }

    buf.put_u8(pps_list.len() as u8);
    for pps in &pps_list {
        buf.put_u16(pps.len() as u16);
        buf.extend_from_slice(pps);
    }

    Some(buf.freeze())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_annexb_nalus_with_4byte_start_codes() {
        let data = [0, 0, 0, 1, 0x65, 0xAA, 0xBB, 0, 0, 0, 1, 0x06, 0xCC, 0xDD];
        let nalus = split_annexb_nalus(&data);
        assert_eq!(
            nalus,
            vec![&[0x65, 0xAA, 0xBB][..], &[0x06, 0xCC, 0xDD][..]]
        );
    }

    #[test]
    fn split_annexb_nalus_with_3byte_start_codes() {
        let data = [0, 0, 1, 0x65, 0xAA, 0xBB, 0, 0, 1, 0x06, 0xCC, 0xDD];
        let nalus = split_annexb_nalus(&data);
        assert_eq!(
            nalus,
            vec![&[0x65, 0xAA, 0xBB][..], &[0x06, 0xCC, 0xDD][..]]
        );
    }

    #[test]
    fn split_annexb_nalus_mixed_start_codes() {
        let data = [0, 0, 0, 1, 0x65, 0xAA, 0xBB, 0, 0, 1, 0x06, 0xCC, 0xDD];
        let nalus = split_annexb_nalus(&data);
        assert_eq!(
            nalus,
            vec![&[0x65, 0xAA, 0xBB][..], &[0x06, 0xCC, 0xDD][..]]
        );
    }

    #[test]
    fn annexb_to_avcc_skips_sps_pps() {
        let mut data = Vec::new();
        data.extend_from_slice(&[0, 0, 0, 1, 0x67, 0x42, 0x00, 0x1E]); // SPS
        data.extend_from_slice(&[0, 0, 0, 1, 0x68, 0xCE, 0x38, 0x80]); // PPS
        data.extend_from_slice(&[0, 0, 0, 1, 0x65, 0x88, 0x80]); // IDR

        let result = annexb_to_avcc(&data);
        let expected: &[u8] = &[
            0, 0, 0, 3, // length = 3
            0x65, 0x88, 0x80, // IDR data
        ];
        assert_eq!(&result[..], expected);
    }

    #[test]
    fn annexb_to_avcc_multiple_non_param_nalus() {
        let mut data = Vec::new();
        data.extend_from_slice(&[0, 0, 0, 1, 0x65, 0xAA, 0xBB]); // IDR (type 5)
        data.extend_from_slice(&[0, 0, 0, 1, 0x06, 0xCC, 0xDD]); // SEI (type 6)

        let result = annexb_to_avcc(&data);
        let expected: &[u8] = &[
            0, 0, 0, 3, 0x65, 0xAA, 0xBB, // first NALU
            0, 0, 0, 3, 0x06, 0xCC, 0xDD, // second NALU
        ];
        assert_eq!(&result[..], expected);
    }

    #[test]
    fn build_avc_decoder_config_basic() {
        let mut data = Vec::new();
        // SPS: type=7, profile=0x42, compat=0x00, level=0x1E, extra bytes
        data.extend_from_slice(&[0, 0, 0, 1, 0x67, 0x42, 0x00, 0x1E, 0xDA]);
        // PPS: type=8
        data.extend_from_slice(&[0, 0, 0, 1, 0x68, 0xCE, 0x38, 0x80]);

        let config = build_avc_decoder_config(&data).unwrap();
        assert_eq!(config[0], 1); // configurationVersion
        assert_eq!(config[1], 0x42); // AVCProfileIndication
        assert_eq!(config[2], 0x00); // profile_compatibility
        assert_eq!(config[3], 0x1E); // AVCLevelIndication
        assert_eq!(config[4], 0xFF); // lengthSizeMinusOne

        // numSPS = 0xE0 | 1 = 0xE1
        assert_eq!(config[5], 0xE1);
        // SPS length = 5
        assert_eq!(u16::from_be_bytes([config[6], config[7]]), 5);
        // SPS data
        assert_eq!(&config[8..13], &[0x67, 0x42, 0x00, 0x1E, 0xDA]);

        // numPPS = 1
        assert_eq!(config[13], 1);
        // PPS length = 4
        assert_eq!(u16::from_be_bytes([config[14], config[15]]), 4);
        // PPS data
        assert_eq!(&config[16..20], &[0x68, 0xCE, 0x38, 0x80]);
    }

    #[test]
    fn build_avc_decoder_config_returns_none_without_sps() {
        let data = [0, 0, 0, 1, 0x68, 0xCE, 0x38, 0x80]; // PPS only
        assert!(build_avc_decoder_config(&data).is_none());
    }

    #[test]
    fn build_avc_decoder_config_returns_none_without_pps() {
        let data = [0, 0, 0, 1, 0x67, 0x42, 0x00, 0x1E]; // SPS only
        assert!(build_avc_decoder_config(&data).is_none());
    }
}
