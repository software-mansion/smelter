use std::{
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    path::{Path, PathBuf},
    ptr::NonNull,
    sync::Arc,
};

use drm_fourcc::DrmFourcc;

use crate::{
    VideoResolution,
    dmabuf::{
        DmaBufError, DmaBufObject, DmaBufPlane, Nv12DmaBufDescriptor, Nv12DmaBufLayer,
    },
};

const DRM_FORMAT_R8: u32 = DrmFourcc::R8 as u32;
const DRM_FORMAT_GR88: u32 = DrmFourcc::Gr88 as u32;

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

    #[error("DRM PRIME descriptor has unsupported fourcc {0}")]
    UnsupportedFourcc(u32),

    #[error(
        "DRM PRIME NV12 descriptor must have either one 2-plane layer or two 1-plane layers"
    )]
    UnsupportedNv12Layout,

    #[error(transparent)]
    InvalidNv12Descriptor(#[from] DmaBufError),
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

    pub(super) fn export_surface(
        &self,
        surface_id: SurfaceId,
    ) -> Result<DrmPrimeDescriptor, VaError> {
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
        DrmPrimeDescriptor::new(descriptor)
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

pub(super) struct DrmPrimeDescriptor {
    pub(super) nv12: Nv12DmaBufDescriptor,
}

impl DrmPrimeDescriptor {
    fn new(descriptor: ffi::VADRMPRIMESurfaceDescriptor) -> Result<Self, VaError> {
        if !(1..=4).contains(&descriptor.num_objects) {
            return Err(VaError::InvalidObjectCount(descriptor.num_objects));
        }
        let object_count = descriptor.num_objects as usize;
        let objects = descriptor.objects[..object_count]
            .iter()
            .map(|object| DmaBufObject {
                fd: Arc::new(unsafe { OwnedFd::from_raw_fd(object.fd) }),
                size: object.size,
                modifier: object.drm_format_modifier,
            })
            .collect::<Box<_>>();
        let layer = nv12_layer(&descriptor)?;
        let resolution =
            VideoResolution { width: descriptor.width, height: descriptor.height };
        Ok(Self { nv12: Nv12DmaBufDescriptor::new(resolution, objects, layer)? })
    }
}

fn nv12_layer(
    descriptor: &ffi::VADRMPRIMESurfaceDescriptor,
) -> Result<Nv12DmaBufLayer, VaError> {
    if descriptor.fourcc != crate::dmabuf::DRM_FORMAT_NV12 {
        return Err(VaError::UnsupportedFourcc(descriptor.fourcc));
    }
    let prime_plane = |layer_index: usize, plane: usize| {
        let layer = &descriptor.layers[layer_index];
        DmaBufPlane {
            object_index: layer.object_index[plane] as usize,
            offset: layer.offset[plane],
            pitch: layer.pitch[plane],
        }
    };

    match descriptor.num_layers {
        1 if descriptor.layers[0].num_planes == 2 => {
            let layer = &descriptor.layers[0];
            if layer.drm_format != crate::dmabuf::DRM_FORMAT_NV12 {
                return Err(VaError::UnsupportedFourcc(layer.drm_format));
            }
            Ok(Nv12DmaBufLayer { planes: [prime_plane(0, 0), prime_plane(0, 1)] })
        }
        2 if descriptor.layers[0].num_planes == 1
            && descriptor.layers[1].num_planes == 1 =>
        {
            let y_layer = &descriptor.layers[0];
            let uv_layer = &descriptor.layers[1];
            if y_layer.drm_format != DRM_FORMAT_R8 {
                return Err(VaError::UnsupportedFourcc(y_layer.drm_format));
            }
            if uv_layer.drm_format != DRM_FORMAT_GR88 {
                return Err(VaError::UnsupportedFourcc(uv_layer.drm_format));
            }
            Ok(Nv12DmaBufLayer { planes: [prime_plane(0, 0), prime_plane(1, 0)] })
        }
        _ => Err(VaError::UnsupportedNv12Layout),
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
