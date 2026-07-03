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

    pub(super) fn export_nv12_surface(
        &self,
        surface_id: SurfaceId,
    ) -> Result<DrmPrimeNv12Surface, VaError> {
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
        DrmPrimeNv12Surface::new(descriptor)
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

pub(super) struct DrmPrimeNv12Surface {
    pub(super) fd: OwnedFd,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) modifier: u64,
    pub(super) y_offset: u32,
    pub(super) y_pitch: u32,
    pub(super) uv_offset: u32,
    pub(super) uv_pitch: u32,
}

impl DrmPrimeNv12Surface {
    fn new(descriptor: ffi::VADRMPRIMESurfaceDescriptor) -> Result<Self, VaError> {
        if descriptor.num_objects != 1 {
            return Err(VaError::InvalidObjectCount(descriptor.num_objects));
        }
        if descriptor.fourcc.to_le_bytes() != *b"NV12"
            || descriptor.num_layers != 2
            || descriptor.layers[0].num_planes != 1
            || descriptor.layers[1].num_planes != 1
        {
            return Err(VaError::UnsupportedSinglePlaneLayout);
        }
        let object = &descriptor.objects[0];
        Ok(Self {
            fd: unsafe { OwnedFd::from_raw_fd(object.fd) },
            width: descriptor.width,
            height: descriptor.height,
            modifier: object.drm_format_modifier,
            y_offset: descriptor.layers[0].offset[0],
            y_pitch: descriptor.layers[0].pitch[0],
            uv_offset: descriptor.layers[1].offset[0],
            uv_pitch: descriptor.layers[1].pitch[0],
        })
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
