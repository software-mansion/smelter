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

    pub(super) fn export_surface_layout(
        &self,
        surface_id: SurfaceId,
    ) -> Result<DrmPrimeSurfaceLayout, VaError> {
        let mut descriptor =
            unsafe { std::mem::zeroed::<ffi::VADRMPRIMESurfaceDescriptor>() };
        check_status("vaExportSurfaceHandle", unsafe {
            ffi::vaExportSurfaceHandle(
                self.handle.as_ptr(),
                surface_id,
                ffi::VA_SURFACE_ATTRIB_MEM_TYPE_DRM_PRIME_2,
                ffi::VA_EXPORT_SURFACE_READ_WRITE,
                &mut descriptor as *mut _ as *mut std::ffi::c_void,
            )
        })?;
        DrmPrimeSurfaceLayout::new(descriptor)
    }

    pub(super) fn export_single_plane_surface(
        &self,
        surface_id: SurfaceId,
    ) -> Result<DrmPrimeSinglePlaneSurface, VaError> {
        let mut descriptor =
            unsafe { std::mem::zeroed::<ffi::VADRMPRIMESurfaceDescriptor>() };
        check_status("vaExportSurfaceHandle", unsafe {
            ffi::vaExportSurfaceHandle(
                self.handle.as_ptr(),
                surface_id,
                ffi::VA_SURFACE_ATTRIB_MEM_TYPE_DRM_PRIME_2,
                ffi::VA_EXPORT_SURFACE_READ_WRITE,
                &mut descriptor as *mut _ as *mut std::ffi::c_void,
            )
        })?;
        DrmPrimeSinglePlaneSurface::new(descriptor)
    }
}

impl Drop for VaDisplay {
    fn drop(&mut self) {
        unsafe {
            ffi::vaTerminate(self.handle.as_ptr());
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct DrmPrimeSurfaceLayout {
    pub(super) fourcc: u32,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) objects: Box<[DrmPrimeObjectLayout]>,
    pub(super) layers: Box<[DrmPrimeLayerLayout]>,
}

pub(super) struct DrmPrimeSinglePlaneSurface {
    pub(super) fd: OwnedFd,
    pub(super) fourcc: u32,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) size: u32,
    pub(super) modifier: u64,
    pub(super) offset: u32,
    pub(super) pitch: u32,
}

#[derive(Debug, Clone)]
pub(super) struct DrmPrimeObjectLayout {
    pub(super) modifier: u64,
}

#[derive(Debug, Clone)]
pub(super) struct DrmPrimeLayerLayout {
    pub(super) planes: Box<[DrmPrimePlaneLayout]>,
}

#[derive(Debug, Clone)]
pub(super) struct DrmPrimePlaneLayout {
    pub(super) object_index: u32,
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
            size: object.size,
            modifier: object.drm_format_modifier,
            offset: layer.offset[0],
            pitch: layer.pitch[0],
        })
    }
}

impl DrmPrimeSurfaceLayout {
    fn new(descriptor: ffi::VADRMPRIMESurfaceDescriptor) -> Result<Self, VaError> {
        if !(1..=4).contains(&descriptor.num_objects) {
            return Err(VaError::InvalidObjectCount(descriptor.num_objects));
        }
        let object_count = descriptor.num_objects as usize;
        let objects = descriptor.objects[..object_count]
            .iter()
            .map(|object| {
                let _fd = unsafe { OwnedFd::from_raw_fd(object.fd) };
                DrmPrimeObjectLayout { modifier: object.drm_format_modifier }
            })
            .collect();
        let layers = descriptor.layers[..descriptor.num_layers as usize]
            .iter()
            .map(|layer| DrmPrimeLayerLayout { planes: layer.planes() })
            .collect();
        Ok(Self {
            fourcc: descriptor.fourcc,
            width: descriptor.width,
            height: descriptor.height,
            objects,
            layers,
        })
    }

    pub(super) fn is_single_plane(&self) -> bool {
        self.objects.len() == 1
            && self.layers.len() == 1
            && self.layers[0].planes.len() == 1
            && self.layers[0].planes[0].object_index == 0
    }
}

trait VaDrmPrimeLayerExt {
    fn planes(&self) -> Box<[DrmPrimePlaneLayout]>;
}

impl VaDrmPrimeLayerExt for ffi::_VADRMPRIMESurfaceDescriptor__bindgen_ty_2 {
    fn planes(&self) -> Box<[DrmPrimePlaneLayout]> {
        (0..self.num_planes as usize)
            .map(|index| DrmPrimePlaneLayout {
                object_index: self.object_index[index],
                pitch: self.pitch[index],
            })
            .collect()
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
