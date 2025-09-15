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
//!     let mut decoder = device.create_wgpu_textures_decoder().unwrap();
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
//! On Linux, the library should work on NVIDIA and AMD GPUs out of the box with recent Mesa drivers. For AMD GPUs with a bit older Mesa drivers, you may need to set the `RADV_PERFTEST=video_decode` environment variable:
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

#![cfg(not(target_os = "macos"))]
mod adapter;
mod device;
mod instance;
mod parser;
mod vulkan_decoder;
mod vulkan_encoder;
pub(crate) mod wrappers;

use ash::vk;
use parser::Parser;
use vulkan_decoder::{FrameSorter, VulkanDecoder};

pub use adapter::{AdapterInfo, VulkanAdapter};
pub use device::caps::{EncodeCapabilities, EncodeH264Capabilities, EncodeH264ProfileCapabilities};
pub use device::{EncoderParameters, Rational, VideoParameters, VulkanDevice};
pub use instance::VulkanInstance;
pub use parser::ParserError;
pub use vulkan_decoder::VulkanDecoderError;
pub use vulkan_encoder::{RateControl, VulkanEncoderError};

use crate::vulkan_encoder::VulkanEncoder;

#[derive(Debug, thiserror::Error)]
pub enum DecoderError {
    #[error("Decoder error: {0}")]
    VulkanDecoderError(#[from] VulkanDecoderError),

    #[error("H264 parser error: {0}")]
    ParserError(#[from] ParserError),
}

#[derive(thiserror::Error, Debug)]
pub enum VulkanInitError {
    #[error("Error loading vulkan: {0}")]
    LoadingError(#[from] ash::LoadingError),

    #[error("Vulkan error: {0}")]
    VkError(#[from] vk::Result),

    #[error("wgpu instance error: {0}")]
    WgpuInstanceError(#[from] wgpu::hal::InstanceError),

    #[error("wgpu device error: {0}")]
    WgpuDeviceError(#[from] wgpu::hal::DeviceError),

    #[error("wgpu request device error: {0}")]
    WgpuRequestDeviceError(#[from] wgpu::RequestDeviceError),

    #[error("cannot create a wgpu adapter")]
    WgpuAdapterNotCreated,

    #[error("Cannot find a suitable physical device")]
    NoDevice,

    #[error("String conversion error: {0}")]
    StringConversionError(#[from] std::ffi::FromBytesUntilNulError),
}

#[derive(thiserror::Error, Debug)]
pub enum VulkanCommonError {
    #[error("Vulkan error: {0}")]
    VkError(#[from] vk::Result),

    #[error("Cannot find a queue with index {0}")]
    NoQueue(usize),

    #[error("Memory copy requested to a bufer that is not set up for receiving input")]
    UploadToImproperBuffer,

    #[error("A slot in the Decoded Pictures Buffer was requested, but all slots are taken")]
    NoFreeSlotsInDpb,

    #[error("DPB can have at most 32 slots, {0} was requested")]
    DpbTooLong(u32),
}

/// A profile in H264 is a set of codec features used while encoding a specific video.
/// Baseline uses the fewest features, Main can use more and High even more than Main.
#[derive(Debug, Clone, Copy)]
pub enum H264Profile {
    Baseline,
    Main,
    High,
}

impl H264Profile {
    pub(crate) fn to_profile_idc(self) -> vk::native::StdVideoH264ProfileIdc {
        match self {
            H264Profile::Baseline => {
                vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_BASELINE
            }
            H264Profile::Main => vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_MAIN,
            H264Profile::High => vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_HIGH,
        }
    }
}

/// Represents a chunk of encoded video data used for decoding.
///
/// If `pts` is [`Option::Some`], it is inferred that the chunk contains bytestream that belongs to
/// one output frame.
/// If `pts` is [`Option::None`], the chunk can contain bytestream from multiple consecutive
/// frames.
pub struct EncodedInputChunk<T> {
    pub data: T,
    pub pts: Option<u64>,
}

/// Represents a chunk of encoded video data returned by the encoder.
pub struct EncodedOutputChunk<T> {
    pub data: T,
    pub pts: Option<u64>,
    pub is_keyframe: bool,
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
pub struct WgpuTexturesDecoder {
    vulkan_decoder: VulkanDecoder<'static>,
    parser: Parser,
    frame_sorter: FrameSorter<wgpu::Texture>,
}

impl WgpuTexturesDecoder {
    /// The produced textures have the [`wgpu::TextureFormat::NV12`] format and can be used as a copy source or a texture binding.
    ///
    /// `pts` is the presentation timestamp -- a number, which describes when the given frame
    /// should be presented, used for synchronization with other tracks, e.g. with audio
    pub fn decode(
        &mut self,
        frame: EncodedInputChunk<&[u8]>,
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
pub struct BytesDecoder {
    vulkan_decoder: VulkanDecoder<'static>,
    parser: Parser,
    frame_sorter: FrameSorter<RawFrameData>,
}

impl BytesDecoder {
    /// The result is a sequence of frames. The payload of each [`Frame`] struct is a [`Vec<u8>`]. Each [`Vec<u8>`] contains a single
    /// decoded frame in the [NV12 format](https://en.wikipedia.org/wiki/YCbCr#4:2:0).
    ///
    /// `pts` is the presentation timestamp -- a number, which describes when the given frame
    /// should be presented, used for synchronization with other tracks, e.g. with audio
    pub fn decode(
        &mut self,
        frame: EncodedInputChunk<&[u8]>,
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

/// An encoder that takes input frames as [`Vec<u8>`] with raw pixel data (in RGBA)
pub struct BytesEncoder {
    vulkan_encoder: VulkanEncoder<'static>,
}

impl BytesEncoder {
    /// The result is a chunk of H264 bytecode.
    ///
    /// If the `force_keyframe` option is set to `true`, the encoder will encode this frame as a
    /// [keyframe](https://en.wikipedia.org/wiki/Video_compression_picture_types#Intra-coded_(I)_frames/slices_(key_frames)).
    /// Otherwise, the encoder will decide which frames should be coded this way.
    pub fn encode(
        &mut self,
        frame: &Frame<RawFrameData>,
        force_keyframe: bool,
    ) -> Result<EncodedOutputChunk<Vec<u8>>, VulkanEncoderError> {
        self.vulkan_encoder.encode_bytes(frame, force_keyframe)
    }
}

/// An encoder that takes input frames as [`wgpu::Texture`]s (in [`wgpu::TextureFormat::Rgba8Unorm`])
pub struct WgpuTexturesEncoder {
    vulkan_encoder: VulkanEncoder<'static>,
}

impl WgpuTexturesEncoder {
    /// The result is a chunk of H264 bytecode.
    ///
    /// If the `force_keyframe` option is set to `true`, the encoder will encode this frame as a
    /// [keyframe](https://en.wikipedia.org/wiki/Video_compression_picture_types#Intra-coded_(I)_frames/slices_(key_frames)).
    /// Otherwise, the encoder will decide which frames should be coded this way.
    ///
    /// # Safety
    /// - The texture cannot be a surface texture
    /// - The texture has to be transitioned to [`wgpu::TextureUses::RESOURCE`] usage:
    ///   ```rust
    ///   # let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    ///   # let texture = device.create_texture(&wgpu::TextureDescriptor {
    ///   #     label: None,
    ///   #     size: wgpu::Extent3d {
    ///   #         width: 1280,
    ///   #         height: 720,
    ///   #         depth_or_array_layers: 1,
    ///   #     },
    ///   #     mip_level_count: 1,
    ///   #     sample_count: 1,
    ///   #     dimension: wgpu::TextureDimension::D2,
    ///   #     format: wgpu::TextureFormat::Rgba8Unorm,
    ///   #     usage: wgpu::TextureUsages::TEXTURE_BINDING,
    ///   #     view_formats: &[],
    ///   # });
    ///   let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    ///   encoder.transition_resources(
    ///       [].into_iter(),
    ///       [wgpu::TextureTransition {
    ///           texture: &texture,
    ///           state: wgpu::TextureUses::RESOURCE,
    ///           selector: None,
    ///       }]
    ///       .into_iter(),
    ///   );
    ///   queue.submit([encoder.finish()]);
    ///
    ///   // Now you can use `WgpuTexturesEncoder::encode` on `texture`
    ///   ```
    pub unsafe fn encode(
        &mut self,
        frame: Frame<wgpu::Texture>,
        force_keyframe: bool,
    ) -> Result<EncodedOutputChunk<Vec<u8>>, VulkanEncoderError> {
        self.vulkan_encoder.encode_texture(frame, force_keyframe)
    }
}
