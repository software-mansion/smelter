#![recursion_limit = "256"]

mod audio_mixer;
mod queue;
pub use queue::{InputSideChannel, LateEventPolicy, QueueInputOptions, QueueTrackOffset};

pub mod codecs;
pub mod error;
pub mod event;
pub mod graphics_context;
pub mod protocols;
pub mod stats;

mod pipeline;
pub use pipeline::*;

mod types;
pub use types::*;

mod frame;
pub use frame::*;

mod timestamp;
pub use timestamp::*;

mod input;
pub use input::*;

mod output;
pub use output::*;

mod prelude;

pub const GPU_VIDEO_ENABLED: bool = cfg!(feature = "gpu-video");
pub const DECKLINK_ENABLED: bool = cfg!(feature = "decklink");
