//! A library for hardware video coding using Vulkan Video, with [`wgpu`] integration.
//!
//! # Overview
//!
//! The goal of this library is to provide easy access to hardware video coding. You can use it to decode a video frame into a `Vec<u8>` with pixel data, or into a [`wgpu::Texture`]. Currently, we only support H.264 (aka AVC or MPEG 4 Part 10) decoding, but we plan to support at least H.264 encoding and hopefully other codecs supported by Vulkan Video.
//!
//! An advantage of using this library with wgpu is that decoded video frames never leave the GPU memory. There's no copying the frames to RAM and back to the GPU, so it should be quite fast if you want to use them for rendering.
//!
//! This library was developed as a part of [smelter, a tool for video composition](https://smelter.dev/).
//!
//! # Usage
//!
//! ```no_run
//! fn decode_video(
//!     window: &winit::window::Window,
//!     mut encoded_video_reader: impl std::io::Read,
//! ) {
//!     let instance = vk_video::VulkanInstance::new().unwrap();
//!     let surface = instance.wgpu_instance().create_surface(window).unwrap();
//!     let adapter = instance.create_adapter(Some(&surface)).unwrap();
//!     let device = adapter
//!         .create_device(
//!             wgpu::Features::empty(),
//!             wgpu::Limits::default(),
//!         )
//!         .unwrap();
//!
//!     let mut decoder = device
//!         .create_wgpu_textures_decoder(
//!             vk_video::parameters::DecoderParameters::default()
//!         ).unwrap();
//!
//!     let mut buffer = vec![0; 4096];
//!
//!     while let Ok(n) = encoded_video_reader.read(&mut buffer) {
//!         if n == 0 {
//!             return;
//!         }
//!
//!         let decoded_frames = decoder.decode(vk_video::EncodedInputChunk {
//!             data: &buffer[..n],
//!             pts: None
//!         }).unwrap();
//!
//!         for frame in decoded_frames {
//!             // Each frame contains a wgpu::Texture you can sample for drawing.
//!             // device.wgpu_device() will give you a wgpu::Device and device.wgpu_queue()
//!             // a wgpu::Queue. You can use these for interacting with the frames.
//!         }
//!     }
//! }
//! ```
//!
//! # Compatibility
//!
//! On Linux, the library should work on NVIDIA and AMD GPUs out of the box with recent Mesa drivers. For AMD GPUs with a bit older Mesa drivers, you may need to set the `RADV_PERFTEST=video_decode,video_encode` environment variable:
//!
//! ```sh
//! RADV_PERFTEST=video_decode,video_encode cargo run
//! ```
//!
//! It should work on Windows with recent drivers out of the box. Be sure to submit an issue if it doesn't.
//!
//! # Smelter toolkit
//!
//! <a href="https://swmansion.com" style="margin: 20px">
//!   <img height="60" alt="Smelter" src="https://logo.swmansion.com/logo?color=white&variant=desktop&width=150&tag=smelter-vk-video">
//! </a>
//! <a href="https://smelter.dev" style="margin: 20px">
//!   <picture>
//!     <source media="(prefers-color-scheme: dark)" srcset="https:///github.com/software-mansion/smelter/raw/master/tools/assets/smelter-logo-transparent.svg">
//!     <source media="(prefers-color-scheme: light)" srcset="https:///github.com/software-mansion/smelter/raw/master/tools/assets/smelter-logo-background.svg">
//!     <img height="60" alt="Smelter" src="https:///github.com/software-mansion/smelter/raw/master/tools/assets/smelter-logo-background.svg">
//!   </picture>
//! </a>
//!
//! `vk_video` is part of the [Smelter toolkit](https://smelter.dev) created by [Software Mansion](https://swmansion.com).
//!

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

pub mod parser;
