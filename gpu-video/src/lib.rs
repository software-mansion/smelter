#![doc = include_str!("../README.md")]

#[cfg(vulkan)]
mod adapter;
#[cfg(vulkan)]
pub(crate) mod codec;
#[cfg(vulkan)]
mod device;
#[cfg(vulkan)]
mod instance;
#[cfg(vulkan)]
mod vulkan_decoder;
#[cfg(vulkan)]
mod vulkan_encoder;
#[cfg(all(vulkan, feature = "transcoder"))]
mod vulkan_transcoder;
#[cfg(all(vulkan, feature = "wgpu"))]
pub(crate) mod wgpu_helpers;
#[cfg(vulkan)]
pub(crate) mod wrappers;

#[cfg(vulkan)]
mod vulkan_video;
#[cfg(vulkan)]
pub use vulkan_video::*;

mod types;
pub use types::{VideoFramerate, VideoResolution};

#[cfg(feature = "expose-parsers")]
pub mod parser;
#[cfg(all(
    not(feature = "expose-parsers"),
    any(vulkan, all(feature = "quicksync", target_os = "linux"))
))]
pub(crate) mod parser;

#[cfg(all(feature = "quicksync", target_os = "linux"))]
mod dmabuf;

#[cfg(all(feature = "quicksync", target_os = "linux"))]
pub mod quicksync;
