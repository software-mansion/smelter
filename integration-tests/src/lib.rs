#![recursion_limit = "256"]

mod aac_decoder;
mod audio_decoder;
mod common;
mod compositor_instance;
mod mp4_reader;
mod output_receiver;
mod packet_sender;
mod texture;
mod video_decoder;

pub mod examples;
pub mod media;
pub mod media_dump;
pub mod paths;
pub mod test_input;
pub mod tools;

pub mod pipeline_tests;
pub mod render_tests;

pub use common::*;
pub use compositor_instance::*;
pub use output_receiver::*;
pub use packet_sender::*;
pub use texture::read_rgba_texture;
