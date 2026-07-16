use objc2_core_foundation as cf;
use objc2_core_video as cv;
use objc2_metal as mtl;
use wgpu::hal::metal::Api as MtlApi;

use crate::{
    backends::video_toolbox::{
        allocate_retained,
        decoder::VTDecoder,
        error::{VTDecoderError, VTInitError},
    },
    decoders::WgpuVideoDecoderBackend,
    frame_sorter::DecodeResult,
};

impl WgpuVideoDecoderBackend for VTDecoder {
    fn decode_to_wgpu_textures(
        &mut self,
        wgpu_device: &wgpu::Device,
        decoder_instructions: &[crate::parser::decoder_instructions::DecoderInstruction],
    ) -> Result<Vec<crate::frame_sorter::DecodeResult<wgpu::Texture>>, crate::VideoDecoderError>
    {
        let buffers = self.decode_to_cvbuffers(decoder_instructions)?;
        Ok(self.to_wgpu_textures(wgpu_device, buffers)?)
    }
}

impl VTDecoder {
    pub(crate) fn new(
        device: Option<&wgpu::Device>,
        usage: crate::parameters::DecoderUsage,
    ) -> Result<Self, VTInitError> {
        let texture_cache = if let Some(device) = device {
            Some(make_texture_cache(device)?)
        } else {
            None
        };

        Ok(Self {
            session: None,
            sps: Default::default(),
            pps: Default::default(),
            needs_session_update: false,
            texture_cache,
            session_color_range: None,
            usage,
        })
    }

    pub(crate) fn output_to_wgpu_textures(&self) -> bool {
        self.texture_cache.is_some()
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

        let width = cv::CVPixelBufferGetWidth(buffer);
        let height = cv::CVPixelBufferGetHeight(buffer);
        let y_width = cv::CVPixelBufferGetWidthOfPlane(buffer, 0);
        let y_height = cv::CVPixelBufferGetHeightOfPlane(buffer, 0);
        let uv_width = cv::CVPixelBufferGetWidthOfPlane(buffer, 1);
        let uv_height = cv::CVPixelBufferGetHeightOfPlane(buffer, 1);

        cache.0.flush(0);
        let texture_y = unsafe {
            allocate_retained(|ptr| {
                cv::CVMetalTextureCache::create_texture_from_image(
                    None,
                    &cache.0,
                    buffer,
                    None,
                    mtl::MTLPixelFormat::R8Unorm,
                    y_width,
                    y_height,
                    0,
                    ptr,
                )
            })?
        };
        let mtl_texture_y = cv::CVMetalTextureGetTexture(&texture_y)
            .ok_or(VTDecoderError::MetalTextureExtractionFailed)?;

        let texture_uv = unsafe {
            allocate_retained(|ptr| {
                cv::CVMetalTextureCache::create_texture_from_image(
                    None,
                    &cache.0,
                    buffer,
                    None,
                    mtl::MTLPixelFormat::RG8Unorm,
                    uv_width,
                    uv_height,
                    1,
                    ptr,
                )
            })?
        };
        let mtl_texture_uv = cv::CVMetalTextureGetTexture(&texture_uv)
            .ok_or(VTDecoderError::MetalTextureExtractionFailed)?;

        let guard_y = SendSyncCVBuffer(texture_y);
        let guard_uv = SendSyncCVBuffer(texture_uv);

        unsafe {
            let texture = wgpu::hal::metal::Device::texture_from_raw_planar(
                [mtl_texture_y, mtl_texture_uv],
                wgpu::TextureFormat::NV12,
                mtl::MTLTextureType::Type2D,
                1,
                1,
                wgpu::hal::CopyExtent {
                    width: width as u32,
                    height: height as u32,
                    depth: 1,
                },
                Some(Box::new(move || {
                    drop(guard_y);
                    drop(guard_uv);
                })),
            );

            let texture = device.create_texture_from_hal::<MtlApi>(
                texture,
                &wgpu::TextureDescriptor {
                    label: Some("gpu-video output"),
                    size: wgpu::Extent3d {
                        width: width as u32,
                        height: height as u32,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::NV12,
                    usage: wgpu::TextureUsages::COPY_SRC
                        | wgpu::TextureUsages::COPY_DST
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                },
                wgpu::TextureUses::RESOURCE,
            );

            Ok(texture)
        }
    }
}

fn make_texture_cache(device: &wgpu::Device) -> Result<SyncCache, VTInitError> {
    let metal_device = unsafe {
        device
            .as_hal::<MtlApi>()
            .ok_or(VTInitError::NotMetalBackend)?
            .raw_device()
            .clone()
    };

    let texture_attributes = unsafe {
        cf::CFDictionary::<cf::CFString, cf::CFNumber>::from_slices(
            &[cv::kCVMetalTextureUsage],
            &[cf::CFNumber::new_i64(mtl::MTLTextureUsage::ShaderRead.0 as i64).as_ref()],
        )
    };

    let texture_cache = unsafe {
        allocate_retained(|ptr| {
            cv::CVMetalTextureCache::create(
                None,
                None,
                &metal_device,
                Some(texture_attributes.as_ref()),
                ptr,
            )
        })?
    };

    let texture_cache = SyncCache(texture_cache);
    Ok(texture_cache)
}

pub(crate) struct SyncCache(pub(crate) cf::CFRetained<cv::CVMetalTextureCache>);
unsafe impl Send for SyncCache {}

#[allow(dead_code)]
struct SendSyncCVBuffer(cf::CFRetained<cv::CVBuffer>);
unsafe impl Send for SendSyncCVBuffer {}
unsafe impl Sync for SendSyncCVBuffer {}
