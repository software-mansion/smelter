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

    pub use crate::vulkan::vulkan_encoder::RateControl;
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

use crate::capabilities::{DecodeCapabilities, EncodeCapabilities};
use crate::decoder::{BytesDecoder, VideoDecoderError};
use crate::device::{
    ColorRange, ColorSpace, DecoderParameters, EncoderOutputParameters, EncoderParametersH264,
    EncoderParametersH265, EncoderPreset, VideoDeviceBackend,
};
use crate::encoder::{BytesEncoderH264, BytesEncoderH265, VideoEncoderError};
use crate::parameters::{H264Profile, H265Profile, RateControl};
use crate::parser::h264::AccessUnit;
use std::sync::Arc;

#[cfg(feature = "wgpu")]
pub use crate::{
    adapter::VideoAdapterExt,
    device::VideoDeviceExt,
    global_registry::RegistryError,
    wgpu_helpers::{WgpuConverterInitError, WgpuNv12ToRgbaConverter, WgpuRgbaToNv12Converter},
};

pub use crate::adapter::VideoAdapter;
pub use crate::instance::VideoInstance;
pub use crate::parser::{h264::H264ParserError, reference_manager::ReferenceManagementError};
pub use crate::vulkan::vulkan_decoder::VulkanDecoderError;
#[cfg(feature = "transcoder")]
pub use crate::vulkan_transcoder::{Transcoder, VideoTranscoderError};

#[derive(thiserror::Error, Debug)]
#[error("{message}")]
pub struct VideoBackendError {
    pub message: String,
    // TODO: remove option
    #[source]
    pub source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
}

#[derive(thiserror::Error, Debug)]
pub enum VideoDeviceInitError {
    #[error("The chosen adapter is not suitable for a video device")]
    NotSuitableAdapter,

    #[error(transparent)]
    BackendError(VideoBackendError),
}

#[derive(thiserror::Error, Debug)]
pub enum VideoInstanceInitError {
    #[error("Cannot find a suitable adapters for a video device")]
    NoAdapterFound,

    #[error(transparent)]
    BackendError(VideoBackendError),
}

#[cfg(feature = "wgpu")]
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

// TODO: update changelog
/// Open connection to a coding-capable device
pub struct VideoDevice {
    pub(crate) inner: Arc<dyn VideoDeviceBackend>,

    #[cfg(feature = "wgpu")]
    pub(crate) wgpu_device: Option<wgpu::Device>,
}

impl VideoDevice {
    pub fn create_bytes_decoder_h264(
        &self,
        parameters: DecoderParameters,
    ) -> Result<BytesDecoder, VideoDecoderError> {
        self.inner.clone().create_bytes_decoder_h264(parameters)
    }

    #[cfg(feature = "wgpu")]
    pub fn create_wgpu_textures_decoder_h264(
        &self,
        parameters: DecoderParameters,
    ) -> Result<crate::decoder::WgpuTexturesDecoder, VideoDecoderError> {
        let Some(wgpu_device) = self.wgpu_device.clone() else {
            return Err(VideoDecoderError::VideoDeviceWithoutWgpu);
        };

        self.inner
            .clone()
            .create_wgpu_textures_decoder_h264(wgpu_device, parameters)
    }

    /// Create a single-input multiple-output transcoder.
    /// Each item in `parameters.output_parameters` corresponds to one output.
    #[cfg(feature = "transcoder")]
    pub fn create_transcoder(
        &self,
        parameters: crate::parameters::TranscoderParameters,
    ) -> Result<crate::vulkan_transcoder::Transcoder, crate::vulkan_transcoder::VideoTranscoderError>
    {
        self.inner.clone().create_transcoder(parameters)
    }

    pub fn create_bytes_encoder_h264(
        &self,
        parameters: EncoderParametersH264,
    ) -> Result<BytesEncoderH264, VideoEncoderError> {
        self.inner.clone().create_bytes_encoder_h264(parameters)
    }

    pub fn create_bytes_encoder_h265(
        &self,
        parameters: EncoderParametersH265,
    ) -> Result<BytesEncoderH265, VideoEncoderError> {
        self.inner.clone().create_bytes_encoder_h265(parameters)
    }

    #[cfg(feature = "wgpu")]
    pub fn create_wgpu_textures_encoder_h264(
        &self,
        wgpu_queue: &wgpu::Queue,
        parameters: EncoderParametersH264,
    ) -> Result<crate::encoder::WgpuTexturesEncoderH264, VideoEncoderError> {
        let Some(wgpu_device) = self.wgpu_device.clone() else {
            return Err(VideoEncoderError::VideoDeviceWithoutWgpu);
        };

        self.inner.clone().create_wgpu_textures_encoder_h264(
            wgpu_device,
            wgpu_queue.clone(),
            parameters,
        )
    }

    #[cfg(feature = "wgpu")]
    pub fn create_wgpu_textures_encoder_h265(
        &self,
        wgpu_queue: &wgpu::Queue,
        parameters: EncoderParametersH265,
    ) -> Result<crate::encoder::WgpuTexturesEncoderH265, VideoEncoderError> {
        let Some(wgpu_device) = self.wgpu_device.clone() else {
            return Err(VideoEncoderError::VideoDeviceWithoutWgpu);
        };

        self.inner.clone().create_wgpu_textures_encoder_h265(
            wgpu_device,
            wgpu_queue.clone(),
            parameters,
        )
    }

    pub fn decode_capabilities(&self) -> DecodeCapabilities {
        self.inner.decode_capabilities()
    }

    pub fn encode_capabilities(&self) -> EncodeCapabilities {
        self.inner.encode_capabilities()
    }

    pub fn encoder_output_parameters_h265_low_latency(
        &self,
        rate_control: RateControl,
    ) -> Result<EncoderOutputParameters<H265Profile>, VideoEncoderError> {
        let Some(caps) = self.encode_capabilities().h265 else {
            return Err(VideoEncoderError::EncoderUnsupported);
        };

        Ok(Self::encoder_output_parameters_low_latency(
            caps.max_profile()
                .ok_or(VideoEncoderError::EncoderUnsupported)?,
            rate_control,
        ))
    }

    pub fn encoder_output_parameters_h264_low_latency(
        &self,
        rate_control: RateControl,
    ) -> Result<EncoderOutputParameters<H264Profile>, VideoEncoderError> {
        let Some(caps) = self.encode_capabilities().h264 else {
            return Err(VideoEncoderError::EncoderUnsupported);
        };

        Ok(Self::encoder_output_parameters_low_latency(
            caps.max_profile()
                .ok_or(VideoEncoderError::EncoderUnsupported)?,
            rate_control,
        ))
    }

    pub fn encoder_output_parameters_h265_high_quality(
        &self,
        rate_control: RateControl,
    ) -> Result<EncoderOutputParameters<H265Profile>, VideoEncoderError> {
        let Some(caps) = self.encode_capabilities().h265 else {
            return Err(VideoEncoderError::EncoderUnsupported);
        };

        Ok(Self::encoder_output_parameters_high_quality(
            caps.max_profile()
                .ok_or(VideoEncoderError::EncoderUnsupported)?,
            rate_control,
        ))
    }

    pub fn encoder_output_parameters_h264_high_quality(
        &self,
        rate_control: RateControl,
    ) -> Result<EncoderOutputParameters<H264Profile>, VideoEncoderError> {
        let Some(caps) = self.encode_capabilities().h264 else {
            return Err(VideoEncoderError::EncoderUnsupported);
        };

        Ok(Self::encoder_output_parameters_high_quality(
            caps.max_profile()
                .ok_or(VideoEncoderError::EncoderUnsupported)?,
            rate_control,
        ))
    }

    pub fn encoder_output_parameters_low_latency<P>(
        profile: P,
        rate_control: RateControl,
    ) -> EncoderOutputParameters<P> {
        EncoderOutputParameters {
            profile,
            idr_period: None,
            max_references: None,
            rate_control,
            preset: EncoderPreset::Speed,
            usage_flags: Some(parameters::EncoderUsageFlags::DEFAULT),
            content_flags: Some(parameters::EncoderContentFlags::DEFAULT),
            tuning_mode: Some(parameters::EncoderTuningMode::LOW_LATENCY),
            inline_stream_params: None,
            color_space: None,
            color_range: None,
        }
    }

    pub fn encoder_output_parameters_high_quality<P>(
        profile: P,
        rate_control: RateControl,
    ) -> EncoderOutputParameters<P> {
        EncoderOutputParameters {
            profile,
            idr_period: None,
            max_references: None,
            rate_control,
            preset: EncoderPreset::Quality,
            usage_flags: Some(parameters::EncoderUsageFlags::DEFAULT),
            content_flags: Some(parameters::EncoderContentFlags::DEFAULT),
            tuning_mode: Some(parameters::EncoderTuningMode::HIGH_QUALITY),
            inline_stream_params: None,
            color_space: None,
            color_range: None,
        }
    }
}

impl std::fmt::Debug for VideoDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VideoDevice").finish()
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
