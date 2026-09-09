use std::{any::Any, ops::Deref};

use crate::{
    InputFrame, VideoEncoderError,
    encoders::{VideoEncoderParametersInfoH264, VideoEncoderParametersInfoH265},
};

pub(crate) trait EncodeTextureBackend: Any + Send {}

// TODO: should this also be used by bytes decoders?
pub struct EncodeTexture {
    pub(crate) wgpu_texture: wgpu::Texture,
    pub(crate) backend_texture: Box<dyn EncodeTextureBackend>,
}

// TODO: should implement deref?
impl Deref for EncodeTexture {
    type Target = wgpu::Texture;

    fn deref(&self) -> &Self::Target {
        &self.wgpu_texture
    }
}

pub(crate) trait WgpuVideoEncoderBackend: Send {
    fn encode_texture(
        &mut self,
        wgpu_device: &wgpu::Device,
        wgpu_queue: &wgpu::Queue,
        // TODO: Maybe the encode texture should return InputFrame?
        frame: InputFrame<EncodeTexture>,
        force_idr: bool,
    ) -> Result<(), VideoEncoderError>;

    fn flush(&mut self) -> Result<(), VideoEncoderError>;

    fn next_input_texture(
        &mut self,
        wgpu_device: &wgpu::Device,
    ) -> Result<EncodeTexture, VideoEncoderError>;
}

pub(crate) trait WgpuVideoEncoderBackendH264:
    WgpuVideoEncoderBackend + VideoEncoderParametersInfoH264
{
}
impl<E: WgpuVideoEncoderBackend + VideoEncoderParametersInfoH264> WgpuVideoEncoderBackendH264
    for E
{
}

pub(crate) trait WgpuVideoEncoderBackendH265:
    WgpuVideoEncoderBackend + VideoEncoderParametersInfoH265
{
}
impl<E: WgpuVideoEncoderBackend + VideoEncoderParametersInfoH265> WgpuVideoEncoderBackendH265
    for E
{
}

/// An H.264 (AVC) encoder that takes input frames as [`wgpu::Texture`]s (in [`wgpu::TextureFormat::NV12`])
pub struct WgpuTexturesEncoderH264 {
    pub(crate) wgpu_device: wgpu::Device,
    pub(crate) wgpu_queue: wgpu::Queue,
    pub(crate) backend: Box<dyn WgpuVideoEncoderBackendH264>,
}

impl WgpuTexturesEncoderH264 {
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
        frame: InputFrame<EncodeTexture>,
        force_keyframe: bool,
    ) -> Result<(), VideoEncoderError> {
        self.backend
            .encode_texture(&self.wgpu_device, &self.wgpu_queue, frame, force_keyframe)
    }

    /// Flush all chunks from the encoder.
    /// This blocks until all chunks have been sent via the provided callback.
    ///
    /// Calling this from within the provided callback can lead to a deadlock.
    pub fn flush(&mut self) -> Result<(), VideoEncoderError> {
        self.backend.flush()
    }

    /// Retrieve encoded SPS NAL units from the video session parameters, in Annex B.
    ///
    /// Useful when `inline_stream_params` is `false` and the parameters need to be
    /// sent out-of-band (e.g. in RTMP or MP4 headers).
    pub fn sps(&self) -> Result<Vec<u8>, VideoEncoderError> {
        self.backend.sps()
    }

    /// Retrieve encoded PPS NAL units from the video session parameters, in Annex B.
    ///
    /// Useful when `inline_stream_params` is `false` and the parameters need to be
    /// sent out-of-band (e.g. in RTMP or MP4 headers).
    pub fn pps(&self) -> Result<Vec<u8>, VideoEncoderError> {
        self.backend.pps()
    }

    // TODO: docs
    pub fn input_texture(&mut self) -> Result<EncodeTexture, VideoEncoderError> {
        self.backend.next_input_texture(&self.wgpu_device)
    }
}

/// An H.265 (HEVC) encoder that takes input frames as [`wgpu::Texture`]s (in [`wgpu::TextureFormat::NV12`])
pub struct WgpuTexturesEncoderH265 {
    pub(crate) wgpu_device: wgpu::Device,
    pub(crate) wgpu_queue: wgpu::Queue,
    pub(crate) backend: Box<dyn WgpuVideoEncoderBackendH265>,
}

impl WgpuTexturesEncoderH265 {
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
        frame: InputFrame<EncodeTexture>,
        force_keyframe: bool,
    ) -> Result<(), VideoEncoderError> {
        self.backend
            .encode_texture(&self.wgpu_device, &self.wgpu_queue, frame, force_keyframe)
    }

    /// Flush all chunks from the encoder.
    /// This blocks until all chunks have been sent via the provided callback.
    ///
    /// Calling this from within the provided callback can lead to a deadlock.
    pub fn flush(&mut self) -> Result<(), VideoEncoderError> {
        self.backend.flush()
    }

    /// Retrieve encoded VPS NAL units from the video session parameters, in Annex B.
    ///
    /// Useful when `inline_stream_params` is `false` and the parameters need to be
    /// sent out-of-band (e.g. in RTMP or MP4 headers).
    pub fn vps(&self) -> Result<Vec<u8>, VideoEncoderError> {
        self.backend.vps()
    }

    /// Retrieve encoded SPS NAL units from the video session parameters, in Annex B.
    ///
    /// Useful when `inline_stream_params` is `false` and the parameters need to be
    /// sent out-of-band (e.g. in RTMP or MP4 headers).
    pub fn sps(&self) -> Result<Vec<u8>, VideoEncoderError> {
        self.backend.sps()
    }

    /// Retrieve encoded PPS NAL units from the video session parameters, in Annex B.
    ///
    /// Useful when `inline_stream_params` is `false` and the parameters need to be
    /// sent out-of-band (e.g. in RTMP or MP4 headers).
    pub fn pps(&self) -> Result<Vec<u8>, VideoEncoderError> {
        self.backend.pps()
    }

    // TODO: docs
    pub fn input_texture(&mut self) -> Result<EncodeTexture, VideoEncoderError> {
        self.backend.next_input_texture(&self.wgpu_device)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WgpuTextureEncoderError {
    #[error("The supplied texture's format is {0:?}, when it should be NV12")]
    NotNV12Texture(wgpu::TextureFormat),

    #[error("The supplied texture does not have COPY_SRC usage. Texture's usages: {0:?}")]
    NoCopySrcTextureUsage(wgpu::TextureUsages),

    #[error(
        "The dimensions of the provided frame ({provided_dimensions:?}) are not the same as the expected dimensions ({expected_dimensions:?})"
    )]
    InconsistentPictureDimensions {
        provided_dimensions: wgpu::Extent3d,
        expected_dimensions: wgpu::Extent3d,
    },

    #[error("Wgpu device error: {0}")]
    WgpuDeviceError(#[from] wgpu::hal::DeviceError),
}
