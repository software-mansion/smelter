use crate::{InputFrame, RawFrameData, VideoBackendError};

#[cfg(feature = "wgpu")]
mod wgpu_api;
#[cfg(feature = "wgpu")]
pub use wgpu_api::*;

pub(crate) trait VideoEncoderBackend: Send {
    fn encode_bytes(
        &mut self,
        frame: &InputFrame<RawFrameData>,
        force_idr: bool,
    ) -> Result<(), VideoEncoderError>;

    fn flush(&mut self) -> Result<(), VideoEncoderError>;
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
    /// Encode a frame. The resulting chunks of H264 bitstream are sent via the callback provided
    /// at encoder creation.
    ///
    /// If the `force_keyframe` option is set to `true`, the encoder will encode this frame as a
    /// [keyframe](https://en.wikipedia.org/wiki/Video_compression_picture_types#Intra-coded_(I)_frames/slices_(key_frames)).
    /// Otherwise, the encoder will decide which frames should be coded this way.
    ///
    /// If [`EncoderOutputParameters::max_in_flight_submissions`](crate::parameters::EncoderOutputParameters::max_in_flight_submissions)
    /// encode submissions are already in flight, this blocks until all submissions above the limit finish.
    ///
    /// Calling this from within the provided callback can lead to a deadlock.
    pub fn encode(
        &mut self,
        frame: &InputFrame<RawFrameData>,
        force_keyframe: bool,
    ) -> Result<(), VideoEncoderError> {
        self.encoder.encode_bytes(frame, force_keyframe)
    }

    /// Flush all chunks from the encoder.
    /// This blocks until all chunks have been sent via the provided callback.
    ///
    /// Calling this from within the provided callback can lead to a deadlock.
    pub fn flush(&mut self) -> Result<(), VideoEncoderError> {
        self.encoder.flush()
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
    /// Encode a frame. The resulting chunks of H265 bitstream are sent via the callback provided
    /// at encoder creation.
    ///
    /// If the `force_keyframe` option is set to `true`, the encoder will encode this frame as a
    /// [keyframe](https://en.wikipedia.org/wiki/Video_compression_picture_types#Intra-coded_(I)_frames/slices_(key_frames)).
    /// Otherwise, the encoder will decide which frames should be coded this way.
    ///
    /// If [`EncoderOutputParameters::max_in_flight_submissions`](crate::parameters::EncoderOutputParameters::max_in_flight_submissions)
    /// encode submissions are already in flight, this blocks until all submissions above the limit finish.
    ///
    /// Calling this from within the provided callback can lead to a deadlock.
    pub fn encode(
        &mut self,
        frame: &InputFrame<RawFrameData>,
        force_keyframe: bool,
    ) -> Result<(), VideoEncoderError> {
        self.encoder.encode_bytes(frame, force_keyframe)
    }

    /// Flush all chunks from the encoder.
    /// This blocks until all chunks have been sent via the provided callback.
    ///
    /// Calling this from within the provided callback can lead to a deadlock.
    pub fn flush(&mut self) -> Result<(), VideoEncoderError> {
        self.encoder.flush()
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
