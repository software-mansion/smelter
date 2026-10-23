use std::{cell::Cell, ptr::NonNull};

use objc2::{Message as _, rc::Retained, runtime::ProtocolObject};
use objc2_core_foundation as cf;
use objc2_core_video as cv;
use objc2_io_surface::IOSurfaceRef;
#[cfg(feature = "transcoder")]
use objc2_metal::MTLCommandBuffer as _;
use objc2_metal::{self as mtl, MTLDevice as _, MTLSharedEvent as _};

/// Makes Metal textures over the planes of pixel buffers' IOSurfaces. Unlike a
/// `CVMetalTextureCache` texture, whose release updates the cache, a texture made here only holds
/// its pixel buffer, so dropping it on any thread races with nothing.
pub(crate) struct IOSurfaceTextures {
    device: Retained<ProtocolObject<dyn mtl::MTLDevice>>,
    usage: mtl::MTLTextureUsage,
}

impl IOSurfaceTextures {
    pub(crate) fn new_from_mtl(
        device: &ProtocolObject<dyn mtl::MTLDevice>,
        usage: mtl::MTLTextureUsage,
    ) -> Self {
        Self {
            device: device.retain(),
            usage,
        }
    }

    fn plane(
        &self,
        surface: &IOSurfaceRef,
        buffer: &cv::CVBuffer,
        format: mtl::MTLPixelFormat,
        plane: usize,
    ) -> Result<Retained<ProtocolObject<dyn mtl::MTLTexture>>, MetalTextureError> {
        let width = cv::CVPixelBufferGetWidthOfPlane(buffer, plane);
        let height = cv::CVPixelBufferGetHeightOfPlane(buffer, plane);
        let descriptor = unsafe {
            mtl::MTLTextureDescriptor::texture2DDescriptorWithPixelFormat_width_height_mipmapped(
                format, width, height, false,
            )
        };
        descriptor.setUsage(self.usage);
        self.device
            .newTextureWithDescriptor_iosurface_plane(&descriptor, surface, plane)
            .ok_or(MetalTextureError::TextureCreationFailed(plane))
    }
}

pub(crate) struct SendSyncCVBuffer(pub(crate) cf::CFRetained<cv::CVBuffer>);
unsafe impl Send for SendSyncCVBuffer {}
unsafe impl Sync for SendSyncCVBuffer {}

#[derive(Debug, thiserror::Error)]
pub enum MetalTextureError {
    #[error("The pixel buffer is not backed by an IOSurface")]
    NoIOSurface,

    #[error("Failed to create a Metal texture on plane {0} of the pixel buffer's IOSurface")]
    TextureCreationFailed(usize),
}

pub(crate) struct PlaneTextures {
    pub(crate) y: Retained<ProtocolObject<dyn mtl::MTLTexture>>,
    pub(crate) uv: Retained<ProtocolObject<dyn mtl::MTLTexture>>,
    /// Keeps the pixel buffer, which VideoToolbox reuses only once released, alive while the
    /// MTLTextures are in use.
    #[cfg_attr(not(feature = "wgpu"), expect(dead_code))]
    pub(crate) buffer: SendSyncCVBuffer,
}

pub(crate) fn plane_textures_from_pixel_buffer(
    textures: &IOSurfaceTextures,
    buffer: &cf::CFRetained<cv::CVBuffer>,
) -> Result<PlaneTextures, MetalTextureError> {
    let surface =
        cv::CVPixelBufferGetIOSurface(Some(buffer)).ok_or(MetalTextureError::NoIOSurface)?;
    Ok(PlaneTextures {
        y: textures.plane(&surface, buffer, mtl::MTLPixelFormat::R8Unorm, 0)?,
        uv: textures.plane(&surface, buffer, mtl::MTLPixelFormat::RG8Unorm, 1)?,
        buffer: SendSyncCVBuffer(buffer.clone()),
    })
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
