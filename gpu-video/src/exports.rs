pub mod parameters {
    pub use crate::adapter::VideoAdapterDescriptor;
    pub use crate::device::{
        ColorRange, ColorSpace, DecoderParameters, EncoderOutputParameters, EncoderParametersH264,
        EncoderParametersH265, MissedFrameHandling, Rational, VideoDeviceDescriptor,
        VideoParameters,
    };
    pub use crate::instance::VideoInstanceDescriptor;

    pub type EncoderOutputParametersH264 = crate::device::EncoderOutputParameters<H264Profile>;

    #[cfg(feature = "transcoder")]
    pub use crate::transcoder::{
        AnyEncoderParameters, TranscoderOutputParameters, TranscoderParameters,
    };

    #[cfg(feature = "wgpu")]
    pub use crate::wgpu_helpers::WgpuConverterParameters;

    /// The rate control algorithm to be used by the encoder.
    ///
    /// Note: `EncoderDefault` is not a good default! For most implementations it is the same as
    /// specifying `Disabled`.
    ///
    /// For most use cases, `Vbr` is the correct option
    #[derive(Debug, Clone, Copy)]
    pub enum RateControl {
        /// Use the default setting of the encoder implementation.
        EncoderDefault,

        /// Variable bitrate rate control. This setting fits most use cases. The encoder will try to
        /// keep the bitrate around the average, but may increase it temporarily up to the max when
        /// necessary, in `virtual_buffer_size`-length windows. Bitrate is measured in bits/second.
        VariableBitrate {
            average_bitrate: u64,
            max_bitrate: u64,
            virtual_buffer_size: std::time::Duration,
        },

        /// Constant bitrate rate control. This setting is for environments that are more
        /// bandwidth-constrained. The encoder will keep the bitrate at the specified value, in
        /// `virtual_buffer_size`-length windows. Bitrate is measured in bits/second.
        ConstantBitrate {
            bitrate: u64,
            virtual_buffer_size: std::time::Duration,
        },

        /// Rate control is turned off, frames are compressed with a constant rate. A more complicated
        /// frame will just be bigger.
        Disabled,
    }

    /// A hint indicating what kind of content the decoder is going to be used for.
    #[derive(Debug, Clone, Copy, Default)]
    pub enum DecoderUsage {
        #[default]
        Default,
        Transcoding,
        Offline,
        Streaming,
    }

    /// A hint indicating what kind of content the encoder is going to be used for.
    #[derive(Debug, Clone, Copy, Default)]
    pub enum EncoderUsage {
        #[default]
        Default,
        Transcoding,
        Streaming,
        Recording,
        Conferencing,
    }

    /// A hint indicating what the encoder should prioritize.
    #[derive(Debug, Clone, Copy)]
    pub enum EncoderPreset {
        HighQuality,
        Balanced,
        LowLatency,
    }

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

    /// A profile in H.265 is a set of codec features used while encoding a specific video.
    /// Right now, only Main is available.
    #[derive(Debug, Clone, Copy)]
    pub enum H265Profile {
        Main,
    }
}

#[cfg(feature = "wgpu")]
mod wgpu_api;
#[cfg(feature = "wgpu")]
pub use wgpu_api::*;

use crate::capabilities::{DecodeCapabilities, EncodeCapabilities};
use crate::device::{
    ColorRange, ColorSpace, DecoderParameters, EncoderOutputParameters, EncoderParametersH264,
    EncoderParametersH265, VideoDeviceBackend,
};
use crate::parameters::{H264Profile, H265Profile, RateControl};
use crate::parser::h264::AccessUnit;
use std::sync::{Arc, Mutex};

#[cfg(feature = "wgpu")]
pub use crate::{
    adapter::VideoAdapterExt,
    device::VideoDeviceExt,
    global_registry::RegistryError,
    wgpu_helpers::{WgpuConverterInitError, WgpuNv12ToRgbaConverter, WgpuRgbaToNv12Converter},
};

pub use crate::adapter::VideoAdapter;
#[cfg(feature = "wgpu")]
pub use crate::decoders::WgpuTexturesDecoderH264;
pub use crate::decoders::{BytesDecoderH264, VideoDecoderError};
pub use crate::encoders::{BytesEncoderH264, BytesEncoderH265, VideoEncoderError};
#[cfg(feature = "wgpu")]
pub use crate::encoders::{WgpuTexturesEncoderH264, WgpuTexturesEncoderH265};
pub use crate::instance::VideoInstance;
pub use crate::parser::{h264::H264ParserError, reference_manager::ReferenceManagementError};
#[cfg(feature = "transcoder")]
pub use crate::transcoder::{VideoTranscoder, VideoTranscoderError};

#[derive(thiserror::Error, Debug)]
#[error("{message}")]
pub struct VideoBackendError {
    pub message: String,
    #[source]
    pub source: Box<dyn std::error::Error + Send + Sync + 'static>,
}

#[derive(thiserror::Error, Debug)]
pub enum VideoInstanceInitError {
    #[error("Cannot find a suitable adapters for a video device")]
    NoAdapterFound,

    #[error("Instance error: {0}")]
    BackendError(VideoBackendError),
}

#[derive(thiserror::Error, Debug)]
pub enum VideoDeviceInitError {
    #[error("The chosen adapter is not suitable for a video device")]
    NotSuitableAdapter,

    #[error("Device error: {0}")]
    BackendError(VideoBackendError),
}

/// Open connection to a coding-capable device
#[derive(Clone)]
pub struct VideoDevice {
    pub(crate) inner: Arc<dyn VideoDeviceBackend>,

    #[cfg(feature = "wgpu")]
    pub(crate) wgpu_device: Option<wgpu::Device>,
}

impl std::fmt::Debug for VideoDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VideoDevice").finish()
    }
}

impl VideoDevice {
    pub fn create_bytes_decoder_h264<F>(
        &self,
        parameters: DecoderParameters,
        on_frame: F,
    ) -> Result<BytesDecoderH264, VideoDecoderError>
    where
        F: FnMut(Result<OutputFrame<RawFrameData>, VideoDecoderError>) + Send + 'static,
    {
        self.inner
            .clone()
            .create_bytes_decoder_h264(parameters, Arc::new(Mutex::new(on_frame)))
    }

    #[cfg(feature = "wgpu")]
    pub fn create_wgpu_textures_decoder_h264<F>(
        &self,
        wgpu_queue: &wgpu::Queue,
        parameters: DecoderParameters,
        on_frame: F,
    ) -> Result<WgpuTexturesDecoderH264, VideoDecoderError>
    where
        F: FnMut(Result<OutputFrame<wgpu::Texture>, VideoDecoderError>) + Send + 'static,
    {
        let Some(wgpu_device) = self.wgpu_device.clone() else {
            return Err(VideoDecoderError::VideoDeviceWithoutWgpu);
        };

        self.inner.clone().create_wgpu_textures_decoder_h264(
            wgpu_device,
            wgpu_queue.clone(),
            parameters,
            Arc::new(Mutex::new(on_frame)),
        )
    }

    /// Create a single-input multiple-output transcoder.
    /// Each item in `parameters.output_parameters` corresponds to one output.
    #[cfg(feature = "transcoder")]
    pub fn create_transcoder(
        &self,
        parameters: crate::parameters::TranscoderParameters,
    ) -> Result<crate::transcoder::VideoTranscoder, crate::transcoder::VideoTranscoderError> {
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
    ) -> Result<WgpuTexturesEncoderH264, VideoEncoderError> {
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
    ) -> Result<WgpuTexturesEncoderH265, VideoEncoderError> {
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
            preset: parameters::EncoderPreset::LowLatency,
            usage_flags: Some(parameters::EncoderUsage::Default),
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
            preset: parameters::EncoderPreset::HighQuality,
            usage_flags: Some(parameters::EncoderUsage::Default),
            inline_stream_params: None,
            color_space: None,
            color_range: None,
        }
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
#[derive(Clone)]
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
