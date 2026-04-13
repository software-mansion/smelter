use std::ptr::NonNull;

use ash::vk;

use crate::{VulkanDecoderError, codec::h265::H265Codec, vulkan_encoder::FullEncoderParameters};

pub(crate) struct VkH265VideoParameterSet {
    pub(crate) vps: vk::native::StdVideoH265VideoParameterSet,

    profile_tier_level: Option<NonNull<vk::native::StdVideoH265ProfileTierLevel>>,
    dec_pic_buf_mgr: Option<NonNull<vk::native::StdVideoH265DecPicBufMgr>>,
}

fn profile_tier_level(
    params: &FullEncoderParameters<H265Codec>,
) -> vk::native::StdVideoH265ProfileTierLevel {
    vk::native::StdVideoH265ProfileTierLevel {
        flags: vk::native::StdVideoH265ProfileTierLevelFlags {
            _bitfield_align_1: [],
            _bitfield_1: vk::native::StdVideoH265ProfileTierLevelFlags::new_bitfield_1(
                1, 1, 0, 1, 1,
            ),
            __bindgen_padding_0: [0; 3],
        },
        general_profile_idc: params.profile.to_profile_idc(),
        general_level_idc: vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_6_1,
    }
}

fn dec_pic_buf_mgr(
    params: &FullEncoderParameters<H265Codec>,
) -> vk::native::StdVideoH265DecPicBufMgr {
    let mut dec_pic_buf_mgr = vk::native::StdVideoH265DecPicBufMgr {
        max_num_reorder_pics: [0; 7],
        max_dec_pic_buffering_minus1: [0; 7],
        max_latency_increase_plus1: [0; 7],
    };
    dec_pic_buf_mgr.max_dec_pic_buffering_minus1[0] = params.max_references.get() as u8;
    dec_pic_buf_mgr.max_latency_increase_plus1[0] = 1;
    dec_pic_buf_mgr.max_num_reorder_pics[0] = 0;

    dec_pic_buf_mgr
}

impl VkH265VideoParameterSet {
    pub(crate) fn new_encode(params: &FullEncoderParameters<H265Codec>) -> Self {
        let profile_tier_level = NonNull::from(Box::leak(Box::new(profile_tier_level(params))));

        let dec_pic_buf_mgr = NonNull::from(Box::leak(Box::new(dec_pic_buf_mgr(params))));

        Self {
            profile_tier_level: Some(profile_tier_level),
            dec_pic_buf_mgr: Some(dec_pic_buf_mgr),
            vps: vk::native::StdVideoH265VideoParameterSet {
                reserved1: 0,
                flags: vk::native::StdVideoH265VpsFlags {
                    _bitfield_align_1: [],
                    _bitfield_1: vk::native::StdVideoH265VpsFlags::new_bitfield_1(1, 1, 0, 0),
                    __bindgen_padding_0: [0; 3],
                },
                vps_video_parameter_set_id: 0,
                vps_max_sub_layers_minus1: 0,
                reserved2: 0,
                vps_num_units_in_tick: 0,
                vps_time_scale: 0,
                vps_num_ticks_poc_diff_one_minus1: 0,
                reserved3: 0,
                pHrdParameters: std::ptr::null(),
                pDecPicBufMgr: dec_pic_buf_mgr.as_ptr(),
                pProfileTierLevel: profile_tier_level.as_ptr() as *const _,
            },
        }
    }
}

impl Drop for VkH265VideoParameterSet {
    fn drop(&mut self) {
        unsafe {
            if let Some(profile_tier_level) = self.profile_tier_level {
                drop(Box::from_raw(profile_tier_level.as_ptr()));
            }

            if let Some(dec_pic_buf_mgr) = self.dec_pic_buf_mgr {
                drop(Box::from_raw(dec_pic_buf_mgr.as_ptr()));
            }
        }
    }
}

pub(crate) struct VkH265SequenceParameterSet {
    profile_tier_level: Option<NonNull<vk::native::StdVideoH265ProfileTierLevel>>,
    pub(crate) sps: vk::native::StdVideoH265SequenceParameterSet,
    dec_pic_buf_mgr: Option<NonNull<vk::native::StdVideoH265DecPicBufMgr>>,
}

impl VkH265SequenceParameterSet {
    pub(crate) fn new_encode(params: &FullEncoderParameters<H265Codec>) -> Self {
        // TODO: VUI
        let profile_tier_level = NonNull::from_mut(Box::leak(Box::new(profile_tier_level(params))));
        let dec_pic_buf_mgr = NonNull::from(Box::leak(Box::new(dec_pic_buf_mgr(params))));
        Self {
            sps: vk::native::StdVideoH265SequenceParameterSet {
                flags: vk::native::StdVideoH265SpsFlags {
                    _bitfield_align_1: [],
                    _bitfield_1: vk::native::StdVideoH265SpsFlags::new_bitfield_1(
                        1, 0, 0, 0, 0, 0, 0, 1, 0, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, // range extension
                        0, 0, 0, 0, 0, // scc extension
                    ),
                },
                chroma_format_idc:
                    vk::native::StdVideoH265ChromaFormatIdc_STD_VIDEO_H265_CHROMA_FORMAT_IDC_420,
                pic_width_in_luma_samples: params.width.get(),
                pic_height_in_luma_samples: params.height.get(),
                sps_video_parameter_set_id: 0,
                sps_max_sub_layers_minus1: 0,
                sps_seq_parameter_set_id: 0,
                bit_depth_luma_minus8: 0,
                bit_depth_chroma_minus8: 0,
                log2_max_pic_order_cnt_lsb_minus4: 4,
                log2_min_luma_coding_block_size_minus3: 0, // ffmpeg
                log2_diff_max_min_luma_coding_block_size: 3, // ffmpeg
                log2_min_luma_transform_block_size_minus2: 0, // ffmpeg
                log2_diff_max_min_luma_transform_block_size: 3, // ffmpeg
                max_transform_hierarchy_depth_inter: 0,
                max_transform_hierarchy_depth_intra: 0,
                num_short_term_ref_pic_sets: 0, // I think we will put ref sets in each slice?
                num_long_term_ref_pics_sps: 0,
                pcm_sample_bit_depth_luma_minus1: 0,   // disabled
                pcm_sample_bit_depth_chroma_minus1: 0, // disabled
                log2_min_pcm_luma_coding_block_size_minus3: 0, // disabled
                log2_diff_max_min_pcm_luma_coding_block_size: 0, //disabled
                reserved1: 0,
                reserved2: 0,
                palette_max_size: 0,                              //disabled
                delta_palette_max_predictor_size: 0,              //disabled
                motion_vector_resolution_control_idc: 0,          //disabled
                sps_num_palette_predictor_initializers_minus1: 0, //disabled
                conf_win_left_offset: 0,                          // TODO
                conf_win_right_offset: 0,                         // TODO
                conf_win_top_offset: 0,                           // TODO
                conf_win_bottom_offset: 0,                        // TODO
                pProfileTierLevel: profile_tier_level.as_ptr(),
                pDecPicBufMgr: dec_pic_buf_mgr.as_ptr(),
                pScalingLists: std::ptr::null(),
                pShortTermRefPicSet: std::ptr::null(),
                pLongTermRefPicsSps: std::ptr::null(),
                pSequenceParameterSetVui: std::ptr::null(), // TODO
                pPredictorPaletteEntries: std::ptr::null(),
            },

            profile_tier_level: Some(profile_tier_level),
            dec_pic_buf_mgr: Some(dec_pic_buf_mgr),
        }
    }
}

impl Drop for VkH265SequenceParameterSet {
    fn drop(&mut self) {
        unsafe {
            if let Some(profile_tier_level) = self.profile_tier_level {
                drop(Box::from_raw(profile_tier_level.as_ptr()));
            }

            if let Some(dec_pic_buf_mgr) = self.dec_pic_buf_mgr {
                drop(Box::from_raw(dec_pic_buf_mgr.as_ptr()));
            }
        }
    }
}

pub(crate) struct VkH265PictureParameterSet {
    pub(crate) pps: vk::native::StdVideoH265PictureParameterSet,
}

impl VkH265PictureParameterSet {
    pub(crate) fn new_encode() -> Self {
        Self {
            pps: vk::native::StdVideoH265PictureParameterSet {
                flags: vk::native::StdVideoH265PpsFlags {
                    _bitfield_align_1: [],
                    _bitfield_1: vk::native::StdVideoH265PpsFlags::new_bitfield_1(
                        0, 0, 1, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0,
                    ),
                },
                sps_video_parameter_set_id: 0,
                pps_seq_parameter_set_id: 0,
                pps_pic_parameter_set_id: 0,
                reserved1: 0,
                reserved2: 0,
                num_extra_slice_header_bits: 0,
                num_ref_idx_l0_default_active_minus1: 0,
                num_ref_idx_l1_default_active_minus1: 0,
                init_qp_minus26: 0,
                diff_cu_qp_delta_depth: 1,
                pps_cb_qp_offset: 0,
                pps_cr_qp_offset: 0,
                pps_beta_offset_div2: 0,
                pps_tc_offset_div2: 0,
                log2_parallel_merge_level_minus2: 0,
                log2_max_transform_skip_block_size_minus2: 0,
                diff_cu_chroma_qp_offset_depth: 0,
                chroma_qp_offset_list_len_minus1: 0,
                cb_qp_offset_list: [0; 6],
                cr_qp_offset_list: [0; 6],
                log2_sao_offset_scale_luma: 0,
                log2_sao_offset_scale_chroma: 0,
                pps_act_y_qp_offset_plus5: 0,
                pps_act_cb_qp_offset_plus5: 0,
                pps_act_cr_qp_offset_plus3: 0,
                pps_num_palette_predictor_initializers: 0,
                luma_bit_depth_entry_minus8: 0,
                chroma_bit_depth_entry_minus8: 0,
                num_tile_columns_minus1: 0,
                num_tile_rows_minus1: 0,
                column_width_minus1: [0; 19],
                row_height_minus1: [0; 21],
                reserved3: 0,
                pScalingLists: std::ptr::null(),
                pPredictorPaletteEntries: std::ptr::null(),
            },
        }
    }
}

pub(crate) fn vk_to_h265_level_idc(
    level_idc: vk::native::StdVideoH265LevelIdc,
) -> Result<u8, VulkanDecoderError> {
    match level_idc {
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_1_0 => Ok(30),
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_2_0 => Ok(60),
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_2_1 => Ok(63),
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_3_0 => Ok(90),
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_3_1 => Ok(93),
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_4_0 => Ok(120),
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_4_1 => Ok(123),
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_5_0 => Ok(150),
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_5_1 => Ok(153),
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_5_2 => Ok(156),
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_6_0 => Ok(180),
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_6_1 => Ok(183),
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_6_2 => Ok(186),
        _ => Err(VulkanDecoderError::InvalidInputData(format!(
            "unknown StdVideoH265LevelIdc: {level_idc}"
        ))),
    }
}
