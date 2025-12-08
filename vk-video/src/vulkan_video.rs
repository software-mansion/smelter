pub mod capabilities {
    pub use crate::adapter::AdapterInfo;
    pub use crate::device::caps::{
        DecodeCapabilities, DecodeH264Capabilities, DecodeH264ProfileCapabilities,
        EncodeCapabilities, EncodeH264Capabilities, EncodeH264ProfileCapabilities,
    };
}

pub mod parameters {
    pub use crate::device::{
        DecoderParameters, EncoderParameters, MissedFrameHandling, Rational, VideoParameters,
    };
    pub use crate::vulkan_encoder::RateControl;

    pub use ash::vk::VideoDecodeUsageFlagsKHR as DecoderUsageFlags;

    pub use ash::vk::VideoEncodeContentFlagsKHR as EncoderContentFlags;
    pub use ash::vk::VideoEncodeTuningModeKHR as EncoderTuningMode;
    pub use ash::vk::VideoEncodeUsageFlagsKHR as EncoderUsageFlags;

    /// A profile in H264 is a set of codec features used while encoding a specific video.
    /// Baseline uses the fewest features, Main can use more and High even more than Main.
    #[derive(Debug, Clone, Copy)]
    pub enum H264Profile {
        Baseline,
        Main,
        High,
    }

    impl H264Profile {
        pub(crate) fn to_profile_idc(self) -> ash::vk::native::StdVideoH264ProfileIdc {
            match self {
                H264Profile::Baseline => {
                    ash::vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_BASELINE
                }
                H264Profile::Main => {
                    ash::vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_MAIN
                }
                H264Profile::High => {
                    ash::vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_HIGH
                }
            }
        }
    }
}

use crate::vulkan_decoder::{FrameSorter, VulkanDecoder};
use ash::vk;

pub use crate::adapter::VulkanAdapter;
pub use crate::device::VulkanDevice;
pub use crate::instance::VulkanInstance;
pub use crate::parser::{h264::H264ParserError, reference_manager::ReferenceManagementError};
pub use crate::vulkan_decoder::VulkanDecoderError;
pub use crate::vulkan_encoder::VulkanEncoderError;

use crate::parser::{
    decoder_instructions::compile_to_decoder_instructions, h264::H264Parser,
    reference_manager::ReferenceContext,
};
use crate::vulkan_encoder::VulkanEncoder;
use crate::wrappers::ImageKey;

#[derive(Debug, thiserror::Error)]
pub enum DecoderError {
    #[error("Decoder error: {0}")]
    VulkanDecoderError(#[from] VulkanDecoderError),

    #[error("H264 parser error: {0}")]
    ParserError(#[from] H264ParserError),

    #[error("Reference management error: {0}")]
    ReferenceManagementError(#[from] ReferenceManagementError),
}

#[derive(thiserror::Error, Debug)]
pub enum VulkanInitError {
    #[error("Error loading vulkan: {0}")]
    LoadingError(#[from] ash::LoadingError),

    #[error("Vulkan error: {0}")]
    VkError(#[from] vk::Result),

    #[error("Wgpu instance error: {0}")]
    WgpuInstanceError(#[from] wgpu::hal::InstanceError),

    #[error("Wgpu device error: {0}")]
    WgpuDeviceError(#[from] wgpu::hal::DeviceError),

    #[error("Wgpu request device error: {0}")]
    WgpuRequestDeviceError(#[from] wgpu::RequestDeviceError),

    #[error("Cannot create a wgpu adapter")]
    WgpuAdapterNotCreated,

    #[error("Cannot find a suitable physical device")]
    NoDevice,

    #[error("String conversion error: {0}")]
    StringConversionError(#[from] std::ffi::FromBytesUntilNulError),

    #[error("Profile does not support NV12 texture format")]
    NoNV12ProfileSupport,
}

#[derive(thiserror::Error, Debug)]
pub enum VulkanCommonError {
    #[error("Vulkan error: {0}")]
    VkError(#[from] vk::Result),

    #[error("Cannot find a queue with index {0}")]
    NoQueue(usize),

    #[error("Memory copy requested to a buffer that is not set up for receiving input")]
    UploadToImproperBuffer,

    #[error("A slot in the Decoded Pictures Buffer was requested, but all slots are taken")]
    NoFreeSlotsInDpb,

    #[error("DPB can have at most 32 slots, {0} was requested")]
    DpbTooLong(u32),

    #[error("Tried to create a semaphore submit that waits for an unsignaled value")]
    SemaphoreSubmitWaitOnUnsignaledValue,

    #[error("Tried to register {0:x?} as a new image, while it already exists")]
    RegisteredNewImageTwice(ImageKey),

    #[error("Tried to access state of image {0:x?}, which does not exist")]
    TriedToAccessNonexistentImageState(ImageKey),

    #[error("Tried to unregister image {0:x?} that was not registered")]
    UnregisteredNonexistentImage(ImageKey),
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
    pub(crate) vulkan_decoder: VulkanDecoder<'static>,
    pub(crate) parser: H264Parser,
    pub(crate) reference_ctx: ReferenceContext,
    pub(crate) frame_sorter: FrameSorter<wgpu::Texture>,
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
        let nalus = self.parser.parse(frame.data, frame.pts)?;
        let instructions = compile_to_decoder_instructions(&mut self.reference_ctx, nalus)?;

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
    pub fn flush(&mut self) -> Result<Vec<Frame<wgpu::Texture>>, DecoderError> {
        let nalus = self.parser.flush()?;
        let instructions = compile_to_decoder_instructions(&mut self.reference_ctx, nalus)?;
        let unsorted_frames = self.vulkan_decoder.decode_to_wgpu_textures(&instructions)?;

        let mut result = Vec::new();
        for unsorted_frame in unsorted_frames {
            let mut sorted_frames = self.frame_sorter.put(unsorted_frame);
            result.append(&mut sorted_frames);
        }

        result.append(&mut self.frame_sorter.flush());

        Ok(result)
    }

    /// Notify the decoder that a chunk of the bitstream was lost.
    ///
    /// What the decoder will do depends on the set [`parameters::MissedFrameHandling`]
    pub fn mark_missing_data(&mut self) {
        self.reference_ctx.mark_missed_frames();
    }
}

/// A decoder that outputs frames stored as [`Vec<u8>`] with the raw pixel data.
pub struct BytesDecoder {
    pub(crate) vulkan_decoder: VulkanDecoder<'static>,
    pub(crate) parser: H264Parser,
    pub(crate) reference_ctx: ReferenceContext,
    pub(crate) frame_sorter: FrameSorter<RawFrameData>,
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
        let nalus = self.parser.parse(frame.data, frame.pts)?;
        let instructions = compile_to_decoder_instructions(&mut self.reference_ctx, nalus)?;

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
    pub fn flush(&mut self) -> Result<Vec<Frame<RawFrameData>>, DecoderError> {
        let nalus = self.parser.flush()?;
        let instructions = compile_to_decoder_instructions(&mut self.reference_ctx, nalus)?;
        let unsorted_frames = self.vulkan_decoder.decode_to_bytes(&instructions)?;

        let mut result = Vec::new();
        for unsorted_frame in unsorted_frames {
            let mut sorted_frames = self.frame_sorter.put(unsorted_frame);
            result.append(&mut sorted_frames);
        }

        result.append(&mut self.frame_sorter.flush());

        Ok(result)
    }

    /// Notify the decoder that a chunk of the bitstream was lost.
    ///
    /// What the decoder will do depends on the set [`parameters::MissedFrameHandling`]
    pub fn mark_missing_data(&mut self) {
        self.reference_ctx.mark_missed_frames();
    }
}

/// An encoder that takes input frames as [`Vec<u8>`] with raw pixel data (in NV12)
pub struct BytesEncoder {
    pub(crate) vulkan_encoder: VulkanEncoder<'static>,
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
    pub(crate) vulkan_encoder: VulkanEncoder<'static>,
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
        unsafe { self.vulkan_encoder.encode_texture(frame, force_keyframe) }
    }
}
