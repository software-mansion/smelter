use bytes::{BufMut, Bytes, BytesMut};
use h264_reader::{
    nal::{
        Nal, RefNal,
        sps::{ChromaFormat, ChromaInfo, ProfileIdc},
    },
    rbsp::BitRead,
};
use memchr::memmem;

const START_CODE: [u8; 3] = [0, 0, 1];

const NALU_TYPE_SPS: u8 = 7;
const NALU_TYPE_PPS: u8 = 8;

/// Splits Annex B byte stream into individual NALUs (without start codes and trailing zero
/// bytes). Empty NALUs are skipped.
fn split_annexb_nalus(data: &[u8]) -> Vec<&[u8]> {
    let mut nalus = Vec::new();
    let mut start_codes = memmem::find_iter(data, &START_CODE).peekable();

    while let Some(start_code) = start_codes.next() {
        let end = start_codes.peek().copied().unwrap_or(data.len());
        let nalu = &data[start_code + START_CODE.len()..end];

        // NAL unit never ends with 0x00, so zeros before the next start code are either the
        // `zero_byte` of a 4-byte start code or `trailing_zero_8bits` padding.
        let Some(last_byte) = nalu.iter().rposition(|byte| *byte != 0) else {
            continue;
        };
        nalus.push(&nalu[..=last_byte]);
    }

    nalus
}

/// Converts Annex B to AVCC with 4-byte length prefixes, matching `build_avc_decoder_config`.
///
/// `data` has to contain only whole NAL units, starting with a start code; bytes before the first
/// start code are dropped and no state is carried between calls. Any number of NAL units is
/// accepted; encoders pass one access unit per call. SPS/PPS are dropped, so they have to be
/// delivered out-of-band (via `build_avc_decoder_config`) and must not change mid-stream.
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
/// Returns `None` if no SPS or PPS is found, or if the first SPS can't be parsed.
///
/// Profile and level come from the first SPS; all SPS/PPS are included and other NAL units are
/// ignored. Declares 4-byte NAL length prefixes, as produced by `annexb_to_avcc`.
pub(crate) fn build_avc_decoder_config(data: &[u8]) -> Option<Bytes> {
    let nalus = split_annexb_nalus(data);

    let mut sps_list: Vec<&[u8]> = Vec::new();
    let mut pps_list: Vec<&[u8]> = Vec::new();

    for nalu in &nalus {
        match nalu[0] & 0x1F {
            NALU_TYPE_SPS => sps_list.push(nalu),
            NALU_TYPE_PPS => pps_list.push(nalu),
            _ => {}
        }
    }

    let first_sps = *sps_list.first()?;
    let &[_, profile, compatibility, level, ..] = first_sps else {
        return None;
    };
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
    // - for profiles other than Baseline (66), Main (77) and Extended (88):
    //   - u8  chroma_format (0xFC | chroma_format_idc)
    //   - u8  bit_depth_luma (0xF8 | bit_depth_luma_minus8)
    //   - u8  bit_depth_chroma (0xF8 | bit_depth_chroma_minus8)
    //   - u8  numOfSequenceParameterSetExt
    let mut buf = BytesMut::new();
    buf.put_u8(1); // configurationVersion
    buf.put_u8(profile); // AVCProfileIndication
    buf.put_u8(compatibility); // profile_compatibility
    buf.put_u8(level); // AVCLevelIndication
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

    if !matches!(profile, 66 | 77 | 88) {
        let chroma_info = read_chroma_info(first_sps)?;
        let chroma_format_idc = match chroma_info.chroma_format {
            ChromaFormat::Monochrome => 0,
            ChromaFormat::YUV420 => 1,
            ChromaFormat::YUV422 => 2,
            ChromaFormat::YUV444 => 3,
            ChromaFormat::Invalid(_) => return None,
        };
        buf.put_u8(0xFC | chroma_format_idc);
        buf.put_u8(0xF8 | chroma_info.bit_depth_luma_minus8);
        buf.put_u8(0xF8 | chroma_info.bit_depth_chroma_minus8);
        buf.put_u8(0); // numOfSequenceParameterSetExt
    }

    Some(buf.freeze())
}

/// Reads chroma format and bit depths from SPS (defaults when profile does not signal them).
fn read_chroma_info(sps: &[u8]) -> Option<ChromaInfo> {
    let mut reader = RefNal::new(sps, &[], true).rbsp_bits();
    let profile_idc = ProfileIdc::from(reader.read::<u8>(8, "profile_idc").ok()?);
    reader.skip(16, "constraint_flags + level_idc").ok()?;
    reader.read_ue("seq_parameter_set_id").ok()?;
    ChromaInfo::read(&mut reader, profile_idc).ok()
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

    #[test]
    fn split_annexb_nalus_trailing_start_code() {
        let data = [0, 0, 0, 1, 0x65, 0xAA, 0, 0, 1];
        assert_eq!(split_annexb_nalus(&data), vec![&[0x65, 0xAA][..]]);

        let data = [0, 0, 0, 1, 0x65, 0xAA, 0, 0, 0, 1];
        assert_eq!(split_annexb_nalus(&data), vec![&[0x65, 0xAA][..]]);
    }

    #[test]
    fn split_annexb_nalus_consecutive_start_codes() {
        let data = [0, 0, 1, 0, 0, 1, 0x65, 0xAA];
        assert_eq!(split_annexb_nalus(&data), vec![&[0x65, 0xAA][..]]);

        let data = [0, 0, 0, 1, 0, 0, 0, 1, 0x65, 0xAA];
        assert_eq!(split_annexb_nalus(&data), vec![&[0x65, 0xAA][..]]);
    }

    #[test]
    fn split_annexb_nalus_ignores_leading_garbage() {
        let data = [0xFF, 0x12, 0, 0, 1, 0x65, 0xAA];
        assert_eq!(split_annexb_nalus(&data), vec![&[0x65, 0xAA][..]]);
    }

    #[test]
    fn split_annexb_nalus_without_start_code() {
        assert!(split_annexb_nalus(&[]).is_empty());
        assert!(split_annexb_nalus(&[0x65, 0xAA, 0xBB]).is_empty());
        assert!(split_annexb_nalus(&[0, 0, 1]).is_empty());
    }

    #[test]
    fn build_avc_decoder_config_returns_none_for_short_sps() {
        let mut data = Vec::new();
        data.extend_from_slice(&[0, 0, 0, 1, 0x67, 0x42]); // SPS, too short
        data.extend_from_slice(&[0, 0, 0, 1, 0x68, 0xCE, 0x38, 0x80]); // PPS
        assert!(build_avc_decoder_config(&data).is_none());
    }

    #[test]
    fn split_annexb_nalus_strips_trailing_zeros() {
        let data = [0, 0, 1, 0x65, 0xAA, 0, 0, 0, 0, 1, 0x06, 0xCC, 0, 0];
        assert_eq!(
            split_annexb_nalus(&data),
            vec![&[0x65, 0xAA][..], &[0x06, 0xCC][..]]
        );
    }

    #[test]
    fn build_avc_decoder_config_high_profile_420_8bit() {
        let mut data = Vec::new();
        // SPS: High (100), level 3.1, sps_id 0, chroma_format_idc 1, bit depths 8
        data.extend_from_slice(&[0, 0, 0, 1, 0x67, 0x64, 0x00, 0x1F, 0xAC]);
        data.extend_from_slice(&[0, 0, 0, 1, 0x68, 0xEE, 0x3C, 0x80]); // PPS

        let config = build_avc_decoder_config(&data).unwrap();
        assert_eq!(config[1], 0x64); // AVCProfileIndication
        assert_eq!(&config[config.len() - 4..], &[0xFD, 0xF8, 0xF8, 0x00]);
    }

    #[test]
    fn build_avc_decoder_config_high_422_profile_10bit() {
        let mut data = Vec::new();
        // SPS: High 4:2:2 (122), sps_id 0, chroma_format_idc 2, bit depths 10
        data.extend_from_slice(&[0, 0, 0, 1, 0x67, 0x7A, 0x00, 0x1F, 0xB6, 0xC0]);
        data.extend_from_slice(&[0, 0, 0, 1, 0x68, 0xEE, 0x3C, 0x80]); // PPS

        let config = build_avc_decoder_config(&data).unwrap();
        assert_eq!(&config[config.len() - 4..], &[0xFE, 0xFA, 0xFA, 0x00]);
    }

    #[test]
    fn build_avc_decoder_config_high_444_profile() {
        let mut data = Vec::new();
        // SPS: High 4:4:4 (244), sps_id 0, chroma_format_idc 3, bit depths 8
        data.extend_from_slice(&[0, 0, 0, 1, 0x67, 0xF4, 0x00, 0x1F, 0x91, 0x80]);
        data.extend_from_slice(&[0, 0, 0, 1, 0x68, 0xEE, 0x3C, 0x80]); // PPS

        let config = build_avc_decoder_config(&data).unwrap();
        assert_eq!(&config[config.len() - 4..], &[0xFF, 0xF8, 0xF8, 0x00]);
    }

    #[test]
    fn build_avc_decoder_config_baseline_has_no_extension() {
        let sps = [0x67, 0x42, 0x00, 0x1E, 0xDA];
        let pps = [0x68, 0xCE, 0x38, 0x80];
        let mut data = Vec::new();
        data.extend_from_slice(&[0, 0, 0, 1]);
        data.extend_from_slice(&sps);
        data.extend_from_slice(&[0, 0, 0, 1]);
        data.extend_from_slice(&pps);

        let config = build_avc_decoder_config(&data).unwrap();
        // header (6) + SPS (2 + len) + PPS count (1) + PPS (2 + len)
        assert_eq!(config.len(), 6 + 2 + sps.len() + 1 + 2 + pps.len());
    }
}
