//! A library for hardware video coding using Vulkan Video, with [`wgpu`] integration.
//!
//! To start using the API, create a [`VulkanInstance`]

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

pub struct RawFrameData<T> {
    pub frame: T,
    pub width: u32,
    pub height: u32,
}

pub type OwnedRawFrameData = RawFrameData<Vec<u8>>;
pub type BorrowedRawFrameData<'a> = RawFrameData<&'a [u8]>;

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
    frame_sorter: FrameSorter<OwnedRawFrameData>,
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
    ) -> Result<Vec<Frame<OwnedRawFrameData>>, DecoderError> {
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
    pub fn flush(&mut self) -> Vec<Frame<OwnedRawFrameData>> {
        self.frame_sorter.flush()
    }
}
