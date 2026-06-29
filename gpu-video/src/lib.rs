#![doc = include_str!("../README.md")]

#[cfg(feature = "expose-backends")]
pub mod backends;
#[cfg(all(vulkan, not(feature = "expose-backends")))]
pub(crate) mod backends;

// TODO: After caps refactor cfg for instance and adapter won't be needed
#[cfg(vulkan)]
mod adapter;
#[cfg(vulkan)]
mod instance;

#[cfg(vulkan)]
pub(crate) mod codec;
#[cfg(vulkan)]
mod device;
// TODO: cfg(vulkan) will not be needed once we add proper metal support
#[cfg(all(vulkan, feature = "wgpu"))]
mod global_registry;
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

// TODO: This could be inlined with lib.rs when metal support is added
#[cfg(vulkan)]
mod vulkan_video;
#[cfg(vulkan)]
pub use vulkan_video::*;

#[cfg(feature = "expose-parsers")]
pub mod parser;
#[cfg(not(feature = "expose-parsers"))]
pub(crate) mod parser;

// If vulkan is unsupported and parsers are not exposed
#[cfg(not(any(vulkan, feature = "expose-parsers")))]
compile_error!("gpu-video can be only compiled on platforms supported by vulkan.");
