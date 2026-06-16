#![recursion_limit = "256"]

mod audio_decoder;
mod common;
mod compositor_instance;
#[cfg(all(test, target_os = "linux"))]
mod gpu_video_tests;
mod texture;
mod video_decoder;

pub mod examples;
pub mod media;
pub mod paths;
pub mod test_input;
pub mod tools;

pub mod pipeline_tests;
pub mod render_tests;

pub use audio_decoder::AudioSampleBatch;
pub use common::*;
pub use compositor_instance::*;
pub use texture::read_rgba_texture;
