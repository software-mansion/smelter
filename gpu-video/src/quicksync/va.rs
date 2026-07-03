use std::{
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    path::{Path, PathBuf},
    ptr::NonNull,
};

pub(super) type DisplayHandle = NonNull<std::ffi::c_void>;
pub(super) type SurfaceId = ffi::VASurfaceID;

#[derive(Debug, thiserror::Error)]
pub(super) enum VaError {
    #[error("failed to open DRM render node {}: {source}", path.display())]
    OpenDrm { path: PathBuf, source: std::io::Error },

    #[error("vaGetDisplayDRM returned null for {}", .0.display())]
    NullDisplay(PathBuf),

    #[error("{function} failed with VA status {status}")]
    Status { function: &'static str, status: i32 },

    #[error("DRM PRIME descriptor has invalid object count {0}")]
    InvalidObjectCount(u32),

    #[error("DRM PRIME descriptor must contain exactly one single-plane layer")]
    UnsupportedSinglePlaneLayout,
}

pub(super) struct VaDisplay {
    handle: DisplayHandle,
    _drm: OwnedFd,
}

impl VaDisplay {
    pub(super) fn open(path: impl AsRef<Path>) -> Result<Self, VaError> {
        let path = path.as_ref();
        let drm = std::fs::File::options()
            .read(true)
            .write(true)
            .open(path)
            .map_err(|source| VaError::OpenDrm { path: path.to_owned(), source })?;
        let drm = OwnedFd::from(drm);
        let handle = unsafe { ffi::vaGetDisplayDRM(drm.as_raw_fd()) };
        let handle =
            NonNull::new(handle).ok_or_else(|| VaError::NullDisplay(path.to_owned()))?;

        let mut major = 0;
        let mut minor = 0;
        check_status("vaInitialize", unsafe {
            ffi::vaInitialize(handle.as_ptr(), &mut major, &mut minor)
        })?;

        Ok(Self { handle, _drm: drm })
    }

    pub(super) fn handle(&self) -> DisplayHandle {
        self.handle
    }

    pub(super) fn export_single_plane_surface(
        &self,
        surface_id: SurfaceId,
    ) -> Result<DrmPrimeSinglePlaneSurface, VaError> {
        DrmPrimeSinglePlaneSurface::new(self.export_drm_prime_surface(surface_id)?)
    }

    /// Imports an externally-allocated single-object, 2-plane NV12 dma-buf as a
    /// VA surface via `vaCreateSurfaces` + `DRM_PRIME_2`. VA duplicates the fd
    /// internally, so the caller keeps ownership.
    pub(super) fn import_nv12_surface(
        &self,
        layout: ExternalNv12DmaBuf,
    ) -> Result<VaSurface, VaError> {
        let mut descriptor =
            unsafe { std::mem::zeroed::<ffi::VADRMPRIMESurfaceDescriptor>() };
        descriptor.fourcc = ffi::VA_FOURCC_NV12;
        descriptor.width = layout.width;
        descriptor.height = layout.height;
        descriptor.num_objects = 1;
        descriptor.objects[0].fd = layout.fd;
        descriptor.objects[0].size = layout.size;
        descriptor.objects[0].drm_format_modifier = layout.modifier;
        descriptor.num_layers = 1;
        descriptor.layers[0].drm_format = DRM_FORMAT_NV12;
        descriptor.layers[0].num_planes = 2;
        descriptor.layers[0].object_index = [0; 4];
        descriptor.layers[0].offset = [layout.y_offset, layout.uv_offset, 0, 0];
        descriptor.layers[0].pitch = [layout.y_pitch, layout.uv_pitch, 0, 0];

        let mut attribs = unsafe { std::mem::zeroed::<[ffi::VASurfaceAttrib; 2]>() };
        attribs[0].type_ = ffi::VASurfaceAttribType_VASurfaceAttribMemoryType;
        attribs[0].flags = ffi::VA_SURFACE_ATTRIB_SETTABLE;
        attribs[0].value.type_ = ffi::VAGenericValueType_VAGenericValueTypeInteger;
        attribs[0].value.value.i = ffi::VA_SURFACE_ATTRIB_MEM_TYPE_DRM_PRIME_2 as i32;
        attribs[1].type_ =
            ffi::VASurfaceAttribType_VASurfaceAttribExternalBufferDescriptor;
        attribs[1].flags = ffi::VA_SURFACE_ATTRIB_SETTABLE;
        attribs[1].value.type_ = ffi::VAGenericValueType_VAGenericValueTypePointer;
        attribs[1].value.value.p = &mut descriptor as *mut _ as *mut std::ffi::c_void;

        let mut surface_id: SurfaceId = 0;
        check_status("vaCreateSurfaces", unsafe {
            ffi::vaCreateSurfaces(
                self.handle.as_ptr(),
                ffi::VA_RT_FORMAT_YUV420,
                layout.width,
                layout.height,
                &mut surface_id,
                1,
                attribs.as_mut_ptr(),
                2,
            )
        })?;
        Ok(VaSurface { display: self.handle, id: surface_id })
    }

    fn export_drm_prime_surface(
        &self,
        surface_id: SurfaceId,
    ) -> Result<ffi::VADRMPRIMESurfaceDescriptor, VaError> {
        let mut descriptor =
            unsafe { std::mem::zeroed::<ffi::VADRMPRIMESurfaceDescriptor>() };
        check_status("vaExportSurfaceHandle", unsafe {
            ffi::vaExportSurfaceHandle(
                self.handle.as_ptr(),
                surface_id,
                ffi::VA_SURFACE_ATTRIB_MEM_TYPE_DRM_PRIME_2,
                ffi::VA_EXPORT_SURFACE_READ_WRITE | ffi::VA_EXPORT_SURFACE_SEPARATE_LAYERS,
                &mut descriptor as *mut _ as *mut std::ffi::c_void,
            )
        })?;
        Ok(descriptor)
    }
}

impl Drop for VaDisplay {
    fn drop(&mut self) {
        unsafe {
            ffi::vaTerminate(self.handle.as_ptr());
        }
    }
}

pub(super) struct DrmPrimeSinglePlaneSurface {
    pub(super) fd: OwnedFd,
    pub(super) fourcc: u32,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) modifier: u64,
    pub(super) offset: u32,
    pub(super) pitch: u32,
}

impl DrmPrimeSinglePlaneSurface {
    fn new(descriptor: ffi::VADRMPRIMESurfaceDescriptor) -> Result<Self, VaError> {
        if descriptor.num_objects != 1 {
            return Err(VaError::InvalidObjectCount(descriptor.num_objects));
        }
        if descriptor.num_layers != 1 || descriptor.layers[0].num_planes != 1 {
            return Err(VaError::UnsupportedSinglePlaneLayout);
        }
        let object = &descriptor.objects[0];
        let layer = &descriptor.layers[0];
        Ok(Self {
            fd: unsafe { OwnedFd::from_raw_fd(object.fd) },
            fourcc: descriptor.fourcc,
            width: descriptor.width,
            height: descriptor.height,
            modifier: object.drm_format_modifier,
            offset: layer.offset[0],
            pitch: layer.pitch[0],
        })
    }
}

const DRM_FORMAT_NV12: u32 = u32::from_le_bytes(*b"NV12");

/// Layout of an externally-allocated single-object, 2-plane NV12 dma-buf. The
/// raw `fd` is borrowed for the duration of [`VaDisplay::import_nv12_surface`].
pub(super) struct ExternalNv12DmaBuf {
    pub(super) fd: std::os::fd::RawFd,
    pub(super) size: u32,
    pub(super) modifier: u64,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) y_offset: u32,
    pub(super) y_pitch: u32,
    pub(super) uv_offset: u32,
    pub(super) uv_pitch: u32,
}

pub(super) struct VaSurface {
    display: DisplayHandle,
    id: SurfaceId,
}

impl VaSurface {
    pub(super) fn id(&self) -> SurfaceId {
        self.id
    }
}

impl Drop for VaSurface {
    fn drop(&mut self) {
        unsafe {
            ffi::vaDestroySurfaces(self.display.as_ptr(), &mut self.id, 1);
        }
    }
}

fn check_status(function: &'static str, status: ffi::VAStatus) -> Result<(), VaError> {
    if status == ffi::VA_STATUS_SUCCESS as ffi::VAStatus {
        Ok(())
    } else {
        Err(VaError::Status { function, status })
    }
}

mod ffi {
    #![allow(non_upper_case_globals)]
    #![allow(non_camel_case_types)]
    #![allow(non_snake_case)]
    #![allow(dead_code)]
    #![allow(unused_imports)]
    #![allow(clippy::all)]

    include!(concat!(env!("OUT_DIR"), "/va_bindings.rs"));
}
