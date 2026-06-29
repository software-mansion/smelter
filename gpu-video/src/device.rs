use std::num::NonZeroU32;
use std::sync::Arc;

use crate::capabilities::{DecodeCapabilities, EncodeCapabilities};
use crate::parameters::{
    EncoderContentFlags, EncoderTuningMode, EncoderUsageFlags, H264Profile, H265Profile,
    RateControl,
};
use crate::{
    BytesDecoder, BytesEncoderH264, BytesEncoderH265, VideoDecoderError, VideoEncoderError,
};

#[cfg(feature = "wgpu")]
pub(crate) mod wgpu_api;
#[cfg(feature = "wgpu")]
pub use wgpu_api::*;

/// Describes a [`VideoDevice`](crate::VideoDevice).
/// Used by [`VideoAdapter::create_device`]
#[derive(Default, Clone)]
pub struct VideoDeviceDescriptor {
    #[cfg(feature = "wgpu")]
    pub wgpu_features: wgpu::Features,

    #[cfg(feature = "wgpu")]
    pub wgpu_experimental_features: wgpu::ExperimentalFeatures,

    #[cfg(feature = "wgpu")]
    pub wgpu_limits: wgpu::Limits,
}

/// A fraction
#[derive(Debug, Clone, Copy)]
pub struct Rational {
    pub numerator: u32,
    pub denominator: NonZeroU32,
}

impl From<u32> for Rational {
    fn from(value: u32) -> Self {
        Rational {
            numerator: value,
            denominator: std::num::NonZeroU32::new(1).unwrap(),
        }
    }
}

/// An enum used to specify how the decoder should handle missing frames
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissedFrameHandling {
    /// When missed frames are detected, error on every subsequent frame that depends on them
    /// (i. e. fail on every frame until an IDR frame arrives)
    #[default]
    Strict,

    /// When missed frames are detected, try to decode later frames that depend on them anyway.
    /// This can produce decoded frames with very visible artifacts.
    Tolerant,
}

/// Parameters for decoder creation
#[derive(Debug, Default, Clone, Copy)]
pub struct DecoderParameters {
    /// See [`MissedFrameHandling`] for description of different handling approaches.
    ///
    /// **Defaults to [`MissedFrameHandling::Strict`]**
    pub missed_frame_handling: MissedFrameHandling,

    /// A hint indicating what kind of content the decoder is going to be used for.
    ///
    /// Multiple flags can be combined using the `|` operator to indicate multiple usages.
    pub usage_flags: crate::parameters::DecoderUsageFlags,
}

/// Things the encoder needs to know about the video
#[derive(Debug, Clone, Copy)]
pub struct VideoParameters {
    pub width: NonZeroU32,
    pub height: NonZeroU32,
    /// The expected/approximate framerate of the encoded video
    pub target_framerate: Rational,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ColorSpace {
    #[default]
    Unspecified,
    BT709,
    BT601Ntsc,
    BT601Pal,
}

impl From<&h264_reader::nal::sps::SeqParameterSet> for ColorSpace {
    fn from(sps: &h264_reader::nal::sps::SeqParameterSet) -> Self {
        let Some(vui) = &sps.vui_parameters else {
            return ColorSpace::Unspecified;
        };
        let Some(vst) = &vui.video_signal_type else {
            return ColorSpace::Unspecified;
        };
        let Some(cd) = &vst.colour_description else {
            return ColorSpace::Unspecified;
        };

        match (
            cd.colour_primaries,
            cd.transfer_characteristics,
            cd.matrix_coefficients,
        ) {
            (1, 1, 1) => ColorSpace::BT709,
            (6, 6, 6) => ColorSpace::BT601Ntsc,
            (5, 6, 5) => ColorSpace::BT601Pal,
            _ => ColorSpace::Unspecified,
        }
    }
}

/// Whether the video signal uses the full or limited range of sample values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorRange {
    /// Luma and chroma use the full [0, 255] range.
    Full,
    /// Luma is restricted to [16, 235] and chroma to [16, 240].
    Limited,
}

impl From<&h264_reader::nal::sps::SeqParameterSet> for ColorRange {
    fn from(sps: &h264_reader::nal::sps::SeqParameterSet) -> Self {
        sps.vui_parameters
            .as_ref()
            .and_then(|v| v.video_signal_type.as_ref())
            .map(|vst| {
                if vst.video_full_range_flag {
                    ColorRange::Full
                } else {
                    ColorRange::Limited
                }
            })
            .unwrap_or(ColorRange::Limited)
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CodecColorDescription {
    pub colour_primaries: u8,
    pub transfer_characteristics: u8,
    pub matrix_coefficients: u8,
}

impl From<ColorSpace> for CodecColorDescription {
    fn from(color_space: ColorSpace) -> Self {
        match color_space {
            ColorSpace::Unspecified => Self {
                colour_primaries: 2,
                transfer_characteristics: 2,
                matrix_coefficients: 2,
            },
            ColorSpace::BT709 => Self {
                colour_primaries: 1,
                transfer_characteristics: 1,
                matrix_coefficients: 1,
            },
            ColorSpace::BT601Ntsc => Self {
                colour_primaries: 6,
                transfer_characteristics: 6,
                matrix_coefficients: 6,
            },
            ColorSpace::BT601Pal => Self {
                colour_primaries: 5,
                transfer_characteristics: 6,
                matrix_coefficients: 5,
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum EncoderPreset {
    /// A hint for the encoder to prioritize speed over quality
    Speed,
    /// A hint for the encoder to prioritize quality over speed
    Quality,
}

/// Parameters that describe an encoded output.
#[derive(Debug, Clone, Copy)]
pub struct EncoderOutputParameters<P> {
    /// Number of frames between IDRs. If [`None`], this will be set to an encoder preferred value,
    /// or, if the encoder doesn't provide a preferred value, to 30.
    pub idr_period: Option<NonZeroU32>,
    /// See [`RateControl`] for description of different rate control modes. The selected mode must
    /// be supported by the device.
    pub rate_control: RateControl,
    /// Max number of references a P-frame can have. This value will be clamped to the max number the
    /// GPU supports. If [`None`], this value will be set to the max value supported by the device.
    pub max_references: Option<NonZeroU32>,
    /// The profile must be supported by the device
    pub profile: P,
    /// A hint indicating what the encoder should prioritize.
    pub preset: EncoderPreset,
    /// A hint indicating what the encoded content is going to be used for.
    ///
    /// Multiple flags can be combined using the `|` operator to indicate multiple usages.
    pub usage_flags: Option<EncoderUsageFlags>,
    /// A hint indicating how to tune the encoder implementation.
    pub tuning_mode: Option<EncoderTuningMode>,
    /// A hint indicating what kind of content the encoder is going to be used for.
    ///
    /// Multiple flags can be combined using the `|` operator to indicate multiple usages.
    pub content_flags: Option<EncoderContentFlags>,
    /// Whether to prepend SPS/PPS NAL units inline before IDR frames.
    /// If `false`, SPS/PPS can be retrieved separately using methods defined on the encoder.
    /// If [`None`], defaults to `true`.
    pub inline_stream_params: Option<bool>,
    /// Color space of the encoded output.
    /// If [`None`], defaults to [`ColorSpace::Unspecified`].
    pub color_space: Option<ColorSpace>,
    /// Color range of the encoded output.
    /// If [`None`], defaults to [`ColorRange::Limited`].
    pub color_range: Option<ColorRange>,
}

/// Parameters for H.264 encoder creation
#[derive(Debug, Clone, Copy)]
pub struct EncoderParametersH264 {
    pub input_parameters: VideoParameters,
    pub output_parameters: EncoderOutputParameters<H264Profile>,
}

/// Parameters for H.265 encoder creation
#[derive(Debug, Clone, Copy)]
pub struct EncoderParametersH265 {
    pub input_parameters: VideoParameters,
    pub output_parameters: EncoderOutputParameters<H265Profile>,
}

pub(crate) trait VideoDeviceBackend: Send + Sync {
    fn create_bytes_decoder_h264(
        self: Arc<Self>,
        parameters: DecoderParameters,
    ) -> Result<BytesDecoder, VideoDecoderError>;

    fn create_bytes_encoder_h264(
        self: Arc<Self>,
        parameters: EncoderParametersH264,
    ) -> Result<BytesEncoderH264, VideoEncoderError>;

    fn create_bytes_encoder_h265(
        self: Arc<Self>,
        parameters: EncoderParametersH265,
    ) -> Result<BytesEncoderH265, VideoEncoderError>;

    #[cfg(feature = "wgpu")]
    fn create_wgpu_textures_decoder_h264(
        self: Arc<Self>,
        wgpu_device: wgpu::Device,
        parameters: DecoderParameters,
    ) -> Result<crate::WgpuTexturesDecoder, VideoDecoderError>;

    #[cfg(feature = "wgpu")]
    fn create_wgpu_textures_encoder_h264(
        self: Arc<Self>,
        wgpu_device: wgpu::Device,
        wgpu_queue: wgpu::Queue,
        parameters: EncoderParametersH264,
    ) -> Result<crate::WgpuTexturesEncoderH264, VideoEncoderError>;

    #[cfg(feature = "wgpu")]
    fn create_wgpu_textures_encoder_h265(
        self: Arc<Self>,
        wgpu_device: wgpu::Device,
        wgpu_queue: wgpu::Queue,
        parameters: EncoderParametersH265,
    ) -> Result<crate::WgpuTexturesEncoderH265, VideoEncoderError>;

    #[cfg(feature = "transcoder")]
    fn create_transcoder(
        self: Arc<Self>,
        parameters: crate::parameters::TranscoderParameters,
    ) -> Result<crate::vulkan_transcoder::Transcoder, crate::vulkan_transcoder::VideoTranscoderError>;

    fn decode_capabilities(&self) -> DecodeCapabilities;

    fn encode_capabilities(&self) -> EncodeCapabilities;
}
