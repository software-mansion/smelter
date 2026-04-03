use ash::vk;

use crate::VulkanDecoderError;

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
