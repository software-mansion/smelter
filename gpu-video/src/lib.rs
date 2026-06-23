#![doc = include_str!("../README.md")]

#[cfg(vulkan)]
mod adapter;
#[cfg(vulkan)]
pub(crate) mod codec;
#[cfg(vulkan)]
mod device;
// TODO: cfg(vulkan) will not be needed once we add proper metal support
#[cfg(all(vulkan, feature = "wgpu"))]
mod global_registry;
#[cfg(vulkan)]
mod instance;
#[cfg(vulkan)]
mod vulkan;
#[cfg(all(vulkan, feature = "transcoder"))]
mod vulkan_transcoder;
#[cfg(all(vulkan, feature = "wgpu"))]
pub(crate) mod wgpu_helpers;
#[cfg(vulkan)]
pub(crate) mod wrappers;

// TODO: make sure it's not vulkan specific
#[cfg(vulkan)]
mod prelude;
#[cfg(vulkan)]
pub use prelude::*;

#[cfg(feature = "expose-parsers")]
pub mod parser;
#[cfg(not(feature = "expose-parsers"))]
pub(crate) mod parser;

// If vulkan is unsupported and parsers are not exposed
#[cfg(not(any(vulkan, feature = "expose-parsers")))]
compile_error!("gpu-video can be only compiled on platforms supported by vulkan.");
