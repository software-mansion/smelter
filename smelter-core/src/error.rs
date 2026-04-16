use smelter_render::{
    InputId, OutputId,
    error::{
        InitRendererEngineError, RegisterError, RegisterRendererError, RequestKeyframeError,
        UnregisterRendererError, UpdateSceneError, WgpuError,
    },
};

#[cfg(feature = "vk-video")]
use vk_video::VulkanEncoderError;

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

    #[error("Failed to create tokio::Runtime.")]
    CreateTokioRuntime(#[source] std::io::Error),

    #[error("Failed to initialize WHIP WHEP server.")]
    WhipWhepServerInitError(#[source] std::io::Error),

    #[error("Failed to initialize RTMP server.")]
    RtmpServerInitError(#[source] std::io::Error),

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

    #[error(
        "Failed to register output stream \"{0}\". Resolution in each dimension has to be divisible by 2."
    )]
    UnsupportedResolution(OutputId),

    #[error("Failed to initialize the scene when registering output \"{0}\".")]
    SceneError(OutputId, #[source] UpdateSceneError),

    #[error(
        "Failed to register output stream \"{0}\". At least one of \"video\" and \"audio\" must be specified."
    )]
    NoVideoAndAudio(OutputId),

    #[error("Unknown error: {0}")]
    UnknownError(String),
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

    #[error(
        "Failed to unregister input stream. Stream \"{0}\" is still used in the current scene."
    )]
    StillInUse(InputId),
}

#[derive(Debug, thiserror::Error)]
pub enum UnregisterOutputError {
    #[error("Failed to unregister output stream. Stream \"{0}\" does not exist.")]
    NotFound(OutputId),

    #[error(
        "Failed to unregister output stream. Stream \"{0}\" is still used in the current scene."
    )]
    StillInUse(OutputId),
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
}

/// Error that can happen after registration
#[derive(Debug, thiserror::Error, Clone)]
pub enum OutputRuntimeError {
    #[error(transparent)]
    Mp4(#[from] OutputMp4RuntimeError),
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

    #[cfg(feature = "vk-video")]
    #[error(transparent)]
    VulkanEncoderError(#[from] vk_video::VulkanEncoderError),

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
    #[cfg(feature = "vk-video")]
    #[error(transparent)]
    VulkanDecoderError(#[from] vk_video::DecoderError),

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

pub enum ErrorType {
    UserError,
    EntityNotFound,
    Conflict,
    BadGateway,

    ServerError,
}

pub struct PipelineErrorInfo {
    pub error_code: &'static str,
    pub error_type: ErrorType,
}

impl PipelineErrorInfo {
    fn new(error_code: &'static str, error_type: ErrorType) -> Self {
        Self {
            error_code,
            error_type,
        }
    }
}

impl From<&InitPipelineError> for PipelineErrorInfo {
    fn from(_value: &InitPipelineError) -> Self {
        PipelineErrorInfo::new("PIPELINE_INIT_FAILED", ErrorType::ServerError)
    }
}

const INPUT_STREAM_ALREADY_REGISTERED: &str = "INPUT_STREAM_ALREADY_REGISTERED";
const INPUT_ERROR: &str = "INPUT_STREAM_INPUT_ERROR";

const RESOURCE_DOES_NOT_EXIST: &str = "RESOURCE_DOES_NOT_EXIST";
const INVALID_MP4_SOURCE: &str = "INVALID_MP4_SOURCE";
const WHEP_INVALID_SERVER_URL: &str = "WHEP_INVALID_SERVER_URL";
const WHEP_REQUEST_FAILED: &str = "WHEP_REQUEST_FAILED";
const WHEP_BAD_STATUS: &str = "WHEP_BAD_STATUS";

impl From<&RegisterInputError> for PipelineErrorInfo {
    fn from(err: &RegisterInputError) -> Self {
        match err {
            RegisterInputError::AlreadyRegistered(_) => {
                PipelineErrorInfo::new(INPUT_STREAM_ALREADY_REGISTERED, ErrorType::Conflict)
            }

            // WHEP
            RegisterInputError::InputError(_, InputInitError::Whep(err))
                if matches!(err.as_ref(), WebrtcClientError::InvalidEndpointUrl(_, _)) =>
            {
                PipelineErrorInfo::new(WHEP_INVALID_SERVER_URL, ErrorType::UserError)
            }
            RegisterInputError::InputError(_, InputInitError::Whep(err))
                if matches!(err.as_ref(), WebrtcClientError::RequestFailed(_, _)) =>
            {
                PipelineErrorInfo::new(WHEP_REQUEST_FAILED, ErrorType::UserError)
            }
            RegisterInputError::InputError(_, InputInitError::Whep(err)) if matches!(err.as_ref(), WebrtcClientError::BadStatus(status, _) if status.is_client_error()) => {
                PipelineErrorInfo::new(WHEP_BAD_STATUS, ErrorType::UserError)
            }
            RegisterInputError::InputError(_, InputInitError::Whep(err)) if matches!(err.as_ref(), WebrtcClientError::BadStatus(status, _) if status.is_server_error()) => {
                PipelineErrorInfo::new(WHEP_BAD_STATUS, ErrorType::BadGateway)
            }

            // MP4
            RegisterInputError::InputError(
                _,
                InputInitError::Mp4(Mp4InputError::Mp4ReaderError(_)),
            ) => PipelineErrorInfo::new(INVALID_MP4_SOURCE, ErrorType::UserError),
            RegisterInputError::InputError(
                _,
                InputInitError::Mp4(Mp4InputError::HttpError(err)),
            ) if err.is_request() || err.is_status() => {
                PipelineErrorInfo::new(INVALID_MP4_SOURCE, ErrorType::UserError)
            }
            RegisterInputError::InputError(_, InputInitError::Mp4(Mp4InputError::IoError(_))) => {
                PipelineErrorInfo::new(INVALID_MP4_SOURCE, ErrorType::UserError)
            }

            // FFmpeg (used in HLS input)
            RegisterInputError::InputError(
                _,
                InputInitError::FfmpegError(ffmpeg_next::Error::Other {
                    errno: ffmpeg_next::error::ENOENT,
                }),
            ) => PipelineErrorInfo::new(RESOURCE_DOES_NOT_EXIST, ErrorType::UserError),

            // Generic
            RegisterInputError::InputError(_, _) => {
                PipelineErrorInfo::new(INPUT_ERROR, ErrorType::ServerError)
            }
        }
    }
}

const OUTPUT_STREAM_ALREADY_REGISTERED: &str = "OUTPUT_STREAM_ALREADY_REGISTERED";
const OUTPUT_ERROR: &str = "OUTPUT_STREAM_OUTPUT_ERROR";
const UNSUPPORTED_RESOLUTION: &str = "UNSUPPORTED_RESOLUTION";
const NO_VIDEO_OR_AUDIO_FOR_OUTPUT: &str = "NO_VIDEO_OR_AUDIO_FOR_OUTPUT";
const UNKNOWN_REGISTER_OUTPUT_ERROR: &str = "UNKNOWN_REGISTER_OUTPUT_ERROR";

const RTMP_CONNECTION_FAILED: &str = "RTMP_CONNECTION_FAILED";
const WHIP_INVALID_SERVER_URL: &str = "WHIP_INVALID_SERVER_URL";
const WHIP_REQUEST_FAILED: &str = "WHIP_REQUEST_FAILED";
const WHIP_BAD_STATUS: &str = "WHIP_BAD_STATUS";

const SERVER_PATH_RESOLUTION_FAILED: &str = "SERVER_PATH_RESOLUTION_FAILED";
#[cfg(feature = "vk-video")]
const INVALID_VULKAN_VIDEO_PARAMETERS: &str = "INVALID_VULKAN_VIDEO_PARAMETERS";

impl From<&RegisterOutputError> for PipelineErrorInfo {
    fn from(err: &RegisterOutputError) -> Self {
        match err {
            RegisterOutputError::AlreadyRegistered(_) => {
                PipelineErrorInfo::new(OUTPUT_STREAM_ALREADY_REGISTERED, ErrorType::Conflict)
            }

            // RTMP
            RegisterOutputError::OutputError(
                _,
                OutputInitError::RtmpError(RtmpClientError::RtmpStreamError(
                    rtmp::RtmpStreamError::TcpError(_),
                )),
            ) => PipelineErrorInfo::new(RTMP_CONNECTION_FAILED, ErrorType::UserError),

            // WHIP
            RegisterOutputError::OutputError(_, OutputInitError::WhipInitError(err))
                if matches!(err.as_ref(), WebrtcClientError::InvalidEndpointUrl(_, _)) =>
            {
                PipelineErrorInfo::new(WHIP_INVALID_SERVER_URL, ErrorType::UserError)
            }
            RegisterOutputError::OutputError(_, OutputInitError::WhipInitError(err))
                if matches!(err.as_ref(), WebrtcClientError::RequestFailed(_, _)) =>
            {
                PipelineErrorInfo::new(WHIP_REQUEST_FAILED, ErrorType::UserError)
            }
            RegisterOutputError::OutputError(_, OutputInitError::WhipInitError(err)) if matches!(err.as_ref(), WebrtcClientError::BadStatus(status, _) if status.is_client_error()) => {
                PipelineErrorInfo::new(WHIP_BAD_STATUS, ErrorType::UserError)
            }
            RegisterOutputError::OutputError(_, OutputInitError::WhipInitError(err)) if matches!(err.as_ref(), WebrtcClientError::BadStatus(status, _) if status.is_server_error()) => {
                PipelineErrorInfo::new(WHIP_BAD_STATUS, ErrorType::BadGateway)
            }

            // FFmpeg (used in MP4/HLS output)
            RegisterOutputError::OutputError(
                _,
                OutputInitError::FfmpegError(ffmpeg_next::Error::Other { errno }),
            ) if matches!(
                *errno,
                ffmpeg_next::error::ENOENT
                    | ffmpeg_next::error::EACCES
                    | ffmpeg_next::error::ENOTSUP
            ) =>
            {
                PipelineErrorInfo::new(SERVER_PATH_RESOLUTION_FAILED, ErrorType::UserError)
            }

            // Vulkan
            #[cfg(feature = "vk-video")]
            RegisterOutputError::OutputError(
                _,
                OutputInitError::EncoderError(EncoderInitError::VulkanEncoderError(
                    VulkanEncoderError::ParametersError { .. },
                )),
            ) => PipelineErrorInfo::new(INVALID_VULKAN_VIDEO_PARAMETERS, ErrorType::UserError),

            // Generic
            RegisterOutputError::OutputError(_, _) => {
                PipelineErrorInfo::new(OUTPUT_ERROR, ErrorType::ServerError)
            }
            RegisterOutputError::UnsupportedResolution(_) => {
                PipelineErrorInfo::new(UNSUPPORTED_RESOLUTION, ErrorType::UserError)
            }
            RegisterOutputError::SceneError(_, err) => err.into(),
            RegisterOutputError::NoVideoAndAudio(_) => {
                PipelineErrorInfo::new(NO_VIDEO_OR_AUDIO_FOR_OUTPUT, ErrorType::UserError)
            }
            RegisterOutputError::UnknownError(_) => {
                PipelineErrorInfo::new(UNKNOWN_REGISTER_OUTPUT_ERROR, ErrorType::ServerError)
            }
        }
    }
}

const UPDATE_INPUT_NOT_FOUND: &str = "INPUT_STREAM_NOT_FOUND";
const UPDATE_INPUT_ACTION_NOT_SUPPORTED: &str = "INPUT_ACTION_NOT_SUPPORTED";

impl From<&UpdateInputError> for PipelineErrorInfo {
    fn from(err: &UpdateInputError) -> Self {
        match err {
            UpdateInputError::NotFound(_) => {
                PipelineErrorInfo::new(UPDATE_INPUT_NOT_FOUND, ErrorType::EntityNotFound)
            }
            UpdateInputError::SeekNotSupported(_) | UpdateInputError::PausingNotSupported(_) => {
                PipelineErrorInfo::new(UPDATE_INPUT_ACTION_NOT_SUPPORTED, ErrorType::UserError)
            }
        }
    }
}

const INPUT_STREAM_STILL_IN_USE: &str = "INPUT_STREAM_STILL_IN_USE";
const INPUT_STREAM_NOT_FOUND: &str = "INPUT_STREAM_NOT_FOUND";

impl From<&UnregisterInputError> for PipelineErrorInfo {
    fn from(err: &UnregisterInputError) -> Self {
        match err {
            UnregisterInputError::NotFound(_) => {
                PipelineErrorInfo::new(INPUT_STREAM_NOT_FOUND, ErrorType::EntityNotFound)
            }
            UnregisterInputError::StillInUse(_) => {
                PipelineErrorInfo::new(INPUT_STREAM_STILL_IN_USE, ErrorType::UserError)
            }
        }
    }
}

const OUTPUT_STREAM_STILL_IN_USE: &str = "OUTPUT_STREAM_STILL_IN_USE";
const OUTPUT_STREAM_NOT_FOUND: &str = "OUTPUT_STREAM_NOT_FOUND";
const NO_AUDIO_AND_VIDEO_SPECIFIED: &str = "NO_AUDIO_AND_VIDEO_SPECIFIED";
const AUDIO_VIDEO_SPECIFICATION_NOT_MATCHING: &str = "AUDIO_VIDEO_SPECIFICATION_NOT_MATCHING";

impl From<&UnregisterOutputError> for PipelineErrorInfo {
    fn from(err: &UnregisterOutputError) -> Self {
        match err {
            UnregisterOutputError::NotFound(_) => {
                PipelineErrorInfo::new(OUTPUT_STREAM_NOT_FOUND, ErrorType::EntityNotFound)
            }
            UnregisterOutputError::StillInUse(_) => {
                PipelineErrorInfo::new(OUTPUT_STREAM_STILL_IN_USE, ErrorType::UserError)
            }
        }
    }
}

const BUILD_SCENE_ERROR: &str = "BUILD_SCENE_ERROR";

impl From<&UpdateSceneError> for PipelineErrorInfo {
    fn from(err: &UpdateSceneError) -> Self {
        match err {
            UpdateSceneError::WgpuError(err) => err.into(),
            UpdateSceneError::OutputNotRegistered(_) => {
                PipelineErrorInfo::new(OUTPUT_STREAM_NOT_FOUND, ErrorType::UserError)
            }
            UpdateSceneError::SceneError(_) => PipelineErrorInfo {
                error_code: BUILD_SCENE_ERROR,
                error_type: ErrorType::UserError,
            },
            UpdateSceneError::NoAudioAndVideo(_) => PipelineErrorInfo {
                error_code: NO_AUDIO_AND_VIDEO_SPECIFIED,
                error_type: ErrorType::UserError,
            },
            UpdateSceneError::AudioVideoNotMatching(_) => PipelineErrorInfo {
                error_code: AUDIO_VIDEO_SPECIFICATION_NOT_MATCHING,
                error_type: ErrorType::UserError,
            },
        }
    }
}

const REQUEST_KEYFRAME_ERROR: &str = "REQUEST_KEYFRAME_ERROR";

impl From<&RequestKeyframeError> for PipelineErrorInfo {
    fn from(_err: &RequestKeyframeError) -> Self {
        PipelineErrorInfo {
            error_code: REQUEST_KEYFRAME_ERROR,
            error_type: ErrorType::UserError,
        }
    }
}

const WGPU_INIT_ERROR: &str = "WGPU_INIT_ERROR";
const LAYOUT_INIT_ERROR: &str = "LAYOUT_INIT_ERROR";

impl From<&InitRendererEngineError> for PipelineErrorInfo {
    fn from(err: &InitRendererEngineError) -> Self {
        match err {
            InitRendererEngineError::FailedToInitWgpuCtx(_) => {
                PipelineErrorInfo::new(WGPU_INIT_ERROR, ErrorType::ServerError)
            }
            InitRendererEngineError::LayoutTransformationsInitError(_) => {
                PipelineErrorInfo::new(LAYOUT_INIT_ERROR, ErrorType::ServerError)
            }
        }
    }
}

const ENTITY_ALREADY_REGISTERED: &str = "ENTITY_ALREADY_REGISTERED";
const INVALID_SHADER: &str = "INVALID_SHADER";
const REGISTER_IMAGE_ERROR: &str = "REGISTER_IMAGE_ERROR";
const REGISTER_WEB_RENDERER_ERROR: &str = "REGISTER_WEB_RENDERER_ERROR";

impl From<&RegisterRendererError> for PipelineErrorInfo {
    fn from(err: &RegisterRendererError) -> Self {
        match err {
            RegisterRendererError::RendererRegistry(err) => match err {
                RegisterError::KeyTaken { .. } => {
                    PipelineErrorInfo::new(ENTITY_ALREADY_REGISTERED, ErrorType::Conflict)
                }
            },
            RegisterRendererError::Shader(_, _) => {
                PipelineErrorInfo::new(INVALID_SHADER, ErrorType::UserError)
            }
            RegisterRendererError::Image(_, _) => {
                PipelineErrorInfo::new(REGISTER_IMAGE_ERROR, ErrorType::UserError)
            }
            RegisterRendererError::Web(_, _) => {
                PipelineErrorInfo::new(REGISTER_WEB_RENDERER_ERROR, ErrorType::ServerError)
            }
        }
    }
}

const ENTITY_NOT_FOUND: &str = "ENTITY_NOT_FOUND";

impl From<&UnregisterRendererError> for PipelineErrorInfo {
    fn from(err: &UnregisterRendererError) -> Self {
        match err {
            UnregisterRendererError::RendererRegistry(_) => {
                PipelineErrorInfo::new(ENTITY_NOT_FOUND, ErrorType::EntityNotFound)
            }
        }
    }
}

const WGPU_VALIDATION_ERROR: &str = "WGPU_VALIDATION_ERROR";
const WGPU_OUT_OF_MEMORY_ERROR: &str = "WGPU_OUT_OF_MEMORY_ERROR";
const WGPU_INTERNAL_ERROR: &str = "WGPU_INTERNAL_ERROR";

impl From<&WgpuError> for PipelineErrorInfo {
    fn from(err: &WgpuError) -> Self {
        match err {
            WgpuError::Validation(_) => {
                PipelineErrorInfo::new(WGPU_VALIDATION_ERROR, ErrorType::UserError)
            }
            WgpuError::OutOfMemory(_) => {
                PipelineErrorInfo::new(WGPU_OUT_OF_MEMORY_ERROR, ErrorType::ServerError)
            }
            WgpuError::Internal(_) => {
                PipelineErrorInfo::new(WGPU_INTERNAL_ERROR, ErrorType::ServerError)
            }
        }
    }
}
