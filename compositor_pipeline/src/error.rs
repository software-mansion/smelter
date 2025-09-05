use compositor_render::{
    error::{
        InitRendererEngineError, RegisterError, RegisterRendererError, RequestKeyframeError,
        UnregisterRendererError, UpdateSceneError, WgpuError,
    },
    InputId, OutputId,
};

use crate::prelude::*;

#[derive(Debug, thiserror::Error)]
pub enum InitPipelineError {
    #[error(transparent)]
    InitRendererEngine(#[from] InitRendererEngineError),

    #[error("Failed to create a download directory.")]
    CreateDownloadDir(#[source] std::io::Error),

    #[cfg(feature = "vk-video")]
    #[error(transparent)]
    VulkanInitError(#[from] vk_video::VulkanInitError),

    #[error("Failed to create tokio::Runtime.")]
    CreateTokioRuntime(#[source] std::io::Error),

    #[error("Failed to initialize WHIP WHEP server.")]
    WhipWhepServerInitError(#[source] std::io::Error),
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

    #[error("Failed to register output stream \"{0}\". Resolution in each dimension has to be divisible by 2.")]
    UnsupportedResolution(OutputId),

    #[error("Failed to initialize the scene when registering output \"{0}\".")]
    SceneError(OutputId, #[source] UpdateSceneError),

    #[error("Failed to register output stream \"{0}\". At least one of \"video\" and \"audio\" must be specified.")]
    NoVideoAndAudio(OutputId),

    #[error("Unknown error: {0}")]
    UnknownError(String),
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

    #[error("Failed to register output. All ports in range {lower_bound} to {upper_bound} are already used or not available.")]
    AllPortsAlreadyInUse { lower_bound: u16, upper_bound: u16 },

    #[error("Failed to register output. FFmpeg error: {0}.")]
    FfmpegError(ffmpeg_next::Error),

    #[error("Unknown whip output error.")]
    UnknownWhipError,

    #[error("Whip init timeout exceeded")]
    WhipInitTimeout,

    #[error("Failed to init whip output")]
    WhipInitError(#[source] Box<WhipOutputError>),

    #[error("Unknown whep output error.")]
    UnknownWhepError,

    #[error("Whep init timeout exceeded")]
    WhepInitTimeout,

    #[error("Failed to init whep output")]
    WhepInitError(#[source] Box<WhepOutputError>),

    #[error("WHIP WHEP server is not running, cannot start WHEP output")]
    WhipWhepServerNotRunning,
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
}

#[derive(Debug, thiserror::Error)]
pub enum InputInitError {
    #[error(transparent)]
    Rtp(#[from] RtpInputError),

    #[error(transparent)]
    Mp4(#[from] Mp4InputError),

    #[error("WHIP WHEP server is not running, cannot start WHIP input")]
    WhipWhepServerNotRunning,

    #[cfg(feature = "decklink")]
    #[error(transparent)]
    DeckLink(#[from] DeckLinkInputError),

    #[error(transparent)]
    FfmpegError(#[from] ffmpeg_next::Error),

    #[error(transparent)]
    ResamplerError(#[from] rubato::ResamplerConstructionError),

    #[error("Failed to initialize decoder.")]
    DecoderError(#[from] DecoderInitError),

    #[error("Invalid video decoder provided. Expected {expected:?} decoder")]
    InvalidVideoDecoderProvided { expected: VideoCodec },
}

#[derive(Debug, thiserror::Error)]
pub enum DecoderInitError {
    #[cfg(feature = "vk-video")]
    #[error(transparent)]
    VulkanDecoderError(#[from] vk_video::DecoderError),

    #[error("Pipeline couldn't detect a vulkan video compatible device when it was being initialized. Cannot create a vulkan video decoder")]
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
    ServerError,
    EntityNotFound,
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

impl From<&RegisterInputError> for PipelineErrorInfo {
    fn from(err: &RegisterInputError) -> Self {
        match err {
            RegisterInputError::AlreadyRegistered(_) => {
                PipelineErrorInfo::new(INPUT_STREAM_ALREADY_REGISTERED, ErrorType::UserError)
            }

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

impl From<&RegisterOutputError> for PipelineErrorInfo {
    fn from(err: &RegisterOutputError) -> Self {
        match err {
            RegisterOutputError::AlreadyRegistered(_) => {
                PipelineErrorInfo::new(OUTPUT_STREAM_ALREADY_REGISTERED, ErrorType::UserError)
            }
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
                    PipelineErrorInfo::new(ENTITY_ALREADY_REGISTERED, ErrorType::UserError)
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
