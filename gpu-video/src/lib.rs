#![doc = include_str!("../README.md")]

#[cfg(feature = "expose-parsers")]
pub mod parser;
#[cfg(not(feature = "expose-parsers"))]
pub(crate) mod parser;

// TODO: The modules below should compile on macos
#[cfg(all(vulkan, feature = "expose-backends"))]
pub mod backends;
#[cfg(all(vulkan, not(feature = "expose-backends")))]
pub(crate) mod backends;

#[cfg(vulkan)]
mod adapter;
#[cfg(vulkan)]
pub mod capabilities;
#[cfg(vulkan)]
pub(crate) mod decoders;
#[cfg(vulkan)]
mod device;
#[cfg(vulkan)]
mod frame_sorter;
#[cfg(all(vulkan, feature = "wgpu"))]
mod global_registry;
#[cfg(vulkan)]
mod instance;
#[cfg(all(vulkan, feature = "wgpu"))]
pub(crate) mod wgpu_helpers;

#[cfg(vulkan)]
mod exports;
#[cfg(vulkan)]
pub use exports::*;

// TODO: The modules below should be made backend agnostic
#[cfg(vulkan)]
mod vulkan_encoder;
#[cfg(all(vulkan, feature = "transcoder"))]
mod vulkan_transcoder;

// If vulkan is unsupported and parsers are not exposed
#[cfg(not(any(vulkan, feature = "expose-parsers")))]
compile_error!("gpu-video can be only compiled on platforms supported by vulkan.");
