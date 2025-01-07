use ash::vk;

use crate::H264Profile;

use super::VulkanEncoderError;

const MACROBLOCK_SIZE: u32 = 16;
pub(crate) const MAX_FRAME_NUM: u32 = 1 << 7;
const LOG2_MAX_FRAME_NUM_MINUS_4: u8 = (MAX_FRAME_NUM.ilog2() as u8) - 4;

pub(crate) fn vui(fps: u32) -> Result<vk::native::StdVideoH264SequenceParameterSetVui, VulkanEncoderError> {
    let vui = vk::native::StdVideoH264SequenceParameterSetVui {
        flags: vk::native::StdVideoH264SpsVuiFlags {
            _bitfield_align_1: [],
            _bitfield_1: vk::native::StdVideoH264SpsVuiFlags::new_bitfield_1(
                1,
                1,
                0,
                0,
                0,
                0,
                0,
                1,
                1,
                0, // TODO: b-frames
                0,
                0
            ),
            __bindgen_padding_0: 0,
        },
        aspect_ratio_idc: vk::native::StdVideoH264AspectRatioIdc_STD_VIDEO_H264_ASPECT_RATIO_IDC_SQUARE,
        sar_width: 0,
        sar_height: 0,
        video_format: 0,
        colour_primaries: 0,
        transfer_characteristics: 0,
        matrix_coefficients: 0,
        num_units_in_tick: 1,
        time_scale: fps * 2,
        max_num_reorder_frames: 0, // TODO: b-frames
        max_dec_frame_buffering: 0, // TODO: b-frames
        chroma_sample_loc_type_top_field: 0,
        chroma_sample_loc_type_bottom_field: 0,
        reserved1: 0,
        pHrdParameters: std::ptr::null(),
    };

    Ok(vui)
}

pub(crate) fn sps(
    profile: H264Profile,
    width: u32,
    height: u32,
    max_references: u32,
) -> Result<vk::native::StdVideoH264SequenceParameterSet, VulkanEncoderError> {
    if width % MACROBLOCK_SIZE != 0 || height % MACROBLOCK_SIZE != 0 {
        return Err(VulkanEncoderError::DimensionsNotDivisibleBy16);
    }

    let pic_width_in_mbs_minus1 = width / MACROBLOCK_SIZE - 1;
    let pic_height_in_map_units_minus1 = height / MACROBLOCK_SIZE - 1;

    Ok(vk::native::StdVideoH264SequenceParameterSet {
        flags: vk::native::StdVideoH264SpsFlags {
            _bitfield_align_1: [0; 0],
            __bindgen_padding_0: 0,
            _bitfield_1: vk::native::StdVideoH264SpsFlags::new_bitfield_1(
                0, 0, 0, 0, 0, 1, // flag 5 equal to 1 turns off B-slices
                1, // ffmpeg
                0, 1, // 1 - no fields
                0, // only for pic_order_cnt_type 1
                0, 0, 0, // ffmpeg
                0, // TODO: frame cropping
                0, 0,
            ),
        },
        profile_idc: profile.to_profile_idc(),
        level_idc: vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_4_1,
        chroma_format_idc:
            vk::native::StdVideoH264ChromaFormatIdc_STD_VIDEO_H264_CHROMA_FORMAT_IDC_420,
        seq_parameter_set_id: 0,
        bit_depth_luma_minus8: 0,
        bit_depth_chroma_minus8: 0,
        log2_max_frame_num_minus4: LOG2_MAX_FRAME_NUM_MINUS_4, // TODO: see how this impacts output
        pic_order_cnt_type: vk::native::StdVideoH264PocType_STD_VIDEO_H264_POC_TYPE_0,
        offset_for_non_ref_pic: 0,         // only for pic_order_cnt_type 1
        offset_for_top_to_bottom_field: 0, // only for pic_order_cnt_type 1
        log2_max_pic_order_cnt_lsb_minus4: 4, // only for pic_order_cnt_type 0
        num_ref_frames_in_pic_order_cnt_cycle: 0, // only for pic_order_cnt_type 1
        max_num_ref_frames: max_references as u8,
        reserved1: 0,
        pic_width_in_mbs_minus1,
        pic_height_in_map_units_minus1,
        frame_crop_left_offset: 0,
        frame_crop_right_offset: 0,
        frame_crop_top_offset: 0,
        frame_crop_bottom_offset: 0,
        reserved2: 0,
        pOffsetForRefFrame: std::ptr::null(),
        pScalingLists: std::ptr::null(),
        pSequenceParameterSetVui: std::ptr::null(), // TODO: VUI
    })
}

pub(crate) fn pps() -> vk::native::StdVideoH264PictureParameterSet {
    vk::native::StdVideoH264PictureParameterSet {
        flags: vk::native::StdVideoH264PpsFlags {
            __bindgen_padding_0: [0; 3],
            _bitfield_align_1: [],
            _bitfield_1: vk::native::StdVideoH264PpsFlags::new_bitfield_1(
                0, 0, 0, 1, // maybe turn off to enable superfast decoding
                0, // think about this -- think really hard, it seems this
                // means you need to supply the weights yourself
                0, 1, 0,
            ),
        },
        seq_parameter_set_id: 0,
        pic_parameter_set_id: 0,
        num_ref_idx_l0_default_active_minus1: 0,
        num_ref_idx_l1_default_active_minus1: 0,
        weighted_bipred_idc:
            vk::native::StdVideoH264WeightedBipredIdc_STD_VIDEO_H264_WEIGHTED_BIPRED_IDC_DEFAULT, // for b frames
        pic_init_qp_minus26: 0, // no idea what this is, ffmpeg sets this to -4, BBB has 0
        pic_init_qs_minus26: 0, // no idea what this is, ffmpeg sets this to 0, BBB has 0
        chroma_qp_index_offset: 0, // no idea what this is, ffmpeg sets this to 0, BBB has 0
        second_chroma_qp_index_offset: 0, // no idea what this is, ffmpeg sets this to 0, BBB has 0
        pScalingLists: std::ptr::null(),
    }
}
