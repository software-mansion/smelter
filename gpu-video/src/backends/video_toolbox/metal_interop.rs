use std::{cell::Cell, ptr::NonNull, sync::Mutex};

use objc2::{rc::Retained, runtime::ProtocolObject};
use objc2_core_foundation as cf;
use objc2_core_video as cv;
#[cfg(feature = "transcoder")]
use objc2_metal::MTLCommandBuffer as _;
use objc2_metal::{self as mtl, MTLSharedEvent as _};

use crate::backends::video_toolbox::{
    allocate_retained,
    error::{OSStatusError, VTInitError},
};

pub(crate) struct SyncCache(Mutex<MetalTextureCache>);

struct MetalTextureCache(cf::CFRetained<cv::CVMetalTextureCache>);

// Safety: texture caches are not marked in docs as thread-affine (required to be used on the
// thread that created them)
unsafe impl Send for MetalTextureCache {}

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

        Ok(SyncCache(Mutex::new(MetalTextureCache(texture_cache))))
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
    let cache = cache.0.lock().unwrap();
    cache.0.flush(0);
    let (y, y_guard) = plane_texture(&cache.0, buffer, mtl::MTLPixelFormat::R8Unorm, 0)?;
    let (uv, uv_guard) = plane_texture(&cache.0, buffer, mtl::MTLPixelFormat::RG8Unorm, 1)?;

    Ok(PlaneTextures {
        y,
        uv,
        guards: [y_guard, uv_guard],
    })
}

fn plane_texture(
    cache: &cv::CVMetalTextureCache,
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
                None, cache, buffer, None, format, width, height, plane, ptr,
            )
        })?
    };
    let texture =
        cv::CVMetalTextureGetTexture(&cv_texture).ok_or(MetalTextureError::ExtractionFailed)?;

    Ok((texture, SendSyncCVBuffer(cv_texture)))
}

pub(crate) struct SharedEventTimeline {
    event: Retained<ProtocolObject<dyn mtl::MTLSharedEvent>>,
    next_value: u64,
}

impl SharedEventTimeline {
    pub(crate) fn new(event: Retained<ProtocolObject<dyn mtl::MTLSharedEvent>>) -> Self {
        Self {
            event,
            next_value: 1,
        }
    }

    pub(crate) fn next_timeline_point(&mut self) -> TimelinePoint {
        let value = self.next_value;
        self.next_value += 1;
        TimelinePoint {
            event: self.event.clone(),
            value,
        }
    }
}

#[derive(Clone)]
pub(crate) struct TimelinePoint {
    event: Retained<ProtocolObject<dyn mtl::MTLSharedEvent>>,
    value: u64,
}

impl TimelinePoint {
    #[cfg(feature = "wgpu")]
    pub(crate) fn value(&self) -> u64 {
        self.value
    }

    #[cfg(feature = "transcoder")]
    pub(crate) fn signal(&self) {
        self.event.setSignaledValue(self.value);
    }

    pub(crate) fn notify_listener(
        &self,
        listener: &mtl::MTLSharedEventListener,
        on_reached: impl FnOnce() + Send + 'static,
    ) {
        let on_reached = Cell::new(Some(on_reached));
        let block = block2::RcBlock::new(
            move |_event: NonNull<ProtocolObject<dyn mtl::MTLSharedEvent>>, _value: u64| {
                if let Some(on_reached) = on_reached.take() {
                    on_reached();
                }
            },
        );

        // Safety: Metal copies the block, which owns everything it needs.
        unsafe {
            self.event.notifyListener_atValue_block(
                listener,
                self.value,
                block2::RcBlock::as_ptr(&block),
            )
        };
    }
}

#[cfg(feature = "transcoder")]
pub(crate) fn add_completed_handler(
    cmd: &ProtocolObject<dyn mtl::MTLCommandBuffer>,
    on_completed: impl FnOnce(&ProtocolObject<dyn mtl::MTLCommandBuffer>) + Send + 'static,
) {
    let on_completed = Cell::new(Some(on_completed));
    let block = block2::RcBlock::new(
        move |cmd: NonNull<ProtocolObject<dyn mtl::MTLCommandBuffer>>| {
            if let Some(on_completed) = on_completed.take() {
                // Safety: Metal passes a valid command buffer that outlives the handler call.
                on_completed(unsafe { cmd.as_ref() });
            }
        },
    );

    // Safety: Metal copies the block, which owns everything it needs.
    unsafe { cmd.addCompletedHandler(block2::RcBlock::as_ptr(&block)) };
}
