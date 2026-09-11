use objc2_core_foundation as cf;
use objc2_core_video as cv;
use objc2_metal as mtl;

use crate::{
    backends::video_toolbox::{
        decoder::VTDecoder,
        error::{VTDecoderError, VTInitError},
        metal_interop::SyncCache,
        wgpu_api::wgpu_texture_from_pixel_buffer,
    },
    decoders::WgpuVideoDecoderBackend,
    frame_sorter::DecodeResult,
};

impl WgpuVideoDecoderBackend for VTDecoder {
    fn decode_to_wgpu_textures(
        &mut self,
        wgpu_device: &wgpu::Device,
        decoder_instructions: Vec<crate::parser::decoder_instructions::DecoderInstruction>,
    ) -> Result<Vec<crate::frame_sorter::DecodeResult<wgpu::Texture>>, crate::VideoDecoderError>
    {
        let buffers = self.decode_to_cvbuffers(decoder_instructions)?;
        Ok(self.to_wgpu_textures(wgpu_device, buffers)?)
    }
}

impl VTDecoder {
    pub(crate) fn new_wgpu(
        device: &wgpu::Device,
        usage: crate::parameters::DecoderUsage,
    ) -> Result<Self, VTInitError> {
        let mut decoder = Self::new(true, usage);
        decoder.texture_cache = Some(SyncCache::new_from_wgpu(
            device,
            mtl::MTLTextureUsage::ShaderRead,
        )?);
        Ok(decoder)
    }

    fn to_wgpu_textures(
        &self,
        device: &wgpu::Device,
        buffers: Vec<DecodeResult<cf::CFRetained<cv::CVBuffer>>>,
    ) -> Result<Vec<DecodeResult<wgpu::Texture>>, VTDecoderError> {
        buffers
            .into_iter()
            .map(|output_frame| {
                let frame = self.to_wgpu_texture(device, &output_frame.frame)?;
                Ok(DecodeResult {
                    frame,
                    metadata: output_frame.metadata,
                })
            })
            .collect()
    }

    fn to_wgpu_texture(
        &self,
        device: &wgpu::Device,
        buffer: &cv::CVBuffer,
    ) -> Result<wgpu::Texture, VTDecoderError> {
        let Some(cache) = &self.texture_cache else {
            return Err(VTDecoderError::NotConfiguredForWgpuOutput);
        };

        Ok(wgpu_texture_from_pixel_buffer(
            cache,
            device,
            buffer,
            wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::TEXTURE_BINDING,
            wgpu::TextureUses::RESOURCE,
            "gpu-video output",
        )?)
    }
}
