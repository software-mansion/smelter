use bytes::{BufMut, Bytes, BytesMut};

const START_CODE_3: [u8; 3] = [0, 0, 1];
const START_CODE_4: [u8; 4] = [0, 0, 0, 1];

const NALU_TYPE_SPS: u8 = 7;
const NALU_TYPE_PPS: u8 = 8;

/// Splits Annex B byte stream into individual NALUs (without start codes).
fn split_annexb_nalus(data: &[u8]) -> Vec<Bytes> {
    let mut nalus = Vec::new();
    let mut i = 0;

    while i < data.len() {
        let start = if data[i..].starts_with(&START_CODE_4) {
            i + 4
        } else if data[i..].starts_with(&START_CODE_3) {
            i + 3
        } else {
            i += 1;
            continue;
        };

        let mut end = data.len();
        let mut j = start;
        while j < data.len() {
            if data[j..].starts_with(&START_CODE_3) {
                end = j;
                break;
            }
            j += 1;
        }

        if start < end {
            nalus.push(Bytes::copy_from_slice(&data[start..end]));
        }
        i = end;
    }

    nalus
}

/// Converts Annex B NALUs to AVCC format (4-byte length prefix per NALU).
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

    let mut sps_list: Vec<&Bytes> = Vec::new();
    let mut pps_list: Vec<&Bytes> = Vec::new();

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
    // u8  configurationVersion = 1
    // u8  AVCProfileIndication
    // u8  profile_compatibility
    // u8  AVCLevelIndication
    // u8  lengthSizeMinusOne (0xFC | 3) = 0xFF (4-byte NALU lengths)
    // u8  numOfSequenceParameterSets (0xE0 | count)
    // for each SPS: u16 spsLength, sps bytes
    // u8  numOfPictureParameterSets
    // for each PPS: u16 ppsLength, pps bytes
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
