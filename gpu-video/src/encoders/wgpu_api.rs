use std::time::Duration;

use crate::{
    InputFrame, VideoEncoderError,
    encoders::{VideoEncoderParametersInfoH264, VideoEncoderParametersInfoH265},
};

/// Input texture for a wgpu textures encoder.
///
/// [`EncodeTexture::texture`] returns an NV12 [`wgpu::Texture`] with [`wgpu::TextureUsages::COPY_DST`], [`wgpu::TextureUsages::RENDER_ATTACHMENT`]
/// and [`wgpu::TextureUsages::STORAGE_BINDING`] usages. The input data for encoding should be written to the [`wgpu::Texture`].
/// Each frame needs to obtain a new [`EncodeTexture`] from the encoder.
///
/// The [`wgpu::Texture`] is not safe to use after the texture was submitted to the encoder.
pub struct EncodeTexture {
    pub(crate) wgpu_texture: wgpu::Texture,
    pub(crate) on_drop: Option<Box<dyn FnOnce()>>,
}

impl Drop for EncodeTexture {
    fn drop(&mut self) {
        let Some(on_drop) = self.on_drop.take() else {
            return;
        };

        (on_drop)();
    }
}

impl EncodeTexture {
    pub fn texture(&self) -> &wgpu::Texture {
        &self.wgpu_texture
    }
}

pub(crate) trait WgpuVideoEncoderBackend: Send {
    fn encode_texture(
        &mut self,
        wgpu_device: &wgpu::Device,
        wgpu_queue: &wgpu::Queue,
        frame: InputFrame<EncodeTexture>,
        force_idr: bool,
        timeout: Duration,
    ) -> Result<(), VideoEncoderError>;

    fn flush(&mut self, timeout: Duration) -> Result<(), VideoEncoderError>;

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
    /// Encodes a frame represented as [`EncodeTexture`]. Each frame needs to obtain its own [`EncodeTexture`] by calling [`WgpuTexturesEncoderH264::input_texture`].
    /// The resulting chunks of H264 bitstream are sent via the callback provided at encoder creation.
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
        self.encode_timeout(frame, force_keyframe, Duration::MAX)
    }

    /// Same as [`Self::encode`], but if [`EncoderOutputParameters::max_in_flight_submissions`](crate::parameters::EncoderOutputParameters::max_in_flight_submissions)
    /// encode submissions are already in flight, this blocks until all submissions above the limit finish,
    /// or times out after `timeout`.
    pub fn encode_timeout(
        &mut self,
        frame: InputFrame<EncodeTexture>,
        force_keyframe: bool,
        timeout: Duration,
    ) -> Result<(), VideoEncoderError> {
        self.backend.encode_texture(
            &self.wgpu_device,
            &self.wgpu_queue,
            frame,
            force_keyframe,
            timeout,
        )
    }

    /// Flush all chunks from the encoder.
    /// This blocks until all chunks have been sent via the provided callback.
    ///
    /// Calling this from within the provided callback can lead to a deadlock.
    pub fn flush(&mut self) -> Result<(), VideoEncoderError> {
        self.flush_timeout(Duration::MAX)
    }

    /// Flush all chunks from the encoder.
    /// This blocks until all chunks have been sent via the provided callback, or times out after `timeout`.
    pub fn flush_timeout(&mut self, timeout: Duration) -> Result<(), VideoEncoderError> {
        self.backend.flush(timeout)
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

    /// Returns a texture for writing the next input frame.
    ///
    /// Do not use the texture after submitting it to the encoder.
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
    /// Encodes a frame represented as [`EncodeTexture`]. Each frame needs to obtain its own [`EncodeTexture`] by calling [`WgpuTexturesEncoderH265::input_texture`].
    /// The resulting chunks of H264 bitstream are sent via the callback provided at encoder creation.
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
        self.encode_timeout(frame, force_keyframe, Duration::MAX)
    }

    /// Same as [`Self::encode`], but if [`EncoderOutputParameters::max_in_flight_submissions`](crate::parameters::EncoderOutputParameters::max_in_flight_submissions)
    /// encode submissions are already in flight, this blocks until all submissions above the limit finish,
    /// or times out after `timeout`.
    pub fn encode_timeout(
        &mut self,
        frame: InputFrame<EncodeTexture>,
        force_keyframe: bool,
        timeout: Duration,
    ) -> Result<(), VideoEncoderError> {
        self.backend.encode_texture(
            &self.wgpu_device,
            &self.wgpu_queue,
            frame,
            force_keyframe,
            timeout,
        )
    }

    /// Flush all chunks from the encoder.
    /// This blocks until all chunks have been sent via the provided callback.
    ///
    /// Calling this from within the provided callback can lead to a deadlock.
    pub fn flush(&mut self) -> Result<(), VideoEncoderError> {
        self.flush_timeout(Duration::MAX)
    }

    /// Flush all chunks from the encoder.
    /// This blocks until all chunks have been sent via the provided callback, or times out after `timeout`.
    pub fn flush_timeout(&mut self, timeout: Duration) -> Result<(), VideoEncoderError> {
        self.backend.flush(timeout)
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

    /// Returns a texture for writing the next input frame.
    ///
    /// Do not use the texture after submitting it to the encoder.
    pub fn input_texture(&mut self) -> Result<EncodeTexture, VideoEncoderError> {
        self.backend.next_input_texture(&self.wgpu_device)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WgpuTextureEncoderError {
    #[error("The supplied texture's format is {0:?}, when it should be NV12")]
    NotNV12Texture(wgpu::TextureFormat),

    #[error("The supplied texture was not obtained from this encoder's input_texture()")]
    TextureNotFromEncoder,

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
