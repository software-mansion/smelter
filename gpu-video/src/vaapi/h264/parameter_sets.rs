use bytes::Bytes;

use crate::{VideoFramerate, VideoResolution};

const H264_PROFILE_MAIN: u8 = 77;
pub(super) const H264_LEVEL_4_0: u8 = 40;
pub(super) const LOG2_MAX_FRAME_NUM_MINUS4: u32 = 12;
pub(super) const LOG2_MAX_PIC_ORDER_CNT_LSB_MINUS4: u32 = 12;
const MAX_NUM_REF_FRAMES: u32 = 1;
const MAX_NUM_REORDER_FRAMES: u32 = 0;
const MAX_DEC_FRAME_BUFFERING: u32 = MAX_NUM_REF_FRAMES;
const LOG2_MAX_MV_LENGTH: u32 = 16;
const PPS_NAL: &[u8] = &[0, 0, 0, 1, 0x68, 0xce, 0x3c, 0x80];

pub(super) fn main_parameter_sets(
    resolution: VideoResolution,
    framerate: VideoFramerate,
) -> Bytes {
    let coded_width = resolution.width.next_multiple_of(16);
    let coded_height = resolution.height.next_multiple_of(16);
    let width_mbs = coded_width / 16;
    let height_mbs = coded_height / 16;
    let crop_right = (coded_width - resolution.width) / 2;
    let crop_bottom = (coded_height - resolution.height) / 2;

    let mut out = Vec::new();
    append_annexb_nal(
        &mut out,
        0x67,
        sps_rbsp(width_mbs, height_mbs, crop_right, crop_bottom, framerate),
    );
    out.extend_from_slice(PPS_NAL);
    out.into()
}

fn sps_rbsp(
    width_mbs: u32,
    height_mbs: u32,
    crop_right: u32,
    crop_bottom: u32,
    framerate: VideoFramerate,
) -> Vec<u8> {
    let mut bits = BitWriter::new();
    bits.bits(H264_PROFILE_MAIN.into(), 8);
    bits.bits(0, 8);
    bits.bits(H264_LEVEL_4_0.into(), 8);
    bits.ue(0);
    bits.ue(LOG2_MAX_FRAME_NUM_MINUS4);
    bits.ue(0);
    bits.ue(LOG2_MAX_PIC_ORDER_CNT_LSB_MINUS4);
    bits.ue(MAX_NUM_REF_FRAMES);
    bits.bit(false);
    bits.ue(width_mbs - 1);
    bits.ue(height_mbs - 1);
    bits.bit(true);
    bits.bit(true);
    bits.bit(crop_right > 0 || crop_bottom > 0);
    if crop_right > 0 || crop_bottom > 0 {
        bits.ue(0);
        bits.ue(crop_right);
        bits.ue(0);
        bits.ue(crop_bottom);
    }
    bits.bit(true);
    bits.bit(true);
    bits.bits(1, 8);
    bits.bit(false);
    bits.bit(false);
    bits.bit(false);
    bits.bit(true);
    bits.bits(framerate.den.max(1), 32);
    bits.bits(framerate.num.max(1).saturating_mul(2), 32);
    bits.bit(true);
    bits.bit(false);
    bits.bit(false);
    bits.bit(false);
    bits.bit(true);
    bits.bit(true);
    bits.ue(0);
    bits.ue(0);
    bits.ue(LOG2_MAX_MV_LENGTH);
    bits.ue(LOG2_MAX_MV_LENGTH);
    bits.ue(MAX_NUM_REORDER_FRAMES);
    bits.ue(MAX_DEC_FRAME_BUFFERING);
    bits.finish_rbsp()
}

struct BitWriter {
    bytes: Vec<u8>,
    byte: u8,
    used: u8,
}

impl BitWriter {
    fn new() -> Self {
        Self { bytes: Vec::new(), byte: 0, used: 0 }
    }

    fn bit(&mut self, value: bool) {
        self.byte = (self.byte << 1) | u8::from(value);
        self.used += 1;
        if self.used == 8 {
            self.bytes.push(self.byte);
            self.byte = 0;
            self.used = 0;
        }
    }

    fn bits(&mut self, value: u32, count: u8) {
        for shift in (0..count).rev() {
            self.bit(((value >> shift) & 1) != 0);
        }
    }

    fn ue(&mut self, value: u32) {
        let code = value + 1;
        let bits = u32::BITS - code.leading_zeros();
        for _ in 0..bits - 1 {
            self.bit(false);
        }
        self.bits(code, bits as u8);
    }

    fn finish_rbsp(mut self) -> Vec<u8> {
        self.bit(true);
        while self.used != 0 {
            self.bit(false);
        }
        self.bytes
    }
}

fn append_annexb_nal(out: &mut Vec<u8>, header: u8, rbsp: Vec<u8>) {
    out.extend_from_slice(&[0, 0, 0, 1, header]);
    let mut zero_count = 0;
    for byte in rbsp {
        if zero_count >= 2 && byte <= 3 {
            out.push(3);
            zero_count = 0;
        }
        out.push(byte);
        zero_count = if byte == 0 { zero_count + 1 } else { 0 };
    }
}

#[cfg(test)]
mod tests {
    use bytes::{BufMut, BytesMut};
    use h264_reader::nal::{Nal, RefNal, sps::SeqParameterSet};

    use super::*;

    #[test]
    fn parameter_sets_build_avc_config() {
        let parameter_sets = main_parameter_sets(
            VideoResolution { width: 1920, height: 1080 },
            VideoFramerate { num: 30, den: 1 },
        );
        let config = build_avc_decoder_config(&parameter_sets).unwrap();
        assert_eq!(&config[..4], &[1, H264_PROFILE_MAIN, 0, H264_LEVEL_4_0]);
    }

    #[test]
    fn parameter_sets_use_annexb_start_codes() {
        let parameter_sets = main_parameter_sets(
            VideoResolution { width: 1280, height: 720 },
            VideoFramerate { num: 60, den: 1 },
        );
        assert!(parameter_sets.starts_with(&[0, 0, 0, 1, 0x67]));
        assert!(parameter_sets.windows(5).any(|window| window == [0, 0, 0, 1, 0x68]));
    }

    #[test]
    fn main_profile_sps_matches_1080p_ntsc_timing() {
        let parameter_sets = main_parameter_sets(
            VideoResolution { width: 1920, height: 1080 },
            VideoFramerate { num: 30_000, den: 1001 },
        );
        let expected_sps = [
            0x00, 0x00, 0x00, 0x01, 0x67, 0x4d, 0x00, 0x28, 0x8d, 0x8d, 0x40, 0x3c, 0x01,
            0x13, 0xf2, 0xe0, 0x22, 0x00, 0x00, 0x07, 0xd2, 0x00, 0x01, 0xd4, 0xc1, 0x1e,
            0x11, 0x08, 0xd4,
        ];
        assert!(parameter_sets.starts_with(&expected_sps));
    }

    #[test]
    fn main_profile_sps_declares_no_reordering() {
        let parameter_sets = main_parameter_sets(
            VideoResolution { width: 1280, height: 720 },
            VideoFramerate { num: 30, den: 1 },
        );
        let sps = parse_sps(&parameter_sets);
        let restrictions = sps.vui_parameters.unwrap().bitstream_restrictions.unwrap();

        assert_eq!(sps.max_num_ref_frames, MAX_NUM_REF_FRAMES);
        assert_eq!(restrictions.max_num_reorder_frames, MAX_NUM_REORDER_FRAMES);
        assert_eq!(restrictions.max_dec_frame_buffering, MAX_DEC_FRAME_BUFFERING);
    }

    fn build_avc_decoder_config(data: &[u8]) -> Option<bytes::Bytes> {
        let nalus = split_annexb_nalus(data);
        let sps = nalus
            .iter()
            .find(|nalu| nalu.first().is_some_and(|byte| byte & 0x1f == 7))?;
        let pps = nalus
            .iter()
            .find(|nalu| nalu.first().is_some_and(|byte| byte & 0x1f == 8))?;
        let mut config = BytesMut::new();
        config.put_u8(1);
        config.extend_from_slice(&sps[1..4]);
        config.put_u8(0xff);
        config.put_u8(0xe1);
        config.put_u16(sps.len() as u16);
        config.extend_from_slice(sps);
        config.put_u8(1);
        config.put_u16(pps.len() as u16);
        config.extend_from_slice(pps);
        Some(config.freeze())
    }

    fn split_annexb_nalus(data: &[u8]) -> Vec<&[u8]> {
        let mut nalus = Vec::new();
        let mut start = None;
        let mut i = 0;
        while i + 3 <= data.len() {
            let start_code_len = if data[i..].starts_with(&[0, 0, 1]) {
                Some(3)
            } else if data[i..].starts_with(&[0, 0, 0, 1]) {
                Some(4)
            } else {
                None
            };
            if let Some(len) = start_code_len {
                if let Some(nalu_start) = start {
                    nalus.push(&data[nalu_start..i]);
                }
                start = Some(i + len);
                i += len;
            } else {
                i += 1;
            }
        }
        if let Some(nalu_start) = start {
            nalus.push(&data[nalu_start..]);
        }
        nalus
    }

    fn parse_sps(data: &[u8]) -> SeqParameterSet {
        let sps = split_annexb_nalus(data)
            .into_iter()
            .find(|nalu| nalu.first().is_some_and(|byte| byte & 0x1f == 7))
            .unwrap();
        let nal = RefNal::new(sps, &[], true);
        SeqParameterSet::from_bits(nal.rbsp_bits()).unwrap()
    }
}
