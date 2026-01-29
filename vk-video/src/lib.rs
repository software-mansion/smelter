#![doc = include_str!("../README.md")]

#[cfg(vulkan)]
mod adapter;
#[cfg(vulkan)]
mod device;
#[cfg(vulkan)]
mod instance;
#[cfg(vulkan)]
mod vulkan_decoder;
#[cfg(vulkan)]
mod vulkan_encoder;
#[cfg(vulkan)]
pub(crate) mod wrappers;

#[cfg(vulkan)]
mod vulkan_video;
#[cfg(vulkan)]
pub use vulkan_video::*;

#[cfg(feature = "expose_parsers")]
pub mod parser;
#[cfg(not(feature = "expose_parsers"))]
pub(crate) mod parser;

// If vulkan is unsupported and parsers are not exposed
#[cfg(not(any(vulkan, feature = "expose_parsers")))]
compile_error!("vk-video can be only compiled on platforms supported by vulkan.");
