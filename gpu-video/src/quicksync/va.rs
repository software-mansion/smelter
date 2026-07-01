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

    /// POC(dmabuf-import): import an EXTERNAL NV12 dma-buf as a VA surface via
    /// `vaCreateSurfaces` + `VA_SURFACE_ATTRIB_MEM_TYPE_DRM_PRIME_2`. This is the
    /// inverse of `export_surface` and the linchpin of the planned copy-elimination
    /// in the Quick Sync encoder. Single-object, single-layer (2-plane) NV12 only.
    pub(super) fn import_nv12_surface(
        &self,
        layout: ExternalNv12DmaBuf,
    ) -> Result<ImportedVaSurface, VaError> {
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
        descriptor.layers[0].drm_format = crate::dmabuf::DRM_FORMAT_NV12;
        descriptor.layers[0].num_planes = 2;
        descriptor.layers[0].object_index = [0, 0, 0, 0];
        descriptor.layers[0].offset = [layout.y_offset, layout.uv_offset, 0, 0];
        descriptor.layers[0].pitch = [layout.y_pitch, layout.uv_pitch, 0, 0];

        let mut attribs = unsafe { std::mem::zeroed::<[ffi::VASurfaceAttrib; 2]>() };
        attribs[0].type_ = ffi::VASurfaceAttribType_VASurfaceAttribMemoryType;
        attribs[0].flags = ffi::VA_SURFACE_ATTRIB_SETTABLE;
        attribs[0].value.type_ = ffi::VAGenericValueType_VAGenericValueTypeInteger;
        attribs[0].value.value.i = ffi::VA_SURFACE_ATTRIB_MEM_TYPE_DRM_PRIME_2 as i32;
        attribs[1].type_ = ffi::VASurfaceAttribType_VASurfaceAttribExternalBufferDescriptor;
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
        Ok(ImportedVaSurface { display: self.handle, id: surface_id })
    }

    /// Phase 0 feasibility (test-only): import a single-object NV12 dma-buf whose
    /// layer carries N planes — Y, UV, and the render-compression (CCS) aux
    /// plane(s) for a compressed Intel modifier. Reports IMPORT via the returned
    /// surface; the caller checks oneVPL's SHARED vs COPY flag downstream.
    #[cfg(test)]
    pub(super) fn import_nv12_surface_planes(
        &self,
        layout: ExternalNv12DmaBufPlanes,
    ) -> Result<ImportedVaSurface, VaError> {
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
        descriptor.layers[0].drm_format = crate::dmabuf::DRM_FORMAT_NV12;
        descriptor.layers[0].num_planes = layout.planes.len() as u32;
        for (index, plane) in layout.planes.iter().enumerate() {
            descriptor.layers[0].object_index[index] = 0;
            descriptor.layers[0].offset[index] = plane.0;
            descriptor.layers[0].pitch[index] = plane.1;
        }

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
        Ok(ImportedVaSurface { display: self.handle, id: surface_id })
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

/// POC(dmabuf-import): description of an externally-allocated single-object,
/// 2-plane NV12 dma-buf to import into VA. `fd` is borrowed for the duration of
/// `vaCreateSurfaces` only (VA dups it internally), so the caller keeps ownership.
pub(super) struct ExternalNv12DmaBuf {
    pub(super) fd: i32,
    pub(super) size: u32,
    pub(super) modifier: u64,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) y_offset: u32,
    pub(super) y_pitch: u32,
    pub(super) uv_offset: u32,
    pub(super) uv_pitch: u32,
}

/// Phase 0 feasibility (test-only): a single-object NV12 dma-buf described by N
/// planes `(offset, pitch)` — the 2 data planes plus any CCS aux plane(s).
#[cfg(test)]
pub(super) struct ExternalNv12DmaBufPlanes {
    pub(super) fd: i32,
    pub(super) size: u32,
    pub(super) modifier: u64,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) planes: Vec<(u32, u32)>,
}

pub(super) struct ImportedVaSurface {
    display: DisplayHandle,
    id: SurfaceId,
}

impl ImportedVaSurface {
    pub(super) fn id(&self) -> SurfaceId {
        self.id
    }
}

impl Drop for ImportedVaSurface {
    fn drop(&mut self) {
        unsafe {
            ffi::vaDestroySurfaces(self.display.as_ptr(), &mut self.id, 1);
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
