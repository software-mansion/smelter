use objc2::{rc::Retained, runtime::ProtocolObject};
use objc2_core_foundation as cf;
use objc2_core_video as cv;
use objc2_metal as mtl;

use crate::backends::video_toolbox::{
    allocate_retained,
    error::{OSStatusError, VTInitError},
};

pub(crate) struct SyncCache(pub(crate) cf::CFRetained<cv::CVMetalTextureCache>);

unsafe impl Send for SyncCache {}

impl SyncCache {
    pub(crate) fn new_from_mtl(
        device: &ProtocolObject<dyn mtl::MTLDevice>,
        usage: mtl::MTLTextureUsage,
    ) -> Result<Self, VTInitError> {
        let texture_attributes = unsafe {
            cf::CFDictionary::<cf::CFString, cf::CFNumber>::from_slices(
                &[cv::kCVMetalTextureUsage],
                &[cf::CFNumber::new_i64(usage.0 as i64).as_ref()],
            )
        };

        let texture_cache = unsafe {
            allocate_retained(|ptr| {
                cv::CVMetalTextureCache::create(
                    None,
                    None,
                    device,
                    Some(texture_attributes.as_ref()),
                    ptr,
                )
            })?
        };

        Ok(SyncCache(texture_cache))
    }
}

pub(crate) struct SendSyncCVBuffer(pub(crate) cf::CFRetained<cv::CVBuffer>);
unsafe impl Send for SendSyncCVBuffer {}
unsafe impl Sync for SendSyncCVBuffer {}

#[derive(Debug, thiserror::Error)]
pub enum MetalTextureError {
    #[error(transparent)]
    OSStatus(#[from] OSStatusError),

    #[error("Failed to extract Metal texture from CVMetalTexture")]
    ExtractionFailed,
}

pub(crate) struct PlaneTextures {
    pub(crate) y: Retained<ProtocolObject<dyn mtl::MTLTexture>>,
    pub(crate) uv: Retained<ProtocolObject<dyn mtl::MTLTexture>>,
    /// Keeps the CVMetalTextures (and thus the backing pixel buffer) alive while
    /// the MTLTextures are in use.
    #[cfg_attr(not(feature = "wgpu"), expect(dead_code))]
    pub(crate) guards: [SendSyncCVBuffer; 2],
}

pub(crate) fn plane_textures_from_pixel_buffer(
    cache: &SyncCache,
    buffer: &cv::CVBuffer,
) -> Result<PlaneTextures, MetalTextureError> {
    cache.0.flush(0);
    let (y, y_guard) = plane_texture(cache, buffer, mtl::MTLPixelFormat::R8Unorm, 0)?;
    let (uv, uv_guard) = plane_texture(cache, buffer, mtl::MTLPixelFormat::RG8Unorm, 1)?;

    Ok(PlaneTextures {
        y,
        uv,
        guards: [y_guard, uv_guard],
    })
}

fn plane_texture(
    cache: &SyncCache,
    buffer: &cv::CVBuffer,
    format: mtl::MTLPixelFormat,
    plane: usize,
) -> Result<
    (
        Retained<ProtocolObject<dyn mtl::MTLTexture>>,
        SendSyncCVBuffer,
    ),
    MetalTextureError,
> {
    let width = cv::CVPixelBufferGetWidthOfPlane(buffer, plane);
    let height = cv::CVPixelBufferGetHeightOfPlane(buffer, plane);

    let cv_texture = unsafe {
        allocate_retained(|ptr| {
            cv::CVMetalTextureCache::create_texture_from_image(
                None, &cache.0, buffer, None, format, width, height, plane, ptr,
            )
        })?
    };
    let texture =
        cv::CVMetalTextureGetTexture(&cv_texture).ok_or(MetalTextureError::ExtractionFailed)?;

    Ok((texture, SendSyncCVBuffer(cv_texture)))
}
