use crate::{ EncodedOutputChunk, InputFrame, RawFrameData, VideoBackendError};

pub(crate) trait VideoEncoderBackend {
    fn encode_bytes(
        &mut self,
        frame: &InputFrame<RawFrameData>,
        force_idr: bool,
    ) -> Result<EncodedOutputChunk<Vec<u8>>, VideoEncoderError>;

    #[cfg(feature = "wgpu")]
    fn encode_texture(
        &mut self,
        wgpu_device: &wgpu::Device,
        wgpu_queue: &wgpu::Queue,
        frame: InputFrame<wgpu::Texture>,
        force_idr: bool,
    ) -> Result<EncodedOutputChunk<Vec<u8>>, VideoEncoderError>;
}

pub(crate) trait H264VideoEncoderBackend: VideoEncoderBackend {
    fn sps(&self) -> Result<Vec<u8>, VideoEncoderError>;
    fn pps(&self) -> Result<Vec<u8>, VideoEncoderError>;
}

pub(crate) trait H265VideoEncoderBackend: VideoEncoderBackend {
    fn vps(&self) -> Result<Vec<u8>, VideoEncoderError>;
    fn sps(&self) -> Result<Vec<u8>, VideoEncoderError>;
    fn pps(&self) -> Result<Vec<u8>, VideoEncoderError>;
}

/// An H.264 (AVC) encoder that takes input frames as [`Vec<u8>`] with raw pixel data (in NV12)
pub struct BytesEncoderH264 {
    pub(crate) encoder: Box<dyn H264VideoEncoderBackend>,
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
    pub(crate) encoder: Box<dyn H265VideoEncoderBackend>,
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

/// An H.264 (AVC) encoder that takes input frames as [`wgpu::Texture`]s (in [`wgpu::TextureFormat::NV12`])
#[cfg(feature = "wgpu")]
pub struct WgpuTexturesEncoderH264 {
    pub(crate) wgpu_device: wgpu::Device,
    pub(crate) wgpu_queue: wgpu::Queue,
    pub(crate) encoder: Box<dyn H264VideoEncoderBackend>,
}

#[cfg(feature = "wgpu")]
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
        self.encoder
            .encode_texture(&self.wgpu_device, &self.wgpu_queue, frame, force_keyframe)
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

/// An H.265 (HEVC) encoder that takes input frames as [`wgpu::Texture`]s (in [`wgpu::TextureFormat::NV12`])
#[cfg(feature = "wgpu")]
pub struct WgpuTexturesEncoderH265 {
    pub(crate) wgpu_device: wgpu::Device,
    pub(crate) wgpu_queue: wgpu::Queue,
    pub(crate) encoder: Box<dyn H265VideoEncoderBackend>,
}

#[cfg(feature = "wgpu")]
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
        self.encoder
            .encode_texture(&self.wgpu_device, &self.wgpu_queue, frame, force_keyframe)
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

// TODO: it's not clear to me which things should be here and what should be in backend error
// maybe wrap it and include it in vulkan_encoder_error
// TODO: move it to encoders/
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

#[cfg(feature = "wgpu")]
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
