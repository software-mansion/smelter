pub mod capabilities {
    pub use crate::adapter::VideoAdapterInfo;
    pub use crate::device::caps::{
        DecodeCapabilities, DecodeH264Capabilities, DecodeH264ProfileCapabilities,
        DecodeH265Capabilities, DecodeH265ProfileCapabilities, EncodeCapabilities,
        EncodeH264Capabilities, EncodeH265Capabilities, EncodeProfileCapabilities,
    };

    pub use ash::vk::PhysicalDeviceType as VulkanDeviceType;
}

pub mod parameters {
    pub use crate::adapter::VideoAdapterDescriptor;
    pub use crate::device::{
        ColorRange, ColorSpace, DecoderParameters, EncoderOutputParameters, EncoderParametersH264,
        EncoderParametersH265, MissedFrameHandling, Rational, VideoDeviceDescriptor,
        VideoParameters,
    };
    pub use crate::instance::VideoInstanceDescriptor;

    pub type EncoderOutputParametersH264 = crate::device::EncoderOutputParameters<H264Profile>;

    pub use crate::vulkan_encoder::RateControl;
    #[cfg(feature = "transcoder")]
    pub use crate::vulkan_transcoder::{
        AnyEncoderParameters, TranscoderOutputParameters, TranscoderParameters,
    };

    #[cfg(feature = "wgpu")]
    pub use crate::wgpu_helpers::WgpuConverterParameters;

    pub use ash::vk::VideoDecodeUsageFlagsKHR as DecoderUsageFlags;

    pub use ash::vk::VideoEncodeContentFlagsKHR as EncoderContentFlags;
    pub use ash::vk::VideoEncodeTuningModeKHR as EncoderTuningMode;
    pub use ash::vk::VideoEncodeUsageFlagsKHR as EncoderUsageFlags;

    /// Scaling algorithm used when resizing frames in the transcoder.
    #[derive(Debug, Clone, Copy, Default)]
    #[repr(u32)]
    pub enum ScalingAlgorithm {
        NearestNeighbor,
        #[default]
        Bilinear,
        Lanczos3,
    }

    /// A profile in H.264 is a set of codec features used while encoding a specific video.
    /// Baseline uses the fewest features, Main can use more and High even more than Main.
    #[derive(Debug, Clone, Copy)]
    pub enum H264Profile {
        Baseline,
        Main,
        High,
    }

    impl H264Profile {
        pub(crate) fn to_profile_idc(self) -> ash::vk::native::StdVideoH264ProfileIdc {
            match self {
                H264Profile::Baseline => {
                    ash::vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_BASELINE
                }
                H264Profile::Main => {
                    ash::vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_MAIN
                }
                H264Profile::High => {
                    ash::vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_HIGH
                }
            }
        }
    }

    /// A profile in H.265 is a set of codec features used while encoding a specific video.
    /// Right now, only Main is available.
    #[derive(Debug, Clone, Copy)]
    pub enum H265Profile {
        Main,
    }

    impl H265Profile {
        pub(crate) fn to_profile_idc(self) -> ash::vk::native::StdVideoH265ProfileIdc {
            match self {
                H265Profile::Main => {
                    ash::vk::native::StdVideoH265ProfileIdc_STD_VIDEO_H265_PROFILE_IDC_MAIN
                }
            }
        }
    }
}

#[cfg(feature = "wgpu")]
mod wgpu_api;
#[cfg(feature = "wgpu")]
pub use wgpu_api::*;

use crate::capabilities::{DecodeCapabilities, EncodeCapabilities};
use crate::codec::h264::H264Codec;
use crate::codec::h264::encode::H264WriteParametersInfo;
use crate::codec::h265::H265Codec;
use crate::codec::h265::encode::H265WriteParametersInfo;
use crate::device::{
    ColorRange, ColorSpace, DecoderParameters, EncoderOutputParameters, EncoderParametersH264,
    EncoderParametersH265,
};
use crate::parameters::{H264Profile, H265Profile, RateControl};
use crate::parser::h264::AccessUnit;
use crate::vulkan_decoder::{FrameSorter, ImageModifiers, VulkanDecoder};
use ash::vk;
use std::sync::Arc;

#[cfg(feature = "wgpu")]
pub use crate::adapter::VideoAdapterExt;

pub use crate::adapter::VideoAdapter;
pub use crate::device::VideoDeviceExt;
pub use crate::global_registry::RegistryError;
pub use crate::instance::VideoInstance;
pub use crate::parser::{h264::H264ParserError, reference_manager::ReferenceManagementError};
pub use crate::vulkan_decoder::VulkanDecoderError;
pub use crate::vulkan_encoder::VideoEncoderError;
#[cfg(feature = "transcoder")]
pub use crate::vulkan_transcoder::{Transcoder, VideoTranscoderError};

#[cfg(feature = "wgpu")]
pub use crate::wgpu_helpers::{
    WgpuConverterInitError, WgpuNv12ToRgbaConverter, WgpuRgbaToNv12Converter,
};

use crate::parser::{
    decoder_instructions::compile_to_decoder_instructions, h264::H264Parser,
    reference_manager::ReferenceContext,
};
use crate::vulkan_encoder::VulkanEncoder;
use crate::wrappers::ImageKey;

#[derive(Debug, thiserror::Error)]
pub enum VideoDecoderError {
    #[error("Decoder error: {0}")]
    VulkanDecoderError(#[from] VulkanDecoderError),

    #[error("H264 parser error: {0}")]
    ParserError(#[from] H264ParserError),

    #[error("Reference management error: {0}")]
    ReferenceManagementError(#[from] ReferenceManagementError),

    #[cfg(feature = "wgpu")]
    #[error(
        "VideoDevice was created without wgpu support. Initialize wgpu::Device using VideoAdapterExt::request_device_with_video_support"
    )]
    VideoDeviceWithoutWgpu,
}

#[derive(thiserror::Error, Debug)]
pub enum VideoInitError {
    #[error("Error loading vulkan: {0}")]
    LoadingError(#[from] ash::LoadingError),

    #[error("Vulkan error: {0}")]
    VkError(#[from] vk::Result),

    #[error("Cannot find a suitable physical device")]
    NoDevice,

    #[error("Missing required extension: {0}")]
    MissingExtension(String),

    #[error("String conversion error: {0}")]
    StringConversionError(#[from] std::ffi::FromBytesUntilNulError),

    #[error("Profile does not support NV12 texture format")]
    NoNV12ProfileSupport,

    #[cfg(feature = "wgpu")]
    #[error(transparent)]
    WgpuError(#[from] WgpuInitError),
}

#[derive(thiserror::Error, Debug)]
pub enum VulkanCommonError {
    #[error("Vulkan error: {0}")]
    VkError(#[from] vk::Result),

    #[error("Cannot find a queue with index {0}")]
    NoQueue(usize),

    #[error("Memory copy requested to a buffer that is not set up for receiving input")]
    UploadToImproperBuffer,

    #[error("A slot in the Decoded Pictures Buffer was requested, but all slots are taken")]
    NoFreeSlotsInDpb,

    #[error("DPB can have at most 32 slots, {0} was requested")]
    DpbTooLong(u32),

    #[error("Tried to wait for an unsignaled semaphore value")]
    SemaphoreWaitOnUnsignaledValue,

    #[error("Tried to register {0:x?} as a new image, while it already exists")]
    RegisteredNewImageTwice(ImageKey),

    #[error("Tried to access state of image {0:x?}, which does not exist")]
    TriedToAccessNonexistentImageState(ImageKey),

    #[error("Tried to unregister image {0:x?} that was not registered")]
    UnregisteredNonexistentImage(ImageKey),

    #[error("Unsupported image aspect: {0:?}")]
    UnsupportedImageAspect(vk::ImageAspectFlags),
}

/// Open connection to a coding-capable device
#[derive(Debug)]
pub struct VideoDevice {
    pub(crate) inner: Arc<crate::device::VideoDevice>,

    #[cfg(feature = "wgpu")]
    pub(crate) wgpu_device: Option<wgpu::Device>,
}

impl VideoDevice {
    pub fn create_bytes_decoder_h264(
        &self,
        parameters: DecoderParameters,
    ) -> Result<BytesDecoder, VideoDecoderError> {
        let parser = H264Parser::default();
        let reference_ctx = ReferenceContext::new(parameters.missed_frame_handling);

        let vulkan_decoder = VulkanDecoder::new(
            Arc::new(self.inner.decoding_device()?),
            parameters.usage_flags,
            ImageModifiers {
                additional_queue_index: self.inner.queues.transfer.family_index,
                create_flags: Default::default(),
                usage_flags: Default::default(),
            },
        )?;
        let frame_sorter = FrameSorter::<RawFrameData>::new();

        Ok(BytesDecoder {
            parser,
            reference_ctx,
            vulkan_decoder,
            frame_sorter,
        })
    }

    #[cfg(feature = "wgpu")]
    pub fn create_wgpu_textures_decoder_h264(
        &self,
        parameters: DecoderParameters,
    ) -> Result<WgpuTexturesDecoder, VideoDecoderError> {
        let Some(wgpu_device) = self.wgpu_device.clone() else {
            return Err(VideoDecoderError::VideoDeviceWithoutWgpu);
        };

        let parser = H264Parser::default();
        let reference_ctx = ReferenceContext::new(parameters.missed_frame_handling);

        let vulkan_decoder = VulkanDecoder::new(
            Arc::new(self.inner.decoding_device()?),
            parameters.usage_flags,
            ImageModifiers {
                additional_queue_index: self.inner.queues.transfer.family_index,
                create_flags: Default::default(),
                usage_flags: Default::default(),
            },
        )?;
        let frame_sorter = FrameSorter::<wgpu::Texture>::new();

        Ok(crate::WgpuTexturesDecoder {
            wgpu_device,
            parser,
            reference_ctx,
            vulkan_decoder,
            frame_sorter,
        })
    }

    /// Create a single-input multiple-output transcoder.
    /// Each item in `parameters.output_parameters` corresponds to one output.
    #[cfg(feature = "transcoder")]
    pub fn create_transcoder(
        &self,
        parameters: crate::parameters::TranscoderParameters,
    ) -> Result<crate::vulkan_transcoder::Transcoder, crate::vulkan_transcoder::VideoTranscoderError>
    {
        crate::vulkan_transcoder::Transcoder::new(self.inner.clone(), parameters)
    }

    pub fn create_bytes_encoder_h264(
        &self,
        parameters: EncoderParametersH264,
    ) -> Result<BytesEncoderH264, VideoEncoderError> {
        let parameters = self.inner.validate_and_fill_encoder_parameters(
            parameters.output_parameters,
            parameters.input_parameters.width,
            parameters.input_parameters.height,
            parameters.input_parameters.target_framerate,
        )?;
        let encoder = VulkanEncoder::new(Arc::new(self.inner.encoding_device()?), parameters)?;

        Ok(BytesEncoderH264 {
            vulkan_encoder: encoder,
        })
    }

    pub fn create_bytes_encoder_h265(
        &self,
        parameters: EncoderParametersH265,
    ) -> Result<BytesEncoderH265, VideoEncoderError> {
        let parameters = self.inner.validate_and_fill_encoder_parameters(
            parameters.output_parameters,
            parameters.input_parameters.width,
            parameters.input_parameters.height,
            parameters.input_parameters.target_framerate,
        )?;
        let encoder = VulkanEncoder::new(Arc::new(self.inner.encoding_device()?), parameters)?;

        Ok(BytesEncoderH265 {
            vulkan_encoder: encoder,
        })
    }

    #[cfg(feature = "wgpu")]
    pub fn create_wgpu_textures_encoder_h264(
        &self,
        queue: &wgpu::Queue,
        parameters: EncoderParametersH264,
    ) -> Result<WgpuTexturesEncoderH264, VideoEncoderError> {
        let Some(wgpu_device) = self.wgpu_device.clone() else {
            return Err(VideoEncoderError::VideoDeviceWithoutWgpu);
        };

        let parameters = self.inner.validate_and_fill_encoder_parameters(
            parameters.output_parameters,
            parameters.input_parameters.width,
            parameters.input_parameters.height,
            parameters.input_parameters.target_framerate,
        )?;
        let encoder = VulkanEncoder::new(Arc::new(self.inner.encoding_device()?), parameters)?;
        Ok(crate::WgpuTexturesEncoderH264 {
            wgpu_device,
            wgpu_queue: queue.clone(),
            vulkan_encoder: encoder,
        })
    }

    #[cfg(feature = "wgpu")]
    pub fn create_wgpu_textures_encoder_h265(
        &self,
        queue: &wgpu::Queue,
        parameters: EncoderParametersH265,
    ) -> Result<WgpuTexturesEncoderH265, VideoEncoderError> {
        let Some(wgpu_device) = self.wgpu_device.clone() else {
            return Err(VideoEncoderError::VideoDeviceWithoutWgpu);
        };

        let parameters = self.inner.validate_and_fill_encoder_parameters(
            parameters.output_parameters,
            parameters.input_parameters.width,
            parameters.input_parameters.height,
            parameters.input_parameters.target_framerate,
        )?;
        let encoder = VulkanEncoder::new(Arc::new(self.inner.encoding_device()?), parameters)?;
        Ok(crate::WgpuTexturesEncoderH265 {
            wgpu_device,
            wgpu_queue: queue.clone(),
            vulkan_encoder: encoder,
        })
    }

    pub fn decode_capabilities(&self) -> DecodeCapabilities {
        self.inner.adapter_info.decode_capabilities
    }

    pub fn encode_capabilities(&self) -> EncodeCapabilities {
        self.inner.adapter_info.encode_capabilities
    }

    pub fn encoder_output_parameters_h265_low_latency(
        &self,
        rate_control: RateControl,
    ) -> Result<EncoderOutputParameters<H265Profile>, VideoEncoderError> {
        let Some(caps) = self.inner.native_encode_capabilities.as_ref() else {
            return Err(VideoEncoderError::VulkanEncoderUnsupported);
        };

        let caps = caps
            .h265
            .as_ref()
            .ok_or(VideoEncoderError::VulkanEncoderUnsupported)?;

        Ok(
            crate::device::VideoDevice::encoder_output_parameters_low_latency(
                caps.max_profile()
                    .ok_or(VideoEncoderError::VulkanEncoderUnsupported)?,
                rate_control,
            ),
        )
    }

    pub fn encoder_output_parameters_h264_low_latency(
        &self,
        rate_control: RateControl,
    ) -> Result<EncoderOutputParameters<H264Profile>, VideoEncoderError> {
        let Some(caps) = self.inner.native_encode_capabilities.as_ref() else {
            return Err(VideoEncoderError::VulkanEncoderUnsupported);
        };

        let caps = caps
            .h264
            .as_ref()
            .ok_or(VideoEncoderError::VulkanEncoderUnsupported)?;

        Ok(
            crate::device::VideoDevice::encoder_output_parameters_low_latency(
                caps.max_profile()
                    .ok_or(VideoEncoderError::VulkanEncoderUnsupported)?,
                rate_control,
            ),
        )
    }

    pub fn encoder_output_parameters_h265_high_quality(
        &self,
        rate_control: RateControl,
    ) -> Result<EncoderOutputParameters<H265Profile>, VideoEncoderError> {
        let Some(caps) = self.inner.native_encode_capabilities.as_ref() else {
            return Err(VideoEncoderError::VulkanEncoderUnsupported);
        };

        let caps = caps
            .h265
            .as_ref()
            .ok_or(VideoEncoderError::VulkanEncoderUnsupported)?;

        let quality_level = caps
            .profile(
                caps.max_profile()
                    .ok_or(VideoEncoderError::VulkanEncoderUnsupported)?,
            )
            .ok_or(VideoEncoderError::VulkanEncoderUnsupported)?
            .encode_capabilities
            .max_quality_levels
            - 1;

        Ok(
            crate::device::VideoDevice::encoder_output_parameters_high_quality(
                caps.max_profile()
                    .ok_or(VideoEncoderError::VulkanEncoderUnsupported)?,
                rate_control,
                quality_level,
            ),
        )
    }

    pub fn encoder_output_parameters_h264_high_quality(
        &self,
        rate_control: RateControl,
    ) -> Result<EncoderOutputParameters<H264Profile>, VideoEncoderError> {
        let Some(caps) = self.inner.native_encode_capabilities.as_ref() else {
            return Err(VideoEncoderError::VulkanEncoderUnsupported);
        };

        let caps = caps
            .h264
            .as_ref()
            .ok_or(VideoEncoderError::VulkanEncoderUnsupported)?;

        let quality_level = caps
            .profile(
                caps.max_profile()
                    .ok_or(VideoEncoderError::VulkanEncoderUnsupported)?,
            )
            .ok_or(VideoEncoderError::VulkanEncoderUnsupported)?
            .encode_capabilities
            .max_quality_levels
            - 1;

        Ok(
            crate::device::VideoDevice::encoder_output_parameters_high_quality(
                caps.max_profile()
                    .ok_or(VideoEncoderError::VulkanEncoderUnsupported)?,
                rate_control,
                quality_level,
            ),
        )
    }

    pub fn supports_decoding(&self) -> bool {
        self.inner.adapter_info.supports_decoding
    }

    pub fn supports_encoding(&self) -> bool {
        self.inner.adapter_info.supports_encoding
    }
}

/// Represents a chunk of encoded video data used for decoding.
///
/// `pts` is the presentation timestamp -- a number, which describes when the given frame
/// should be presented, used for synchronization with other tracks, e.g. with audio
///
/// If `pts` is [`Option::Some`], it is inferred that the chunk contains bytestream that belongs to
/// one output frame.
/// If `pts` is [`Option::None`], the chunk can contain bytestream from multiple consecutive
/// frames.
pub struct EncodedInputChunk<'a> {
    pub data: &'a [u8],
    pub pts: Option<u64>,
}

pub type H264DecoderEvent<'a> = DecoderEvent<'a, AccessUnit>;

/// Represents all events that can be sent to the decoder
#[non_exhaustive]
pub enum DecoderEvent<'a, ParsedFrame> {
    /// Submit encoded chunk for decoding
    DecodeChunk(EncodedInputChunk<'a>),

    /// Submit parsed frame for decoding
    DecodeParsedFrame(ParsedFrame),

    /// Signal the end of the current frame and flush any buffered bitstream units in the parser.
    ///
    /// You should send this event only if you need to minimize the codec parsing latency.
    /// The decoder does not require it to work.
    ///
    /// Send this only after submitting all bitstream units belonging to a single frame.
    /// Any incomplete bitstream units buffered in the parser will be flushed and decoded,
    /// which may lead to artifacts.
    SignalFrameEnd,

    /// Signal the decoder that a chunk of the bitstream was lost.
    ///
    /// What the decoder will do depends on the set [`parameters::MissedFrameHandling`]
    SignalDataLoss,

    /// Flush all frames from the decoder.
    ///
    /// Make sure that this is done when you have the knowledge that no more frames will be coming
    /// that need to be presented before the already decoded frames.
    Flush,
}

/// Represents a chunk of encoded video data returned by the encoder.
///
/// `pts` is the presentation timestamp -- a number, which describes when the given frame
/// should be presented, used for synchronization with other tracks, e.g. with audio
pub struct EncodedOutputChunk<T> {
    pub data: T,
    pub pts: Option<u64>,
    pub is_keyframe: bool,
}

/// Represents a frame to be encoded.
pub struct InputFrame<T> {
    pub data: T,
    pub pts: Option<u64>,
}

/// Additional information about the decoded frame.
pub struct FrameMetadata {
    pub pts: Option<u64>,
    pub color_space: ColorSpace,
    pub color_range: ColorRange,
}

/// Represents a single decoded frame.
pub struct OutputFrame<T> {
    pub data: T,
    pub metadata: FrameMetadata,
}

pub struct RawFrameData {
    pub frame: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// A decoder that outputs frames stored as [`Vec<u8>`] with the raw pixel data.
pub struct BytesDecoder {
    pub(crate) vulkan_decoder: VulkanDecoder<'static>,
    pub(crate) parser: H264Parser,
    pub(crate) reference_ctx: ReferenceContext,
    pub(crate) frame_sorter: FrameSorter<RawFrameData>,
}

impl BytesDecoder {
    /// The result is a sequence of frames. The payload of each [`OutputFrame`] struct is a [`Vec<u8>`]. Each [`Vec<u8>`] contains a single
    /// decoded frame in the [NV12 format](https://en.wikipedia.org/wiki/YCbCr#4:2:0).
    pub fn decode(
        &mut self,
        frame: EncodedInputChunk<'_>,
    ) -> Result<Vec<OutputFrame<RawFrameData>>, VideoDecoderError> {
        self.process_event(DecoderEvent::DecodeChunk(frame))
    }

    /// Flush all frames from the decoder.
    ///
    /// Make sure that this is done when you have the knowledge that no more frames will be coming
    /// that need to be presented before the already decoded frames.
    pub fn flush(&mut self) -> Result<Vec<OutputFrame<RawFrameData>>, VideoDecoderError> {
        self.process_event(DecoderEvent::Flush)
    }

    /// Process a [`DecoderEvent`]. For most use cases, using [`Self::decode`] and [`Self::flush`] is enough.
    /// Use this only when you need more fine-grained control.
    /// May return a sequence of decoded frames in the [NV12 format](https://en.wikipedia.org/wiki/YCbCr#4:2:0).
    pub fn process_event(
        &mut self,
        event: DecoderEvent<'_, AccessUnit>,
    ) -> Result<Vec<OutputFrame<RawFrameData>>, VideoDecoderError> {
        match event {
            DecoderEvent::DecodeChunk(chunk) => {
                let nalus = self.parser.parse(chunk.data, chunk.pts)?;
                self.decode_access_units(nalus)
            }
            DecoderEvent::DecodeParsedFrame(au) => self.decode_access_units(vec![au]),
            DecoderEvent::SignalFrameEnd => {
                let access_units = self.parser.flush()?;
                self.decode_access_units(access_units)
            }
            DecoderEvent::SignalDataLoss => {
                self.reference_ctx.mark_missed_frames();
                Ok(Vec::new())
            }
            DecoderEvent::Flush => {
                let access_units = self.parser.flush()?;
                let mut frames = self.decode_access_units(access_units)?;
                frames.append(&mut self.frame_sorter.flush());
                Ok(frames)
            }
        }
    }

    fn decode_access_units(
        &mut self,
        access_units: Vec<AccessUnit>,
    ) -> Result<Vec<OutputFrame<RawFrameData>>, VideoDecoderError> {
        let instructions = compile_to_decoder_instructions(&mut self.reference_ctx, access_units)?;
        let unsorted_frames = self.vulkan_decoder.decode_to_bytes(&instructions)?;
        let sorted_frames = self.frame_sorter.put_frames(unsorted_frames);
        Ok(sorted_frames)
    }
}

/// An H.265 (HEVC) encoder that takes input frames as [`Vec<u8>`] with raw pixel data (in NV12)
pub struct BytesEncoderH265 {
    pub(crate) vulkan_encoder: VulkanEncoder<'static, H265Codec>,
}

impl BytesEncoderH265 {
    /// The result is a chunk of H265 bitstream.
    ///
    /// If the `force_keyframe` option is set to `true`, the encoder will encode this frame as a
    /// [keyframe](https://en.wikipedia.org/wiki/Video_compression_picture_types#Intra-coded_(I)_frames/slices_(key_frames)).
    /// Otherwise, the encoder will decide which frames should be coded this way.
    pub fn encode(
        &mut self,
        frame: &InputFrame<RawFrameData>,
        force_keyframe: bool,
    ) -> Result<EncodedOutputChunk<Vec<u8>>, VideoEncoderError> {
        self.vulkan_encoder.encode_bytes(frame, force_keyframe)
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

/// An H.264 (AVC) encoder that takes input frames as [`Vec<u8>`] with raw pixel data (in NV12)
pub struct BytesEncoderH264 {
    pub(crate) vulkan_encoder: VulkanEncoder<'static, H264Codec>,
}

impl BytesEncoderH264 {
    /// The result is a chunk of H264 bitstream.
    ///
    /// If the `force_keyframe` option is set to `true`, the encoder will encode this frame as a
    /// [keyframe](https://en.wikipedia.org/wiki/Video_compression_picture_types#Intra-coded_(I)_frames/slices_(key_frames)).
    /// Otherwise, the encoder will decide which frames should be coded this way.
    pub fn encode(
        &mut self,
        frame: &InputFrame<RawFrameData>,
        force_keyframe: bool,
    ) -> Result<EncodedOutputChunk<Vec<u8>>, VideoEncoderError> {
        self.vulkan_encoder.encode_bytes(frame, force_keyframe)
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
