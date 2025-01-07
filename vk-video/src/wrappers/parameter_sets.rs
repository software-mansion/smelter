use std::ptr::NonNull;

use ash::vk;
use h264_reader::nal::sps::{FrameMbsFlags, SeqParameterSet};

use crate::VulkanDecoderError;

const MACROBLOCK_SIZE: u32 = 16;

pub(crate) trait SeqParameterSetExt {
    fn size(&self) -> Result<vk::Extent2D, VulkanDecoderError>;
}

impl SeqParameterSetExt for SeqParameterSet {
    #[allow(non_snake_case)]
    fn size(&self) -> Result<vk::Extent2D, VulkanDecoderError> {
        let chroma_array_type = if self.chroma_info.separate_colour_plane_flag {
            0
        } else {
            self.chroma_info.chroma_format.to_chroma_format_idc()
        };

        let (SubWidthC, SubHeightC) = match self.chroma_info.chroma_format {
            h264_reader::nal::sps::ChromaFormat::Monochrome => {
                return Err(VulkanDecoderError::MonochromeChromaFormatUnsupported)
            }
            h264_reader::nal::sps::ChromaFormat::YUV420 => (2, 2),
            h264_reader::nal::sps::ChromaFormat::YUV422 => (2, 1),
            h264_reader::nal::sps::ChromaFormat::YUV444 => (1, 1),
            h264_reader::nal::sps::ChromaFormat::Invalid(x) => {
                return Err(VulkanDecoderError::InvalidInputData(format!(
                    "Invalid chroma_format_idc: {x}"
                )))
            }
        };

        let (CropUnitX, CropUnitY) = match chroma_array_type {
            0 => (
                1,
                2 - (self.frame_mbs_flags == FrameMbsFlags::Frames) as u32,
            ),

            _ => (
                SubWidthC,
                SubHeightC * (2 - (self.frame_mbs_flags == FrameMbsFlags::Frames) as u32),
            ),
        };

        let (width_offset, height_offset) = match &self.frame_cropping {
            None => (0, 0),
            Some(frame_cropping) => (
                (frame_cropping.left_offset + frame_cropping.right_offset) * CropUnitX,
                (frame_cropping.top_offset + frame_cropping.bottom_offset) * CropUnitY,
            ),
        };

        let width = (self.pic_width_in_mbs_minus1 + 1) * MACROBLOCK_SIZE - width_offset;
        let height = (self.pic_height_in_map_units_minus1 + 1)
            * (2 - (self.frame_mbs_flags == FrameMbsFlags::Frames) as u32)
            * MACROBLOCK_SIZE
            - height_offset;

        Ok(vk::Extent2D { width, height })
    }
}

pub(crate) struct VkSequenceParameterSet {
    pub(crate) sps: vk::native::StdVideoH264SequenceParameterSet,
    scaling_lists_ptr: Option<NonNull<H264ScalingLists>>,
    // in the future, heap-allocated VUI and HRD parameters can be put here to have everything
    // together
}

impl From<&'_ SeqParameterSet> for VkSequenceParameterSet {
    #[allow(non_snake_case)]
    fn from(sps: &SeqParameterSet) -> VkSequenceParameterSet {
        let flags = vk::native::StdVideoH264SpsFlags {
            _bitfield_1: vk::native::StdVideoH264SpsFlags::new_bitfield_1(
                sps.constraint_flags.flag0().into(),
                sps.constraint_flags.flag1().into(),
                sps.constraint_flags.flag2().into(),
                sps.constraint_flags.flag3().into(),
                sps.constraint_flags.flag4().into(),
                sps.constraint_flags.flag5().into(),
                sps.direct_8x8_inference_flag.into(),
                match sps.frame_mbs_flags {
                    h264_reader::nal::sps::FrameMbsFlags::Frames => 0,
                    h264_reader::nal::sps::FrameMbsFlags::Fields {
                        mb_adaptive_frame_field_flag,
                    } => mb_adaptive_frame_field_flag.into(),
                },
                matches!(
                    sps.frame_mbs_flags,
                    h264_reader::nal::sps::FrameMbsFlags::Frames
                )
                .into(),
                match sps.pic_order_cnt {
                    h264_reader::nal::sps::PicOrderCntType::TypeOne {
                        delta_pic_order_always_zero_flag,
                        ..
                    } => delta_pic_order_always_zero_flag.into(),
                    // The spec doesn't say what to do if this flag is not present...
                    h264_reader::nal::sps::PicOrderCntType::TypeZero { .. }
                    | h264_reader::nal::sps::PicOrderCntType::TypeTwo => 0,
                },
                sps.chroma_info.separate_colour_plane_flag.into(),
                sps.gaps_in_frame_num_value_allowed_flag.into(),
                sps.chroma_info.qpprime_y_zero_transform_bypass_flag.into(),
                sps.frame_cropping.is_some().into(),
                sps.chroma_info.scaling_matrix.is_some().into(),
                0,
            ),
            _bitfield_align_1: [],
            __bindgen_padding_0: 0,
        };

        let profile_idc: u8 = sps.profile_idc.into();

        let pic_order_cnt_type = match sps.pic_order_cnt {
            h264_reader::nal::sps::PicOrderCntType::TypeZero { .. } => 0,
            h264_reader::nal::sps::PicOrderCntType::TypeOne { .. } => 1,
            h264_reader::nal::sps::PicOrderCntType::TypeTwo => 2,
        };

        let (
            offset_for_non_ref_pic,
            offset_for_top_to_bottom_field,
            num_ref_frames_in_pic_order_cnt_cycle,
        ) = match &sps.pic_order_cnt {
            h264_reader::nal::sps::PicOrderCntType::TypeOne {
                offset_for_non_ref_pic,
                offset_for_top_to_bottom_field,
                offsets_for_ref_frame,
                ..
            } => (
                *offset_for_non_ref_pic,
                *offset_for_top_to_bottom_field,
                offsets_for_ref_frame.len() as u8,
            ),
            h264_reader::nal::sps::PicOrderCntType::TypeZero { .. } => (0, 0, 0),
            h264_reader::nal::sps::PicOrderCntType::TypeTwo => (0, 0, 0),
        };

        let log2_max_pic_order_cnt_lsb_minus4 = match &sps.pic_order_cnt {
            h264_reader::nal::sps::PicOrderCntType::TypeZero {
                log2_max_pic_order_cnt_lsb_minus4,
            } => *log2_max_pic_order_cnt_lsb_minus4,
            h264_reader::nal::sps::PicOrderCntType::TypeOne { .. }
            | h264_reader::nal::sps::PicOrderCntType::TypeTwo => 0,
        };

        let (
            frame_crop_left_offset,
            frame_crop_right_offset,
            frame_crop_top_offset,
            frame_crop_bottom_offset,
        ) = match sps.frame_cropping {
            Some(h264_reader::nal::sps::FrameCropping {
                left_offset,
                right_offset,
                top_offset,
                bottom_offset,
            }) => (left_offset, right_offset, top_offset, bottom_offset),
            None => (0, 0, 0, 0),
        };

        let pOffsetForRefFrame = match &sps.pic_order_cnt {
            h264_reader::nal::sps::PicOrderCntType::TypeOne {
                offsets_for_ref_frame,
                ..
            } => offsets_for_ref_frame.as_ptr(),
            h264_reader::nal::sps::PicOrderCntType::TypeZero { .. }
            | h264_reader::nal::sps::PicOrderCntType::TypeTwo => std::ptr::null(),
        };

        let scaling_lists: Option<&mut H264ScalingLists> = sps
            .chroma_info
            .scaling_matrix
            .as_ref()
            .map(|matrix| Box::leak(Box::new(matrix.into())));

        let pScalingLists = match scaling_lists.as_ref() {
            Some(l) => &l.list,
            None => std::ptr::null(),
        };

        let scaling_lists_ptr = scaling_lists.map(NonNull::from);

        // TODO: this is not necessary to reconstruct samples. I don't know why the decoder would
        // need this. Maybe we can do this in the future.
        let pSequenceParameterSetVui = std::ptr::null();

        Self {
            sps: vk::native::StdVideoH264SequenceParameterSet {
                flags,
                profile_idc: profile_idc as u32,
                level_idc: h264_level_idc_to_vk(sps.level_idc),
                chroma_format_idc: sps.chroma_info.chroma_format.to_chroma_format_idc(),
                seq_parameter_set_id: sps.seq_parameter_set_id.id(),
                bit_depth_luma_minus8: sps.chroma_info.bit_depth_luma_minus8,
                bit_depth_chroma_minus8: sps.chroma_info.bit_depth_chroma_minus8,
                log2_max_frame_num_minus4: sps.log2_max_frame_num_minus4,
                pic_order_cnt_type,
                offset_for_non_ref_pic,
                offset_for_top_to_bottom_field,
                num_ref_frames_in_pic_order_cnt_cycle,
                log2_max_pic_order_cnt_lsb_minus4,
                max_num_ref_frames: sps.max_num_ref_frames as u8,
                reserved1: 0,
                pic_width_in_mbs_minus1: sps.pic_width_in_mbs_minus1,
                pic_height_in_map_units_minus1: sps.pic_height_in_map_units_minus1,
                frame_crop_left_offset,
                frame_crop_right_offset,
                frame_crop_top_offset,
                frame_crop_bottom_offset,
                reserved2: 0,
                pOffsetForRefFrame,
                pScalingLists,
                pSequenceParameterSetVui,
            },
            scaling_lists_ptr,
        }
    }
}

impl Drop for VkSequenceParameterSet {
    fn drop(&mut self) {
        self.scaling_lists_ptr
            .map(|p| unsafe { Box::from_raw(p.as_ptr()) });
    }
}

pub(crate) struct H264ScalingLists {
    pub(crate) list: vk::native::StdVideoH264ScalingLists,
}

impl Default for H264ScalingLists {
    fn default() -> Self {
        Self {
            list: vk::native::StdVideoH264ScalingLists {
                scaling_list_present_mask: !0,
                use_default_scaling_matrix_mask: 0,
                ScalingList4x4: [[0; 16]; 6],
                ScalingList8x8: [[0; 64]; 6],
            },
        }
    }
}

impl H264ScalingLists {
    fn insert_4x4(&mut self, list: &[h264_reader::nal::sps::ScalingList<16>]) {
        for (i, list) in list.iter().enumerate() {
            match list {
                h264_reader::nal::sps::ScalingList::NotPresent => {
                    self.list.scaling_list_present_mask &= !(1 << i)
                }
                h264_reader::nal::sps::ScalingList::UseDefault => {
                    self.list.use_default_scaling_matrix_mask |= 1 << i
                }
                h264_reader::nal::sps::ScalingList::List(l) => {
                    self.list.ScalingList4x4[i] = l.map(|n| n.get())
                }
            }
        }
    }

    fn insert_8x8(&mut self, list: &[h264_reader::nal::sps::ScalingList<64>]) {
        for (i, list) in list.iter().enumerate() {
            match list {
                h264_reader::nal::sps::ScalingList::NotPresent => {
                    self.list.scaling_list_present_mask &= !(1 << (i + 6))
                }
                h264_reader::nal::sps::ScalingList::UseDefault => {
                    self.list.use_default_scaling_matrix_mask |= 1 << (i + 6)
                }
                h264_reader::nal::sps::ScalingList::List(l) => {
                    self.list.ScalingList8x8[i] = l.map(|n| n.get())
                }
            }
        }
    }
}

impl From<&h264_reader::nal::sps::SeqScalingMatrix> for H264ScalingLists {
    fn from(value: &h264_reader::nal::sps::SeqScalingMatrix) -> Self {
        let mut result = H264ScalingLists::default();

        result.insert_4x4(&value.scaling_list4x4);
        result.insert_8x8(&value.scaling_list8x8);

        result
    }
}

impl From<&h264_reader::nal::pps::PicScalingMatrix> for H264ScalingLists {
    fn from(value: &h264_reader::nal::pps::PicScalingMatrix) -> Self {
        let mut result = H264ScalingLists::default();

        result.insert_4x4(&value.scaling_list4x4);

        if let Some(v) = &value.scaling_list8x8 {
            result.insert_8x8(v);
        }

        result
    }
}

trait ChromaFormatExt {
    fn to_chroma_format_idc(&self) -> u32;
}

impl ChromaFormatExt for h264_reader::nal::sps::ChromaFormat {
    fn to_chroma_format_idc(&self) -> u32 {
        match self {
            h264_reader::nal::sps::ChromaFormat::Monochrome => 0,
            h264_reader::nal::sps::ChromaFormat::YUV420 => 1,
            h264_reader::nal::sps::ChromaFormat::YUV422 => 2,
            h264_reader::nal::sps::ChromaFormat::YUV444 => 3,
            h264_reader::nal::sps::ChromaFormat::Invalid(v) => *v,
        }
    }
}

pub(crate) fn vk_to_h264_level_idc(
    level_idc: vk::native::StdVideoH264LevelIdc,
) -> Result<u8, VulkanDecoderError> {
    match level_idc {
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_1_0 => Ok(10),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_1_1 => Ok(11),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_1_2 => Ok(12),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_1_3 => Ok(13),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_2_0 => Ok(20),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_2_1 => Ok(21),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_2_2 => Ok(22),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_3_0 => Ok(30),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_3_1 => Ok(31),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_3_2 => Ok(32),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_4_0 => Ok(40),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_4_1 => Ok(41),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_4_2 => Ok(42),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_5_0 => Ok(50),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_5_1 => Ok(51),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_5_2 => Ok(52),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_6_0 => Ok(60),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_6_1 => Ok(61),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_6_2 => Ok(62),
        _ => Err(VulkanDecoderError::InvalidInputData(format!(
            "unknown StdVideoH264LevelIdc: {level_idc}"
        ))),
    }
}

/// As per __Table A-1 Level limits__ in the H.264 spec
/// `mbs` means macroblocks here
pub(crate) fn h264_level_idc_to_max_dpb_mbs(level_idc: u8) -> Result<u64, VulkanDecoderError> {
    match level_idc {
        10 => Ok(396),
        11 => Ok(900),
        12 => Ok(2_376),
        13 => Ok(2_376),
        20 => Ok(2_376),
        21 => Ok(4_752),
        22 => Ok(8_100),
        30 => Ok(8_100),
        31 => Ok(18_000),
        32 => Ok(20_480),
        40 => Ok(32_768),
        41 => Ok(32_768),
        42 => Ok(34_816),
        50 => Ok(110_400),
        51 => Ok(184_320),
        52 => Ok(184_320),
        60 => Ok(696_320),
        61 => Ok(696_320),
        62 => Ok(696_320),
        _ => Err(VulkanDecoderError::InvalidInputData(format!(
            "unknown h264 level_idc: {level_idc}"
        ))),
    }
}

fn h264_level_idc_to_vk(level_idc: u8) -> u32 {
    match level_idc {
        10 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_1_0,
        11 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_1_1,
        12 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_1_2,
        13 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_1_3,
        20 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_2_0,
        21 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_2_1,
        22 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_2_2,
        30 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_3_0,
        31 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_3_1,
        32 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_3_2,
        40 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_4_0,
        41 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_4_1,
        42 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_4_2,
        50 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_5_0,
        51 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_5_1,
        52 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_5_2,
        60 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_6_0,
        61 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_6_1,
        62 => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_6_2,
        _ => vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_INVALID,
    }
}

fn h264_profile_idc_to_vk(
    profile: h264_reader::nal::sps::Profile,
) -> vk::native::StdVideoH264ProfileIdc {
    match profile {
        h264_reader::nal::sps::Profile::Baseline => {
            vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_BASELINE
        }
        h264_reader::nal::sps::Profile::Main => {
            vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_MAIN
        }
        h264_reader::nal::sps::Profile::High => {
            vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_HIGH
        }
        h264_reader::nal::sps::Profile::High444 => {
            vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_HIGH_444_PREDICTIVE
        }
        h264_reader::nal::sps::Profile::High422
        | h264_reader::nal::sps::Profile::High10
        | h264_reader::nal::sps::Profile::Extended
        | h264_reader::nal::sps::Profile::ScalableBase
        | h264_reader::nal::sps::Profile::ScalableHigh
        | h264_reader::nal::sps::Profile::MultiviewHigh
        | h264_reader::nal::sps::Profile::StereoHigh
        | h264_reader::nal::sps::Profile::MFCDepthHigh
        | h264_reader::nal::sps::Profile::MultiviewDepthHigh
        | h264_reader::nal::sps::Profile::EnhancedMultiviewDepthHigh
        | h264_reader::nal::sps::Profile::Unknown(_) => {
            vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_INVALID
        }
    }
}

pub(crate) struct H264ProfileInfo<'a> {
    pub(crate) profile_info: vk::VideoProfileInfoKHR<'a>,
    h264_info_ptr: NonNull<vk::VideoDecodeH264ProfileInfoKHR<'a>>,
}

impl PartialEq for H264ProfileInfo<'_> {
    fn eq(&self, other: &Self) -> bool {
        unsafe {
            other.profile_info.chroma_subsampling == self.profile_info.chroma_subsampling
                && other.profile_info.luma_bit_depth == self.profile_info.luma_bit_depth
                && other.profile_info.chroma_bit_depth == self.profile_info.chroma_bit_depth
                && (*other.h264_info_ptr.as_ptr()).std_profile_idc
                    == (*self.h264_info_ptr.as_ptr()).std_profile_idc
                && (*other.h264_info_ptr.as_ptr()).picture_layout
                    == (*self.h264_info_ptr.as_ptr()).picture_layout
        }
    }
}

impl Eq for H264ProfileInfo<'_> {}

impl H264ProfileInfo<'_> {
    pub(crate) fn from_sps_decode(sps: &SeqParameterSet) -> Result<Self, VulkanDecoderError> {
        let profile_idc = h264_profile_idc_to_vk(sps.profile());

        if profile_idc == vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_INVALID {
            return Err(VulkanDecoderError::InvalidInputData(
                "unsupported h264 profile".into(),
            ));
        }

        let h264_profile_info = Box::leak(Box::new(
            vk::VideoDecodeH264ProfileInfoKHR::default()
                .std_profile_idc(profile_idc)
                .picture_layout(vk::VideoDecodeH264PictureLayoutFlagsKHR::PROGRESSIVE),
        ));

        let chroma_subsampling = match sps.chroma_info.chroma_format {
            h264_reader::nal::sps::ChromaFormat::YUV420 => {
                vk::VideoChromaSubsamplingFlagsKHR::TYPE_420
            }
            h264_reader::nal::sps::ChromaFormat::Monochrome
            | h264_reader::nal::sps::ChromaFormat::YUV422
            | h264_reader::nal::sps::ChromaFormat::YUV444
            | h264_reader::nal::sps::ChromaFormat::Invalid(_) => {
                return Err(VulkanDecoderError::InvalidInputData(format!(
                    "unsupported chroma format: {:?}",
                    sps.chroma_info.chroma_format
                )))
            }
        };

        let luma_bit_depth = if sps.chroma_info.bit_depth_luma_minus8 + 8 == 8 {
            vk::VideoComponentBitDepthFlagsKHR::TYPE_8
        } else {
            return Err(VulkanDecoderError::InvalidInputData(format!(
                "unsupported luma bit length: {}",
                sps.chroma_info.bit_depth_luma_minus8 + 8
            )));
        };

        let chroma_bit_depth = if sps.chroma_info.bit_depth_chroma_minus8 + 8 == 8 {
            vk::VideoComponentBitDepthFlagsKHR::TYPE_8
        } else {
            return Err(VulkanDecoderError::InvalidInputData(format!(
                "unsupported chroma bit length: {}",
                sps.chroma_info.bit_depth_chroma_minus8 + 8
            )));
        };

        let h264_info_ptr = NonNull::from(&mut *h264_profile_info);
        let profile_info = vk::VideoProfileInfoKHR::default()
            .video_codec_operation(vk::VideoCodecOperationFlagsKHR::DECODE_H264)
            .chroma_subsampling(chroma_subsampling)
            .luma_bit_depth(luma_bit_depth)
            .chroma_bit_depth(chroma_bit_depth)
            .push_next(h264_profile_info);

        Ok(Self {
            profile_info,
            h264_info_ptr,
        })
    }
}

impl Drop for H264ProfileInfo<'_> {
    fn drop(&mut self) {
        unsafe {
            let _ = Box::from_raw(self.h264_info_ptr.as_ptr());
        }
    }
}

pub(crate) struct VkPictureParameterSet {
    pub(crate) pps: vk::native::StdVideoH264PictureParameterSet,
    scaling_list_ptr: Option<NonNull<H264ScalingLists>>,
}

impl From<&'_ h264_reader::nal::pps::PicParameterSet> for VkPictureParameterSet {
    #[allow(non_snake_case)]
    fn from(pps: &h264_reader::nal::pps::PicParameterSet) -> Self {
        let flags = vk::native::StdVideoH264PpsFlags {
            _bitfield_align_1: [],
            __bindgen_padding_0: [0; 3],
            _bitfield_1: vk::native::StdVideoH264PpsFlags::new_bitfield_1(
                pps.extension
                    .as_ref()
                    .map(|ext| ext.transform_8x8_mode_flag.into())
                    .unwrap_or(0),
                pps.redundant_pic_cnt_present_flag.into(),
                pps.constrained_intra_pred_flag.into(),
                pps.deblocking_filter_control_present_flag.into(),
                pps.weighted_pred_flag.into(),
                pps.bottom_field_pic_order_in_frame_present_flag.into(),
                pps.entropy_coding_mode_flag.into(),
                pps.extension
                    .as_ref()
                    .map(|ext| ext.pic_scaling_matrix.is_some().into())
                    .unwrap_or(0),
            ),
        };

        let chroma_qp_index_offset = pps.chroma_qp_index_offset as i8;

        let second_chroma_qp_index_offset = pps
            .extension
            .as_ref()
            .map(|ext| ext.second_chroma_qp_index_offset as i8)
            .unwrap_or(chroma_qp_index_offset);

        let scaling_list: Option<&mut H264ScalingLists> = pps
            .extension
            .as_ref()
            .and_then(|e| e.pic_scaling_matrix.as_ref())
            .map(|matrix| Box::leak(Box::new(matrix.into())));

        let pScalingLists = match scaling_list.as_ref() {
            Some(l) => &l.list,
            None => std::ptr::null(),
        };

        let scaling_list_ptr = scaling_list.map(NonNull::from);

        Self {
            pps: vk::native::StdVideoH264PictureParameterSet {
                flags,
                seq_parameter_set_id: pps.seq_parameter_set_id.id(),
                pic_parameter_set_id: pps.pic_parameter_set_id.id(),
                num_ref_idx_l0_default_active_minus1: pps.num_ref_idx_l0_default_active_minus1
                    as u8,
                num_ref_idx_l1_default_active_minus1: pps.num_ref_idx_l1_default_active_minus1
                    as u8,
                weighted_bipred_idc: pps.weighted_bipred_idc.into(),
                pic_init_qp_minus26: pps.pic_init_qp_minus26 as i8,
                pic_init_qs_minus26: pps.pic_init_qs_minus26 as i8,
                chroma_qp_index_offset,
                second_chroma_qp_index_offset,
                pScalingLists,
            },
            scaling_list_ptr,
        }
    }
}

impl Drop for VkPictureParameterSet {
    fn drop(&mut self) {
        self.scaling_list_ptr
            .map(|p| unsafe { Box::from_raw(p.as_ptr()) });
    }
}
