use ash::vk;

pub(crate) mod h264;

pub(crate) trait Codec {
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

    // Capabilities
    type CodecSpecificEncodeCapabilities<'a>: CodecSpecificEncodeCapabilities;
    type CodecSpecificEncodeQualityLevelProperties<'a>: CodecSpecificEncoderQualityLevelProperties;

    fn static_codec_capabilities<'a>(
        codec_caps: &Self::CodecSpecificEncodeCapabilities<'a>,
    ) -> Self::CodecSpecificEncodeCapabilities<'static>;
    fn static_codec_qlp<'a>(
        codec_qlp: &Self::CodecSpecificEncodeQualityLevelProperties<'a>,
    ) -> Self::CodecSpecificEncodeQualityLevelProperties<'static>;
}

pub(crate) trait CodecSpecificEncodeCapabilities:
    std::fmt::Debug + Clone + Default + vk::ExtendsVideoCapabilitiesKHR
{
}

pub(crate) trait CodecSpecificEncoderQualityLevelProperties:
    std::fmt::Debug + Clone + Default + vk::ExtendsVideoEncodeQualityLevelPropertiesKHR + ToOwned
{
    fn zeroed(&self) -> bool;
}
