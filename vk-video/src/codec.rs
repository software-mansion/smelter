use ash::vk;

pub(crate) mod h264;

pub(crate) trait Codec {
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
