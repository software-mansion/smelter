use smelter_render::{
    InputId, OutputId,
    error::{InitRendererEngineError, UpdateSceneError},
};

use crate::{graphics_context::CreateGraphicsContextError, prelude::*};

#[derive(Debug, Clone, Copy)]
pub enum ErrorSeverity {
    /// Unrecoverable failure of some element, e.g. for output
    /// it means that output fully stopped/disconnected
    Critical,

    /// Significant issue with user-facing impact (e.g., artifacts, dropped frames).
    /// The system remains operational and is expected to recover automatically.
    Transient,

    // Incorrect behavior that should be investigated, but did not
    // cause any user-facing effects.
    Warning,
}

impl std::fmt::Display for ErrorSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorSeverity::Critical => "critical".fmt(f),
            ErrorSeverity::Transient => "transient".fmt(f),
            ErrorSeverity::Warning => "warning".fmt(f),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum InitPipelineError {
    #[error(transparent)]
    InitRendererEngine(#[from] InitRendererEngineError),

    #[error(transparent)]
    CreateGraphicsContext(#[from] CreateGraphicsContextError),

    #[error("Failed to create a download directory.")]
    CreateDownloadDir(#[source] std::io::Error),

    #[error("Side channel socket directory error: {0}")]
    SideChannelSocketDir(String),

    #[error("Failed to create tokio::Runtime.")]
    CreateTokioRuntime(#[source] std::io::Error),

    #[error("Failed to initialize WHIP WHEP server.")]
    WhipWhepServerInitError(#[source] std::io::Error),

    #[error("Failed to initialize RTMP server.")]
    RtmpServerInitError(#[source] std::io::Error),

    #[error("Failed to initialize MoQ server: {0}")]
    MoqServerInitError(String),

    #[error("Failed to set up self-signed MoQ TLS certificate.")]
    MoqSelfSignedTlsError(#[from] SelfSignedTlsError),

    #[error("Failed to bind UDP socket for WebRTC mux on port {0}.")]
    BindUdpMuxSocket(u16, #[source] std::io::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum RegisterInputError {
    #[error("Failed to register input stream. Stream \"{0}\" is already registered.")]
    AlreadyRegistered(InputId),

    #[error("Input initialization error while registering input for stream \"{0}\".")]
    InputError(InputId, #[source] InputInitError),
}

#[derive(Debug, thiserror::Error)]
pub enum RegisterOutputError {
    #[error("Failed to register output stream. Stream \"{0}\" is already registered.")]
    AlreadyRegistered(OutputId),

    #[error("Output initialization error while registering output for stream \"{0}\".")]
    OutputError(OutputId, #[source] OutputInitError),

    #[error("Failed to initialize the scene when registering output \"{0}\".")]
    SceneError(OutputId, #[source] UpdateSceneError),

    #[error(
        "Failed to register output stream \"{0}\". At least one of \"video\" and \"audio\" must be specified."
    )]
    NoVideoAndAudio(OutputId),
}

#[derive(Debug, thiserror::Error)]
pub enum UpdateInputError {
    #[error("Input \"{0}\" not found.")]
    NotFound(InputId),

    #[error("Seek is not supported for {0} input. Only MP4 inputs support seeking.")]
    SeekNotSupported(InputProtocolKind),

    #[error("Pausing is not supported for {0} input. Only MP4 inputs support pausing.")]
    PausingNotSupported(InputProtocolKind),
}

#[derive(Debug, thiserror::Error)]
pub enum UnregisterInputError {
    #[error("Failed to unregister input stream. Stream \"{0}\" does not exist.")]
    NotFound(InputId),
}

#[derive(Debug, thiserror::Error)]
pub enum UnregisterOutputError {
    #[error("Failed to unregister output stream. Stream \"{0}\" does not exist.")]
    NotFound(OutputId),
}

#[derive(Debug, thiserror::Error)]
pub enum OutputInitError {
    #[error("Failed to initialize encoder.")]
    EncoderError(#[from] EncoderInitError),

    #[error("An unsupported video codec was requested: {0:?}.")]
    UnsupportedVideoCodec(VideoCodec),

    #[error("An unsupported audio codec was requested: {0:?}.")]
    UnsupportedAudioCodec(AudioCodec),

    #[error(transparent)]
    SocketError(#[from] std::io::Error),

    #[error("Failed to register output. Port: {0} is already used or not available.")]
    PortAlreadyInUse(u16),

    #[error(
        "Failed to register output. All ports in range {lower_bound} to {upper_bound} are already used or not available."
    )]
    AllPortsAlreadyInUse { lower_bound: u16, upper_bound: u16 },

    #[error("Failed to register output. FFmpeg error: {0}.")]
    FfmpegError(ffmpeg_next::Error),

    #[error("Unknown WHIP output error.")]
    UnknownWhipError,

    #[error("WHIP init timeout exceeded")]
    WhipInitTimeout,

    #[error("Failed to init WHIP output")]
    WhipInitError(#[source] Box<WebrtcClientError>),

    #[error("WHIP WHEP server is not running, cannot start WHEP output")]
    WhipWhepServerNotRunning,

    #[error(transparent)]
    RtmpError(#[from] RtmpClientError),

    #[error(transparent)]
    MoqClientError(#[from] MoqClientError),
}

/// Error that can happen after registration
#[derive(Debug, thiserror::Error, Clone)]
pub enum OutputRuntimeError {
    #[error(transparent)]
    Mp4(#[from] OutputMp4RuntimeError),

    #[error(transparent)]
    Whip(#[from] OutputWhipRuntimeError),
}

/// Error that can happen after registration
#[derive(Debug, thiserror::Error, Clone)]
pub enum OutputWhipRuntimeError {
    #[error("Peer connection disconnected.")]
    PeerConnectionDisconnected,
}

/// Error that can happen after registration
#[derive(Debug, thiserror::Error, Clone)]
pub enum OutputMp4RuntimeError {
    #[error("Failed to write packet to mp4 file.")]
    PacketWriteError(#[source] ffmpeg_next::Error),

    #[error("Failed to write MP4 header")]
    TrailerWriteError(#[source] ffmpeg_next::Error),

    /// If this error is returned it is most likely a bug.
    #[error("Internal error: {0}")]
    InternalError(String),

    #[error("No space left on device")]
    NoSpaceLeftOnDevice,
}

#[derive(Debug, thiserror::Error)]
pub enum EncoderInitError {
    #[error("Could not find an ffmpeg codec")]
    NoCodec,

    #[error(transparent)]
    FfmpegError(#[from] ffmpeg_next::Error),

    #[error(transparent)]
    OpusError(#[from] opus::Error),

    #[error("Internal FDK AAC encoder error: {0}")]
    AacError(fdk_aac_sys::AACENC_ERROR),

    #[error(transparent)]
    ResamplerError(#[from] rubato::ResamplerConstructionError),

    #[cfg(feature = "gpu-video")]
    #[error(transparent)]
    VulkanEncoderError(#[from] gpu_video::VideoEncoderError),

    #[error(
        "Pipeline couldn't detect a vulkan video compatible device when it was being initialized. Cannot create a vulkan video encoder"
    )]
    VulkanContextRequiredForVulkanEncoder,
}

#[derive(Debug, thiserror::Error)]
pub enum InputInitError {
    #[error(transparent)]
    Rtp(#[from] RtpInputError),

    #[error(transparent)]
    Mp4(#[from] Mp4InputError),

    #[error(transparent)]
    Whip(#[from] WebrtcServerError),

    #[error(transparent)]
    Whep(#[from] Box<WebrtcClientError>),

    #[error(transparent)]
    Rtmp(#[from] RtmpServerError),

    #[error(transparent)]
    MoqServer(#[from] MoqServerError),

    #[error(transparent)]
    MoqClient(#[from] MoqClientError),

    #[cfg(feature = "decklink")]
    #[error(transparent)]
    DeckLink(#[from] DeckLinkInputError),

    #[error(transparent)]
    FfmpegError(#[from] ffmpeg_next::Error),

    #[error(transparent)]
    ResamplerError(#[from] rubato::ResamplerConstructionError),

    #[error(transparent)]
    V4l2Error(#[from] V4l2InputError),

    #[error("Failed to initialize decoder.")]
    DecoderError(#[from] DecoderInitError),

    #[error("Invalid video decoder provided. Expected {expected:?} decoder")]
    InvalidVideoDecoderProvided { expected: VideoCodec },

    #[error("Internal Server Error: {0}")]
    InternalServerError(&'static str),
}

#[derive(Debug, thiserror::Error)]
pub enum DecoderInitError {
    #[cfg(feature = "gpu-video")]
    #[error(transparent)]
    VulkanDecoderError(#[from] gpu_video::VideoDecoderError),

    #[error(
        "Pipeline couldn't detect a vulkan video compatible device when it was being initialized. Cannot create a vulkan video decoder"
    )]
    VulkanContextRequiredForVulkanDecoder,

    #[error(transparent)]
    OpusError(#[from] opus::Error),

    #[error(transparent)]
    AacError(#[from] FdkAacDecoderError),

    #[error(transparent)]
    FfmpegError(#[from] ffmpeg_next::Error),
}
