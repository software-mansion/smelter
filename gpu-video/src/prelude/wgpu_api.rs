use crate::{
    DecoderEvent, EncodedInputChunk, EncodedOutputChunk, InputFrame, OutputFrame,
    VideoDecoderError, VideoEncoderError,
    codec::{
        h264::{H264Codec, encode::H264WriteParametersInfo},
        h265::{H265Codec, encode::H265WriteParametersInfo},
    },
    frame_sorter::FrameSorter,
    parser::{
        decoder_instructions::compile_to_decoder_instructions,
        h264::{AccessUnit, H264Parser},
        reference_manager::ReferenceContext,
    },
    vulkan::{vulkan_decoder::VulkanDecoder, vulkan_encoder::VulkanEncoder},
};

/// An H.265 (HEVC) encoder that takes input frames as [`wgpu::Texture`]s (in [`wgpu::TextureFormat::NV12`])
pub struct WgpuTexturesEncoderH265 {
    pub(crate) wgpu_device: wgpu::Device,
    pub(crate) wgpu_queue: wgpu::Queue,
    pub(crate) vulkan_encoder: VulkanEncoder<'static, H265Codec>,
}

impl WgpuTexturesEncoderH265 {
    /// The result is a chunk of H265 bitstream.
    ///
    /// If the `force_keyframe` option is set to `true`, the encoder will encode this frame as a
    /// [keyframe](https://en.wikipedia.org/wiki/Video_compression_picture_types#Intra-coded_(I)_frames/slices_(key_frames)).
    /// Otherwise, the encoder will decide which frames should be coded this way.
    pub fn encode(
        &mut self,
        frame: InputFrame<wgpu::Texture>,
        force_keyframe: bool,
    ) -> Result<EncodedOutputChunk<Vec<u8>>, VideoEncoderError> {
        self.vulkan_encoder.encode_texture(
            &self.wgpu_device,
            &self.wgpu_queue,
            frame,
            force_keyframe,
        )
    }

    /// Retrieve encoded VPS NAL units from the video session parameters, in Annex B.
    ///
    /// Useful when `inline_stream_params` is `false` and the parameters need to be
    /// sent out-of-band (e.g. in RTMP or MP4 headers).
    pub fn vps(&self) -> Result<Vec<u8>, VideoEncoderError> {
        self.vulkan_encoder
            .stream_parameters(H265WriteParametersInfo {
                write_vps: true,
                write_sps: false,
                write_pps: false,
            })
    }

    /// Retrieve encoded SPS NAL units from the video session parameters, in Annex B.
    ///
    /// Useful when `inline_stream_params` is `false` and the parameters need to be
    /// sent out-of-band (e.g. in RTMP or MP4 headers).
    pub fn sps(&self) -> Result<Vec<u8>, VideoEncoderError> {
        self.vulkan_encoder
            .stream_parameters(H265WriteParametersInfo {
                write_vps: false,
                write_sps: true,
                write_pps: false,
            })
    }

    /// Retrieve encoded PPS NAL units from the video session parameters, in Annex B.
    ///
    /// Useful when `inline_stream_params` is `false` and the parameters need to be
    /// sent out-of-band (e.g. in RTMP or MP4 headers).
    pub fn pps(&self) -> Result<Vec<u8>, VideoEncoderError> {
        self.vulkan_encoder
            .stream_parameters(H265WriteParametersInfo {
                write_vps: false,
                write_sps: false,
                write_pps: true,
            })
    }
}

/// An H.264 (AVC) encoder that takes input frames as [`wgpu::Texture`]s (in [`wgpu::TextureFormat::NV12`])
pub struct WgpuTexturesEncoderH264 {
    pub(crate) wgpu_device: wgpu::Device,
    pub(crate) wgpu_queue: wgpu::Queue,
    pub(crate) vulkan_encoder: VulkanEncoder<'static, H264Codec>,
}

impl WgpuTexturesEncoderH264 {
    /// The result is a chunk of H264 bitstream.
    ///
    /// If the `force_keyframe` option is set to `true`, the encoder will encode this frame as a
    /// [keyframe](https://en.wikipedia.org/wiki/Video_compression_picture_types#Intra-coded_(I)_frames/slices_(key_frames)).
    /// Otherwise, the encoder will decide which frames should be coded this way.
    pub fn encode(
        &mut self,
        frame: InputFrame<wgpu::Texture>,
        force_keyframe: bool,
    ) -> Result<EncodedOutputChunk<Vec<u8>>, VideoEncoderError> {
        self.vulkan_encoder.encode_texture(
            &self.wgpu_device,
            &self.wgpu_queue,
            frame,
            force_keyframe,
        )
    }

    /// Retrieve encoded SPS NAL units from the video session parameters, in Annex B.
    ///
    /// Useful when `inline_stream_params` is `false` and the parameters need to be
    /// sent out-of-band (e.g. in RTMP or MP4 headers).
    pub fn sps(&self) -> Result<Vec<u8>, VideoEncoderError> {
        self.vulkan_encoder
            .stream_parameters(H264WriteParametersInfo {
                write_sps: true,
                write_pps: false,
            })
    }

    /// Retrieve encoded PPS NAL units from the video session parameters, in Annex B.
    ///
    /// Useful when `inline_stream_params` is `false` and the parameters need to be
    /// sent out-of-band (e.g. in RTMP or MP4 headers).
    pub fn pps(&self) -> Result<Vec<u8>, VideoEncoderError> {
        self.vulkan_encoder
            .stream_parameters(H264WriteParametersInfo {
                write_sps: false,
                write_pps: true,
            })
    }
}

#[derive(thiserror::Error, Debug)]
pub enum WgpuInitError {
    #[error("Wgpu instance error: {0}")]
    WgpuInstanceError(#[from] wgpu::hal::InstanceError),

    #[error("Wgpu device error: {0}")]
    WgpuDeviceError(#[from] wgpu::hal::DeviceError),

    #[error("Wgpu request device error: {0}")]
    WgpuRequestDeviceError(#[from] wgpu::RequestDeviceError),

    #[error("Cannot create a wgpu adapter")]
    WgpuAdapterNotCreated,
}
