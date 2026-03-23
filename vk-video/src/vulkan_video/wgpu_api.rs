use crate::{
    DecoderError, EncodedInputChunk, EncodedOutputChunk, Frame, VulkanEncoderError,
    codec::h264::{H264Codec, encode::H264WriteParametersInfo},
    parser::{
        decoder_instructions::compile_to_decoder_instructions,
        h264::{AccessUnit, H264Parser},
        reference_manager::ReferenceContext,
    },
    vulkan_decoder::{FrameSorter, VulkanDecoder},
    vulkan_encoder::VulkanEncoder,
};

/// A decoder that outputs frames stored as [`wgpu::Texture`]s
pub struct WgpuTexturesDecoder {
    pub(crate) vulkan_decoder: VulkanDecoder<'static>,
    pub(crate) parser: H264Parser,
    pub(crate) reference_ctx: ReferenceContext,
    pub(crate) frame_sorter: FrameSorter<wgpu::Texture>,
}

impl WgpuTexturesDecoder {
    /// The produced textures have the [`wgpu::TextureFormat::NV12`] format and can be used as a texture binding.
    pub fn decode(
        &mut self,
        frame: EncodedInputChunk<&[u8]>,
    ) -> Result<Vec<Frame<wgpu::Texture>>, DecoderError> {
        let nalus = self.parser.parse(frame.data, frame.pts)?;
        self.decode_nalus(nalus)
    }

    /// Flush all frames from the decoder.
    ///
    /// Make sure that this is done when you have the knowledge that no more frames will be coming
    /// that need to be presented before the already decoded frames.
    pub fn flush(&mut self) -> Result<Vec<Frame<wgpu::Texture>>, DecoderError> {
        let nalus = self.parser.flush()?;
        let mut frames = self.decode_nalus(nalus)?;
        frames.append(&mut self.frame_sorter.flush());
        Ok(frames)
    }

    /// Notify the decoder that a chunk of the bitstream was lost.
    ///
    /// What the decoder will do depends on the set [`parameters::MissedFrameHandling`](`crate::parameters::MissedFrameHandling`)
    pub fn mark_missing_data(&mut self) {
        self.reference_ctx.mark_missed_frames();
    }

    fn decode_nalus(
        &mut self,
        nalus: Vec<AccessUnit>,
    ) -> Result<Vec<Frame<wgpu::Texture>>, DecoderError> {
        let instructions = compile_to_decoder_instructions(&mut self.reference_ctx, nalus)?;
        let unsorted_frames = self.vulkan_decoder.decode_to_wgpu_textures(&instructions)?;
        let sorted_frames = self.frame_sorter.put_frames(unsorted_frames);
        Ok(sorted_frames)
    }
}

/// An encoder that takes input frames as [`wgpu::Texture`]s (in [`wgpu::TextureFormat::NV12`])
pub struct WgpuTexturesEncoder {
    pub(crate) vulkan_encoder: VulkanEncoder<'static, H264Codec>,
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
    /// - The texture has to be transitioned to [`wgpu::TextureUses::COPY_SRC`] usage:
    ///   ```rust
    ///   # let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor {
    ///   #     required_features: wgpu::Features::TEXTURE_FORMAT_NV12,
    ///   #     ..Default::default()
    ///   # });
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
    ///   #     format: wgpu::TextureFormat::NV12,
    ///   #     usage: wgpu::TextureUsages::COPY_SRC,
    ///   #     view_formats: &[],
    ///   # });
    ///   let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    ///   encoder.transition_resources(
    ///       [].into_iter(),
    ///       [wgpu::TextureTransition {
    ///           texture: &texture,
    ///           state: wgpu::TextureUses::COPY_SRC,
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

    /// Retrieve encoded SPS NAL units from the video session parameters, in Annex B.
    ///
    /// Useful when `inline_stream_params` is `false` and the parameters need to be
    /// sent out-of-band (e.g. in RTMP or MP4 headers).
    pub fn sps(&self) -> Result<Vec<u8>, VulkanEncoderError> {
        self.vulkan_encoder
            .stream_parameters(H264WriteParametersInfo {
                write_sps: true,
                write_pps: false,
            })
    }

    /// Retrieve encoded PPS NAL units from the video session parameters, in Annex B.
    ///
    /// Useful when `inline_stream_params` is `false` and the parameters need to be
    /// sent out-of-band (e.g. in RTMP or MP4 headers).
    pub fn pps(&self) -> Result<Vec<u8>, VulkanEncoderError> {
        self.vulkan_encoder
            .stream_parameters(H264WriteParametersInfo {
                write_sps: false,
                write_pps: true,
            })
    }
}

#[derive(thiserror::Error, Debug)]
pub enum WgpuInitError {
    #[error("Wgpu instance error: {0}")]
    WgpuInstanceError(#[from] wgpu::hal::InstanceError),

    #[error("Wgpu device error: {0}")]
    WgpuDeviceError(#[from] wgpu::hal::DeviceError),

    #[error("Wgpu request device error: {0}")]
    WgpuRequestDeviceError(#[from] wgpu::RequestDeviceError),

    #[error("Cannot create a wgpu adapter")]
    WgpuAdapterNotCreated,
}
