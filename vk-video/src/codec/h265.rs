use std::ptr::null_mut;

use ash::vk;

use crate::codec::{
    CodecCapabilities, CodecSpecificDecodeCapabilities, CodecSpecificEncodeCapabilities,
    CodecSpecificEncoderQualityLevelProperties,
};

#[derive(Debug, Clone)]
pub(crate) struct H265Codec;

impl CodecCapabilities for H265Codec {
    type CodecSpecificDecodeCapabilities<'a> = vk::VideoDecodeH265CapabilitiesKHR<'a>;
    type CodecSpecificEncodeCapabilities<'a> = vk::VideoEncodeH265CapabilitiesKHR<'a>;
    type CodecSpecificEncodeQualityLevelProperties<'a> =
        vk::VideoEncodeH265QualityLevelPropertiesKHR<'a>;

    fn static_decode_capabilities<'a>(
        codec_caps: &Self::CodecSpecificDecodeCapabilities<'a>,
    ) -> Self::CodecSpecificDecodeCapabilities<'static> {
        vk::VideoDecodeH265CapabilitiesKHR {
            p_next: null_mut(),
            _marker: Default::default(),
            ..*codec_caps
        }
    }

    fn static_encode_capabilities<'a>(
        codec_caps: &Self::CodecSpecificEncodeCapabilities<'a>,
    ) -> Self::CodecSpecificEncodeCapabilities<'static> {
        vk::VideoEncodeH265CapabilitiesKHR {
            p_next: null_mut(),
            _marker: Default::default(),
            ..*codec_caps
        }
    }

    fn static_encode_qlp<'a>(
        codec_qlp: &Self::CodecSpecificEncodeQualityLevelProperties<'a>,
    ) -> Self::CodecSpecificEncodeQualityLevelProperties<'static> {
        vk::VideoEncodeH265QualityLevelPropertiesKHR {
            p_next: null_mut(),
            _marker: Default::default(),
            ..*codec_qlp
        }
    }
}

impl<'a> CodecSpecificDecodeCapabilities for vk::VideoDecodeH265CapabilitiesKHR<'a> {}
impl<'a> CodecSpecificEncodeCapabilities for vk::VideoEncodeH265CapabilitiesKHR<'a> {}
impl<'a> CodecSpecificEncoderQualityLevelProperties
    for vk::VideoEncodeH265QualityLevelPropertiesKHR<'a>
{
    fn zeroed(&self) -> bool {
        self.preferred_rate_control_flags.as_raw() == 0
            && self.preferred_gop_frame_count == 0
            && self.preferred_idr_period == 0
            && self.preferred_consecutive_b_frame_count == 0
            && self.preferred_sub_layer_count == 0
            && self.preferred_constant_qp.qp_i == 0
            && self.preferred_constant_qp.qp_p == 0
            && self.preferred_constant_qp.qp_b == 0
            && self.preferred_max_l0_reference_count == 0
            && self.preferred_max_l1_reference_count == 0
    }
}
