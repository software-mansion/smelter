use crate::{EncodedOutputChunk, InputFrame, RawFrameData, VideoBackendError};

#[cfg(feature = "wgpu")]
mod wgpu_api;
#[cfg(feature = "wgpu")]
pub use wgpu_api::*;

pub(crate) trait VideoEncoderBackend: Send {
    fn encode_bytes(
        &mut self,
        frame: &InputFrame<RawFrameData>,
        force_idr: bool,
    ) -> Result<EncodedOutputChunk<Vec<u8>>, VideoEncoderError>;
}

pub(crate) trait VideoEncoderParametersInfoH264 {
    fn sps(&self) -> Result<Vec<u8>, VideoEncoderError>;
    fn pps(&self) -> Result<Vec<u8>, VideoEncoderError>;
}

pub(crate) trait VideoEncoderParametersInfoH265 {
    fn vps(&self) -> Result<Vec<u8>, VideoEncoderError>;
    fn sps(&self) -> Result<Vec<u8>, VideoEncoderError>;
    fn pps(&self) -> Result<Vec<u8>, VideoEncoderError>;
}

pub(crate) trait VideoEncoderBackendH264:
    VideoEncoderBackend + VideoEncoderParametersInfoH264
{
}
impl<E: VideoEncoderBackend + VideoEncoderParametersInfoH264> VideoEncoderBackendH264 for E {}

pub(crate) trait VideoEncoderBackendH265:
    VideoEncoderBackend + VideoEncoderParametersInfoH265
{
}
impl<E: VideoEncoderBackend + VideoEncoderParametersInfoH265> VideoEncoderBackendH265 for E {}

/// An H.264 (AVC) encoder that takes input frames as [`Vec<u8>`] with raw pixel data (in NV12)
pub struct BytesEncoderH264 {
    pub(crate) encoder: Box<dyn VideoEncoderBackendH264>,
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
        self.encoder.encode_bytes(frame, force_keyframe)
    }

    /// Retrieve encoded SPS NAL units from the video session parameters, in Annex B.
    ///
    /// Useful when `inline_stream_params` is `false` and the parameters need to be
    /// sent out-of-band (e.g. in RTMP or MP4 headers).
    pub fn sps(&self) -> Result<Vec<u8>, VideoEncoderError> {
        self.encoder.sps()
    }

    /// Retrieve encoded PPS NAL units from the video session parameters, in Annex B.
    ///
    /// Useful when `inline_stream_params` is `false` and the parameters need to be
    /// sent out-of-band (e.g. in RTMP or MP4 headers).
    pub fn pps(&self) -> Result<Vec<u8>, VideoEncoderError> {
        self.encoder.pps()
    }
}

/// An H.265 (HEVC) encoder that takes input frames as [`Vec<u8>`] with raw pixel data (in NV12)
pub struct BytesEncoderH265 {
    pub(crate) encoder: Box<dyn VideoEncoderBackendH265>,
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
        self.encoder.encode_bytes(frame, force_keyframe)
    }

    /// Retrieve encoded VPS NAL units from the video session parameters, in Annex B.
    ///
    /// Useful when `inline_stream_params` is `false` and the parameters need to be
    /// sent out-of-band (e.g. in RTMP or MP4 headers).
    pub fn vps(&self) -> Result<Vec<u8>, VideoEncoderError> {
        self.encoder.vps()
    }

    /// Retrieve encoded SPS NAL units from the video session parameters, in Annex B.
    ///
    /// Useful when `inline_stream_params` is `false` and the parameters need to be
    /// sent out-of-band (e.g. in RTMP or MP4 headers).
    pub fn sps(&self) -> Result<Vec<u8>, VideoEncoderError> {
        self.encoder.sps()
    }

    /// Retrieve encoded PPS NAL units from the video session parameters, in Annex B.
    ///
    /// Useful when `inline_stream_params` is `false` and the parameters need to be
    /// sent out-of-band (e.g. in RTMP or MP4 headers).
    pub fn pps(&self) -> Result<Vec<u8>, VideoEncoderError> {
        self.encoder.pps()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum VideoEncoderError {
    #[error("The device does not support encoding")]
    EncoderUnsupported,

    #[error("The profile '{0}' is not supported by this device")]
    ProfileUnsupported(String),

    #[cfg(feature = "wgpu")]
    #[error(
        "VideoDevice was created without wgpu support. Initialize wgpu::Device using VideoAdapterExt::request_device_with_video_support"
    )]
    VideoDeviceWithoutWgpu,

    #[error("Invalid encoder parameters, field: {field} - problem: {problem}")]
    ParametersError {
        field: &'static str,
        problem: String,
    },

    #[error(
        "The byte length of the provided frame ({bytes}) is not the same as the picture size calculated from the dimensions ({size_from_resolution})"
    )]
    InconsistentPictureByteSize {
        bytes: usize,
        size_from_resolution: usize,
    },

    #[cfg(feature = "wgpu")]
    #[error(transparent)]
    WgpuTextureEncoderError(#[from] WgpuTextureEncoderError),

    #[error("Encoder error: {0}")]
    BackendError(VideoBackendError),
}
