use crate::transformations::web_renderer::CreateWebRendererError;
use crate::wgpu::common_pipeline::CreateShaderError;
use crate::wgpu::CreateWgpuCtxError;
use crate::{
    registry,
    scene::SceneError,
    transformations::{image::ImageError, web_renderer::chromium_context::WebRendererContextError},
};
use crate::{OutputId, RendererId};

pub use crate::registry::RegisterError;
pub use crate::wgpu::WgpuError;

#[derive(Debug, thiserror::Error)]
pub enum InitRendererEngineError {
    #[error("Failed to initialize a wgpu context.")]
    FailedToInitWgpuCtx(#[from] CreateWgpuCtxError),

    #[error("Failed to initialize chromium context.")]
    FailedToInitChromiumCtx(#[from] WebRendererContextError),

    #[error("Failed to initialize apply_layout transformation.")]
    LayoutTransformationsInitError(#[source] CreateShaderError),
}

#[derive(Debug, thiserror::Error)]
pub enum RegisterRendererError {
    #[error(transparent)]
    RendererRegistry(#[from] registry::RegisterError),

    #[error("Failed to register shader \"{1}\".")]
    Shader(#[source] CreateShaderError, RendererId),

    #[error("Failed to register image \"{1}\".")]
    Image(#[source] ImageError, RendererId),

    #[error("Failed to register web renderer \"{1}\".")]
    Web(#[source] CreateWebRendererError, RendererId),
}

#[derive(Debug, thiserror::Error)]
pub enum UnregisterRendererError {
    #[error(transparent)]
    RendererRegistry(#[from] registry::UnregisterError),
}

#[derive(Debug, thiserror::Error)]
pub enum RenderSceneError {
    #[error(transparent)]
    WgpuError(#[from] WgpuError),
}

#[derive(Debug, thiserror::Error)]
pub enum UpdateSceneError {
    #[error(transparent)]
    WgpuError(#[from] WgpuError),

    #[error(
        "Output \"{0}\" does not exist, register it first before using it in the scene definition."
    )]
    OutputNotRegistered(OutputId),

    #[error(transparent)]
    SceneError(#[from] SceneError),

    #[error("No video and audio specified for output \"{0}\"")]
    NoAudioAndVideo(OutputId),

    #[error("Audio and video specification for output \"{0}\" doesn't match one provided in register output request.
    If audio or video was specified on register, it has to be specified in update.
    If audio or video wasn't specified on register, it can't be specified in update.")]
    AudioVideoNotMatching(OutputId),
}

#[derive(Debug, thiserror::Error)]
pub enum RequestKeyframeError {
    #[error("Output \"{0}\" does not exist, register it first before requesting keyframe.")]
    OutputNotRegistered(OutputId),
    #[error(
        "Output \"{0}\" is a raw output. Keyframe request is only available for encoded outputs."
    )]
    RawOutput(OutputId),
    #[error("Output \"{0}\" is not a video output. Can't request a keyframe on non video output.")]
    NoVideoOutput(OutputId),
}

pub struct ErrorStack<'a>(Option<&'a (dyn std::error::Error)>);

impl<'a> ErrorStack<'a> {
    pub fn new(value: &'a (dyn std::error::Error)) -> Self {
        ErrorStack(Some(value))
    }

    pub fn into_string(self) -> String {
        let stack: Vec<String> = self.map(ToString::to_string).collect();
        stack.join("\n")
    }
}

impl<'a> Iterator for ErrorStack<'a> {
    type Item = &'a (dyn std::error::Error);

    fn next(&mut self) -> Option<Self::Item> {
        self.0.inspect(|err| {
            self.0 = err.source();
        })
    }
}
