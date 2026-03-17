use ash::vk;

pub(crate) mod h264;
pub(crate) mod h265;

pub(crate) trait Codec: CodecCapabilities + std::fmt::Debug + Clone {
    // Parameters
    type InitialParameters<'a>;

    type VideoDecodeSessionParametersAddInfo<'a>;
    type VideoDecodeSessionParametersCreateInfo<'a>: vk::ExtendsVideoSessionParametersCreateInfoKHR;

    type VideoEncodeSessionParametersAddInfo<'a>;
    type VideoEncodeSessionParametersCreateInfo<'a>: vk::ExtendsVideoSessionParametersCreateInfoKHR;

    fn decode_parameters_add_info<'a: 'b, 'b>(
        parameters: &Self::InitialParameters<'a>,
    ) -> Self::VideoDecodeSessionParametersAddInfo<'b>;
    fn decode_parameters_create_info<'a: 'b, 'b>(
        add_info: &'b Self::VideoDecodeSessionParametersAddInfo<'a>,
    ) -> Self::VideoDecodeSessionParametersCreateInfo<'b>;

    fn encode_parameters_add_info<'a: 'b, 'b>(
        parameters: &Self::InitialParameters<'a>,
    ) -> Self::VideoEncodeSessionParametersAddInfo<'b>;
    fn encode_parameters_create_info<'a: 'b, 'b>(
        add_info: &'b Self::VideoEncodeSessionParametersAddInfo<'a>,
    ) -> Self::VideoEncodeSessionParametersCreateInfo<'b>;
}

pub(crate) trait CodecCapabilities: std::fmt::Debug + Clone {
    type CodecSpecificDecodeCapabilities<'a>: CodecSpecificDecodeCapabilities;
    type CodecSpecificEncodeCapabilities<'a>: CodecSpecificEncodeCapabilities;
    type CodecSpecificEncodeQualityLevelProperties<'a>: CodecSpecificEncoderQualityLevelProperties;

    fn static_decode_capabilities<'a>(
        codec_caps: &Self::CodecSpecificDecodeCapabilities<'a>,
    ) -> Self::CodecSpecificDecodeCapabilities<'static>;
    fn static_encode_capabilities<'a>(
        codec_caps: &Self::CodecSpecificEncodeCapabilities<'a>,
    ) -> Self::CodecSpecificEncodeCapabilities<'static>;
    fn static_encode_qlp<'a>(
        codec_qlp: &Self::CodecSpecificEncodeQualityLevelProperties<'a>,
    ) -> Self::CodecSpecificEncodeQualityLevelProperties<'static>;
}

pub(crate) trait CodecSpecificDecodeCapabilities:
    std::fmt::Debug + Clone + Default + vk::ExtendsVideoCapabilitiesKHR
{
}

pub(crate) trait CodecSpecificEncodeCapabilities:
    std::fmt::Debug + Clone + Default + vk::ExtendsVideoCapabilitiesKHR
{
}

pub(crate) trait CodecSpecificEncoderQualityLevelProperties:
    std::fmt::Debug + Clone + Default + vk::ExtendsVideoEncodeQualityLevelPropertiesKHR
{
    fn zeroed(&self) -> bool;
}
