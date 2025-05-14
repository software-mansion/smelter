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
//! ```
//! fn decode_video(
//!     window: &winit::window::Window,
//!     mut encoded_video_reader: impl std::io::Read,
//! ) {
//!     let instance = vk_video::VulkanInstance::new().unwrap();
//!     let surface = instance.wgpu_instance.create_surface(window).unwrap();
//!     let device = instance
//!         .create_device(
//!             wgpu::Features::empty(),
//!             wgpu::Limits::default(),
//!             Some(&surface),
//!         )
//!         .unwrap();
//!
//!     let mut decoder = device.create_wgpu_textures_decoder().unwrap();
//!     let mut buffer = vec![0; 4096];
//!
//!     while let Ok(n) = encoded_video_reader.read(&mut buffer) {
//!         if n == 0 {
//!             return;
//!         }
//!
//!         let decoded_frames = decoder.decode(&buffer[..n], None).unwrap();
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
//! On Linux, the library should work on NVIDIA and AMD GPUs out of the box with recent Mesa drivers. For AMD GPUs with a bit older Mesa drivers, you may need to set the `RADV_PERFTEST=video_decode` environment variable:
//!
//! ```sh
//! RADV_PERFTEST=video_decode cargo run
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
//!     <source media="(prefers-color-scheme: dark)" srcset="https:///github.com/software-mansion/smelter/raw/master/assets/smelter-logo-transparent.svg">
//!     <source media="(prefers-color-scheme: light)" srcset="https:///github.com/software-mansion/smelter/raw/master/assets/smelter-logo-background.svg">
//!     <img height="60" alt="Smelter" src="https:///github.com/software-mansion/smelter/raw/master/assets/smelter-logo-background.svg">
//!   </picture>
//! </a>
//!
//! `vk_video` is part of the [Smelter toolkit](https://smelter.dev) created by [Software Mansion](https://swmansion.com).
//!

#![cfg(not(target_os = "macos"))]
mod parser;
mod vulkan_decoder;

use parser::Parser;
use vulkan_decoder::{FrameSorter, VulkanDecoder};

pub use parser::ParserError;
pub use vulkan_decoder::{VulkanCtxError, VulkanDecoderError, VulkanDevice, VulkanInstance};

#[derive(Debug, thiserror::Error)]
pub enum DecoderError {
    #[error("Decoder error: {0}")]
    VulkanDecoderError(#[from] VulkanDecoderError),

    #[error("H264 parser error: {0}")]
    ParserError(#[from] ParserError),
}

/// Represents a chunk of encoded video data.
///
/// If `pts` is [`Option::Some`], it is inferred that the chunk contains bytestream that belongs to
/// one output frame.
/// If `pts` is [`Option::None`], the chunk can contain bytestream from multiple consecutive
/// frames.
pub struct EncodedChunk<T> {
    pub data: T,
    pub pts: Option<u64>,
}

/// Represents a single decoded frame.
pub struct Frame<T> {
    pub data: T,
    pub pts: Option<u64>,
}

pub struct RawFrameData {
    pub frame: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// A decoder that outputs frames stored as [`wgpu::Texture`]s
pub struct WgpuTexturesDecoder<'a> {
    vulkan_decoder: VulkanDecoder<'a>,
    parser: Parser,
    frame_sorter: FrameSorter<wgpu::Texture>,
}

impl WgpuTexturesDecoder<'_> {
    /// The produced textures have the [`wgpu::TextureFormat::NV12`] format and can be used as a copy source or a texture binding.
    ///
    /// `pts` is the presentation timestamp -- a number, which describes when the given frame
    /// should be presented, used for synchronization with other tracks, e.g. with audio
    pub fn decode(
        &mut self,
        frame: EncodedChunk<&'_ [u8]>,
    ) -> Result<Vec<Frame<wgpu::Texture>>, DecoderError> {
        let instructions = self.parser.parse(frame.data, frame.pts)?;

        let unsorted_frames = self.vulkan_decoder.decode_to_wgpu_textures(&instructions)?;

        let mut result = Vec::new();

        for unsorted_frame in unsorted_frames {
            let mut sorted_frames = self.frame_sorter.put(unsorted_frame);
            result.append(&mut sorted_frames);
        }

        Ok(result)
    }

    /// Flush all frames from the decoder.
    ///
    /// Make sure that this is done when you have the knowledge that no more frames will be coming
    /// that need to be presented before the already decoded frames.
    pub fn flush(&mut self) -> Vec<Frame<wgpu::Texture>> {
        self.frame_sorter.flush()
    }
}

/// A decoder that outputs frames stored as [`Vec<u8>`] with the raw pixel data.
pub struct BytesDecoder<'a> {
    vulkan_decoder: VulkanDecoder<'a>,
    parser: Parser,
    frame_sorter: FrameSorter<RawFrameData>,
}

impl BytesDecoder<'_> {
    /// The result is a sequence of frames. Te payload of each [`Frame`] struct is a [`Vec<u8>`]. Each [`Vec<u8>`] contains a single
    /// decoded frame in the [NV12 format](https://en.wikipedia.org/wiki/YCbCr#4:2:0).
    ///
    /// `pts` is the presentation timestamp -- a number, which describes when the given frame
    /// should be presented, used for synchronization with other tracks, e.g. with audio
    pub fn decode(
        &mut self,
        frame: EncodedChunk<&'_ [u8]>,
    ) -> Result<Vec<Frame<RawFrameData>>, DecoderError> {
        let instructions = self.parser.parse(frame.data, frame.pts)?;

        let unsorted_frames = self.vulkan_decoder.decode_to_bytes(&instructions)?;

        let mut result = Vec::new();

        for unsorted_frame in unsorted_frames {
            let mut sorted_frames = self.frame_sorter.put(unsorted_frame);
            result.append(&mut sorted_frames);
        }

        Ok(result)
    }

    /// Flush all frames from the decoder.
    ///
    /// Make sure that this is done when you have the knowledge that no more frames will be coming
    /// that need to be presented before the already decoded frames.
    pub fn flush(&mut self) -> Vec<Frame<RawFrameData>> {
        self.frame_sorter.flush()
    }
}
