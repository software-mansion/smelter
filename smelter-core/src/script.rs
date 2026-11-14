//! Output scenes driven by a script.
//!
//! A script replaces update requests as the source of the scene. It runs on the renderer thread,
//! before each frame, so its reactions to input changes and its timers are frame accurate.
//!
//! This crate only defines how a script is driven. The scripting language itself is provided
//! with [`ScriptRuntime`] in `PipelineOptions::script_runtime`.

use std::{
    fmt::{self, Debug},
    sync::Arc,
};

use smelter_render::{
    Framerate, InputId, OutputId, Resolution,
    scene::{Component, ComponentSize, Scene, SceneError},
};

use crate::{AudioMixerConfig, Timestamp};

/// Defines what is rendered for an output.
#[derive(Debug, Clone, PartialEq)]
pub enum OutputScene {
    /// Scene that changes only with an update request.
    Static(Scene),
    /// Scene produced by a script.
    Script(ScriptSpec),
}

impl From<Scene> for OutputScene {
    fn from(scene: Scene) -> Self {
        Self::Static(scene)
    }
}

#[derive(Clone, PartialEq)]
pub struct ScriptSpec {
    pub source: Arc<str>,
    /// Data passed to the script. An update with the same `source` only replaces props of the
    /// running script, its state is preserved.
    pub props: serde_json::Value,
}

impl Debug for ScriptSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ScriptSpec")
            .field("source", &format!("<{} bytes>", self.source.len()))
            .field("props", &self.props)
            .finish()
    }
}

/// Creates script instances.
pub trait ScriptRuntime: Debug + Send + Sync {
    fn load(&self, spec: &ScriptSpec, ctx: ScriptOutputCtx)
    -> Result<Box<dyn Script>, ScriptError>;
}

/// Script instance that drives a scene of a single output.
pub trait Script: Send {
    /// Called once right after the script is loaded and then before each rendered frame. The
    /// first call has to return a video scene.
    fn update(&mut self, frame: &ScriptFrame<'_>) -> Result<ScriptUpdate, ScriptError>;

    /// Replaces props. The script observes the new value on the next `update` call.
    fn set_props(&mut self, props: serde_json::Value);
}

/// Output that the script is loaded for.
pub struct ScriptOutputCtx {
    pub output_id: OutputId,
    pub resolution: Resolution,
    pub framerate: Framerate,
    pub host: Arc<dyn ScriptHost>,
}

/// Pipeline functionality available for a script.
pub trait ScriptHost: Send + Sync {
    /// Size the component would have if it was used in a scene right now.
    fn measure_component(&self, component: Component) -> Result<ComponentSize, SceneError>;
}

#[derive(Debug, Clone, Copy)]
pub struct ScriptFrame<'a> {
    /// PTS of the frame that will be rendered next, relative to the pipeline start. It never
    /// decreases between calls.
    pub pts: Timestamp,
    /// Registered inputs in registration order, with `events` already applied.
    pub inputs: &'a [ScriptInput],
    /// What changed since the previous call.
    pub events: &'a [ScriptEvent],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptInput {
    pub input_id: InputId,
    /// `None` if the video track did not deliver any data yet.
    pub video: Option<TrackState>,
    /// `None` if the audio track did not deliver any data yet.
    pub audio: Option<TrackState>,
    /// Resolution of the last rendered video frame.
    pub resolution: Option<Resolution>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackState {
    /// Data was delivered, but it is not used for rendering/mixing yet.
    Ready,
    Playing,
    Paused,
    Finished,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptEvent {
    pub input_id: InputId,
    pub kind: ScriptEventKind,
}

/// Video `Playing`, `Eos` and `Resized` are in sync with frames: the event is delivered before
/// the first frame it applies to is rendered. Other events are delivered on the first frame
/// after they happen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptEventKind {
    InputRegistered,
    InputUnregistered,
    VideoDelivered,
    VideoPlaying,
    VideoPaused,
    VideoEos,
    VideoResized,
    AudioDelivered,
    AudioPlaying,
    AudioPaused,
    AudioEos,
}

#[derive(Debug, Default)]
pub struct ScriptUpdate {
    /// `None` if the scene did not change.
    pub video: Option<Component>,
    /// `None` if the audio config did not change. Ignored for outputs without audio.
    pub audio: Option<AudioMixerConfig>,
    /// Errors that did not prevent the script from producing this update, e.g. a failed
    /// event handler.
    pub errors: Vec<ScriptError>,
}

#[derive(Debug, thiserror::Error, Clone)]
pub enum ScriptError {
    #[error("Scripts are not supported, pipeline was created without a script runtime.")]
    RuntimeNotAvailable,

    #[error("Failed to load the script. {0}")]
    Load(String),

    #[error("Script failed. {0}")]
    Runtime(String),

    #[error("Script produced an invalid scene. {0}")]
    InvalidScene(String),

    /// Script will not run anymore, e.g. it exceeded execution time or memory limit.
    #[error("Script was terminated. {0}")]
    Terminated(String),
}
