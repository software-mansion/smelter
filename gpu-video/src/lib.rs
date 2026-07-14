#![doc = include_str!("../README.md")]

#[cfg(feature = "expose-parsers")]
pub mod parser;
#[cfg(not(feature = "expose-parsers"))]
pub(crate) mod parser;

// TODO: The modules below should compile on macos
#[cfg(all(supported, feature = "expose-backends"))]
pub mod backends;
#[cfg(all(supported, not(feature = "expose-backends")))]
pub(crate) mod backends;

#[cfg(supported)]
mod adapter;
#[cfg(supported)]
pub mod capabilities;
#[cfg(supported)]
pub(crate) mod decoders;
#[cfg(supported)]
mod device;
#[cfg(supported)]
pub(crate) mod encoders;
#[cfg(supported)]
mod frame_sorter;
#[cfg(all(supported, feature = "wgpu"))]
mod global_registry;
#[cfg(supported)]
mod instance;
#[cfg(all(supported, feature = "transcoder"))]
mod transcoder;
#[cfg(all(supported, feature = "wgpu"))]
pub(crate) mod wgpu_helpers;

#[cfg(supported)]
mod exports;
#[cfg(supported)]
pub use exports::*;

// If supported is unsupported and parsers are not exposed
#[cfg(not(any(supported, feature = "expose-parsers")))]
compile_error!("gpu-video can be only compiled on platforms supported by vulkan.");
