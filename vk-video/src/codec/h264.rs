use ash::vk;

use crate::codec::Codec;

pub(crate) struct H264Codec;
pub(crate) struct H264Parameters<'a> {
    pub(crate) sps: &'a [vk::native::StdVideoH264SequenceParameterSet],
    pub(crate) pps: &'a [vk::native::StdVideoH264PictureParameterSet],
}

impl Codec for H264Codec {
    // Parameters
    type InitialParameters<'a> = H264Parameters<'a>;

    type VideoDecodeSessionParametersAddInfo<'a> =
        vk::VideoDecodeH264SessionParametersAddInfoKHR<'a>;
    type VideoDecodeSessionParametersCreateInfo<'a> =
        vk::VideoDecodeH264SessionParametersCreateInfoKHR<'a>;

    type VideoEncodeSessionParametersAddInfo<'a> =
        vk::VideoEncodeH264SessionParametersAddInfoKHR<'a>;
    type VideoEncodeSessionParametersCreateInfo<'a> =
        vk::VideoEncodeH264SessionParametersCreateInfoKHR<'a>;

    fn decode_parameters_add_info<'a: 'b, 'b>(
        parameters: &Self::InitialParameters<'a>,
    ) -> Self::VideoDecodeSessionParametersAddInfo<'b> {
        vk::VideoDecodeH264SessionParametersAddInfoKHR::default()
            .std_sp_ss(parameters.sps)
            .std_pp_ss(parameters.pps)
    }

    fn decode_parameters_create_info<'a: 'b, 'b>(
        add_info: &'b Self::VideoDecodeSessionParametersAddInfo<'a>,
    ) -> Self::VideoDecodeSessionParametersCreateInfo<'b> {
        vk::VideoDecodeH264SessionParametersCreateInfoKHR::default()
            .max_std_sps_count(32)
            .max_std_pps_count(32)
            .parameters_add_info(add_info)
    }

    fn encode_parameters_add_info<'a: 'b, 'b>(
        parameters: &Self::InitialParameters<'a>,
    ) -> Self::VideoEncodeSessionParametersAddInfo<'b> {
        vk::VideoEncodeH264SessionParametersAddInfoKHR::default()
            .std_sp_ss(parameters.sps)
            .std_pp_ss(parameters.pps)
    }

    fn encode_parameters_create_info<'a: 'b, 'b>(
        add_info: &'b Self::VideoEncodeSessionParametersAddInfo<'a>,
    ) -> Self::VideoEncodeSessionParametersCreateInfo<'b> {
        vk::VideoEncodeH264SessionParametersCreateInfoKHR::default()
            .max_std_sps_count(32)
            .max_std_pps_count(32)
            .parameters_add_info(add_info)
    }
}
