#![cfg(not(target_os = "macos"))]
mod parser;
pub(crate) mod vulkan_ctx;
mod vulkan_decoder;
mod vulkan_encoder;
pub(crate) mod wrappers;

use parser::Parser;
use vulkan_decoder::{FrameSorter, VulkanDecoder};

pub use parser::ParserError;
pub use vulkan_ctx::{VulkanCtxError, VulkanDevice, VulkanInstance};
pub use vulkan_decoder::VulkanDecoderError;

#[derive(Debug, thiserror::Error)]
pub enum DecoderError {
    #[error("Decoder error: {0}")]
    VulkanDecoderError(#[from] VulkanDecoderError),

    #[error("H264 parser error: {0}")]
    ParserError(#[from] ParserError),
}

pub struct Frame<T> {
    pub frame: T,
    pub pts: Option<u64>,
}

pub struct RawFrame {
    data: Vec<u8>,
    width: u32,
    height: u32,
}

pub struct WgpuTexturesDecoder<'a> {
    vulkan_decoder: VulkanDecoder<'a>,
    parser: Parser,
    frame_sorter: FrameSorter<wgpu::Texture>,
}

impl WgpuTexturesDecoder<'_> {
    // TODO: the below hasn't been verified.
    /// The produced textures have the [`wgpu::TextureFormat::NV12`] format and can be used as a copy source or a texture binding.
    pub fn decode(
        &mut self,
        h264_bytestream: &[u8],
        pts: Option<u64>,
    ) -> Result<Vec<Frame<wgpu::Texture>>, DecoderError> {
        let instructions = self.parser.parse(h264_bytestream, pts)?;

        let unsorted_frames = self.vulkan_decoder.decode_to_wgpu_textures(&instructions)?;

        let mut result = Vec::new();

        for unsorted_frame in unsorted_frames {
            let mut sorted_frames = self.frame_sorter.put(unsorted_frame);
            result.append(&mut sorted_frames);
        }

        Ok(result)
    }
}

pub struct BytesDecoder<'a> {
    vulkan_decoder: VulkanDecoder<'a>,
    parser: Parser,
    frame_sorter: FrameSorter<Vec<u8>>,
}

impl BytesDecoder<'_> {
    /// The result is a sequence of frames. Te payload of each [`Frame`] struct is a [`Vec<u8>`]. Each [`Vec<u8>`] contains a single
    /// decoded frame in the [NV12 format](https://en.wikipedia.org/wiki/YCbCr#4:2:0).
    pub fn decode(
        &mut self,
        h264_bytestream: &[u8],
        pts: Option<u64>,
    ) -> Result<Vec<Frame<Vec<u8>>>, DecoderError> {
        let instructions = self.parser.parse(h264_bytestream, pts)?;

        let unsorted_frames = self.vulkan_decoder.decode_to_bytes(&instructions)?;

        let mut result = Vec::new();

        for unsorted_frame in unsorted_frames {
            let mut sorted_frames = self.frame_sorter.put(unsorted_frame);
            result.append(&mut sorted_frames);
        }

        Ok(result)
    }
}
