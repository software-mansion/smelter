use std::ptr::NonNull;

use crate::quicksync::sys as vpl;

use crate::quicksync::va::{DisplayHandle, SurfaceId};

type FrameSurfaceRelease =
    unsafe extern "C" fn(*mut vpl::mfxFrameSurface1) -> vpl::mfxStatus;
type ExportedSurfaceRelease =
    unsafe extern "C" fn(*mut vpl::mfxSurfaceInterface) -> vpl::mfxStatus;

#[derive(Debug, Clone, Copy)]
pub(super) enum Codec {
    H264,
}

impl Codec {
    fn id(self) -> u32 {
        match self {
            Self::H264 => vpl::MFX_CODEC_AVC,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) enum Component {
    Encode,
    Decode,
    VppInput,
}

impl Component {
    fn surface_component(self) -> vpl::mfxSurfaceComponent {
        match self {
            Self::Encode => vpl::mfxSurfaceComponent_MFX_SURFACE_COMPONENT_ENCODE,
            Self::Decode => vpl::mfxSurfaceComponent_MFX_SURFACE_COMPONENT_DECODE,
            Self::VppInput => vpl::mfxSurfaceComponent_MFX_SURFACE_COMPONENT_VPP_INPUT,
        }
    }

    fn codec_filter(self) -> Option<&'static [u8]> {
        match self {
            Self::Encode => {
                Some(b"mfxImplDescription.mfxEncoderDescription.encoder.CodecID\0")
            }
            Self::Decode => {
                Some(b"mfxImplDescription.mfxDecoderDescription.decoder.CodecID\0")
            }
            Self::VppInput => None,
        }
    }
}

const SURFACE_COMPONENT_FILTER: &[u8] =
    b"mfxSurfaceTypesSupported.surftype.surfcomp.SurfaceComponent\0";

#[derive(Debug, thiserror::Error)]
pub(super) enum VplError {
    #[error("{function} failed with oneVPL status {status}")]
    Status { function: &'static str, status: i32 },

    #[error("{0} returned null")]
    Null(&'static str),

    #[error("oneVPL function pointer {0} is unavailable")]
    MissingFunction(&'static str),
}

pub(super) enum SyncStatus {
    Complete,
    Pending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SyncWait {
    Poll,
    Block,
}

impl SyncWait {
    fn timeout(self) -> u32 {
        match self {
            Self::Poll => 0,
            Self::Block => vpl::MFX_INFINITE,
        }
    }
}

pub(super) struct Session {
    _loader: Loader,
    raw: NonNull<vpl::_mfxSession>,
}

impl Session {
    pub(super) fn new(
        render_node: u32,
        codec: Codec,
        component: Component,
        va_display: DisplayHandle,
    ) -> Result<Self, VplError> {
        let loader = Loader::new()?;

        for (name, value) in [
            (
                b"mfxImplDescription.Impl\0" as &'static [u8],
                vpl::mfxImplType_MFX_IMPL_TYPE_HARDWARE,
            ),
            (b"mfxExtendedDeviceId.DRMRenderNodeNum\0", render_node),
            (b"mfxImplDescription.ApiVersion.Version\0", vpl_version(2, 10)),
            (
                b"mfxImplDescription.AccelerationMode\0",
                vpl::mfxAccelerationMode_MFX_ACCEL_MODE_VIA_VAAPI,
            ),
        ] {
            set_filter_u32(loader.raw(), name, value)?;
        }
        if let Some(codec_filter) = component.codec_filter() {
            set_filter_u32(loader.raw(), codec_filter, codec.id())?;
        }

        for (name, value) in [
            (
                b"mfxSurfaceTypesSupported.surftype.SurfaceType\0" as &'static [u8],
                vpl::mfxSurfaceType_MFX_SURFACE_TYPE_VAAPI,
            ),
            (SURFACE_COMPONENT_FILTER, component.surface_component()),
            (
                b"mfxSurfaceTypesSupported.surftype.surfcomp.SurfaceFlags\0",
                vpl::MFX_SURFACE_FLAG_EXPORT_SHARED,
            ),
        ] {
            set_filter_u32(loader.raw(), name, value)?;
        }

        let mut raw = std::ptr::null_mut();
        check_status("MFXCreateSession", unsafe {
            vpl::MFXCreateSession(loader.raw(), 0, &mut raw)
        })?;
        let raw = non_null(raw, "MFXCreateSession")?;
        let session = Self { _loader: loader, raw };
        session.set_va_display(va_display)?;
        Ok(session)
    }

    pub(super) fn raw(&self) -> vpl::mfxSession {
        self.raw.as_ptr()
    }

    fn set_va_display(&self, va_display: DisplayHandle) -> Result<(), VplError> {
        check_status("MFXVideoCORE_SetHandle", unsafe {
            vpl::MFXVideoCORE_SetHandle(
                self.raw(),
                vpl::mfxHandleType_MFX_HANDLE_VA_DISPLAY,
                va_display.as_ptr(),
            )
        })
    }

    pub(super) fn sync_status(
        &self,
        syncp: vpl::mfxSyncPoint,
        wait: SyncWait,
    ) -> Result<SyncStatus, VplError> {
        let status =
            unsafe { vpl::MFXVideoCORE_SyncOperation(self.raw(), syncp, wait.timeout()) };
        match status {
            vpl::mfxStatus_MFX_ERR_NONE => Ok(SyncStatus::Complete),
            vpl::mfxStatus_MFX_WRN_IN_EXECUTION => Ok(SyncStatus::Pending),
            status if status > vpl::mfxStatus_MFX_ERR_NONE => Ok(SyncStatus::Complete),
            status => {
                Err(VplError::Status { function: "MFXVideoCORE_SyncOperation", status })
            }
        }
    }

    pub(super) fn get_surface_for_vpp_input(&self) -> Result<FrameSurface, VplError> {
        let mut surface = std::ptr::null_mut();
        check_status("MFXMemory_GetSurfaceForVPP", unsafe {
            vpl::MFXMemory_GetSurfaceForVPP(self.raw(), &mut surface)
        })?;
        FrameSurface::new(surface)
    }

    pub(super) fn get_surface_for_vpp_output(&self) -> Result<FrameSurface, VplError> {
        let mut surface = std::ptr::null_mut();
        check_status("MFXMemory_GetSurfaceForVPPOut", unsafe {
            vpl::MFXMemory_GetSurfaceForVPPOut(self.raw(), &mut surface)
        })?;
        FrameSurface::new(surface)
    }

    pub(super) fn get_surface_for_encode(&self) -> Result<FrameSurface, VplError> {
        let mut surface = std::ptr::null_mut();
        check_status("MFXMemory_GetSurfaceForEncode", unsafe {
            vpl::MFXMemory_GetSurfaceForEncode(self.raw(), &mut surface)
        })?;
        FrameSurface::new(surface)
    }

    pub(super) fn init_vpp_rgb4_to_nv12(
        &self,
        coded_width: u16,
        coded_height: u16,
        crop_width: u16,
        crop_height: u16,
    ) -> Result<(), VplError> {
        let mut params: vpl::mfxVideoParam = unsafe { std::mem::zeroed() };
        params.IOPattern = (vpl::MFX_IOPATTERN_IN_VIDEO_MEMORY
            | vpl::MFX_IOPATTERN_OUT_VIDEO_MEMORY) as u16;
        unsafe {
            let vpp = &mut params.__bindgen_anon_1.vpp;
            fill_vpp_frame_info(
                &mut vpp.In,
                vpl::MFX_FOURCC_RGB4,
                0,
                coded_width,
                coded_height,
                crop_width,
                crop_height,
            );
            fill_vpp_frame_info(
                &mut vpp.Out,
                vpl::MFX_FOURCC_NV12,
                vpl::MFX_CHROMAFORMAT_YUV420 as u16,
                coded_width,
                coded_height,
                crop_width,
                crop_height,
            );
        }
        check_status_allow_warnings("MFXVideoVPP_Init", unsafe {
            vpl::MFXVideoVPP_Init(self.raw(), &mut params)
        })
    }

    pub(super) fn init_vpp_nv12_to_rgb4(
        &self,
        coded_width: u16,
        coded_height: u16,
        crop_width: u16,
        crop_height: u16,
    ) -> Result<(), VplError> {
        let mut params: vpl::mfxVideoParam = unsafe { std::mem::zeroed() };
        params.IOPattern = (vpl::MFX_IOPATTERN_IN_VIDEO_MEMORY
            | vpl::MFX_IOPATTERN_OUT_VIDEO_MEMORY) as u16;
        unsafe {
            let vpp = &mut params.__bindgen_anon_1.vpp;
            fill_vpp_frame_info(
                &mut vpp.In,
                vpl::MFX_FOURCC_NV12,
                vpl::MFX_CHROMAFORMAT_YUV420 as u16,
                coded_width,
                coded_height,
                crop_width,
                crop_height,
            );
            fill_vpp_frame_info(
                &mut vpp.Out,
                vpl::MFX_FOURCC_RGB4,
                0,
                coded_width,
                coded_height,
                crop_width,
                crop_height,
            );
        }
        check_status_allow_warnings("MFXVideoVPP_Init", unsafe {
            vpl::MFXVideoVPP_Init(self.raw(), &mut params)
        })
    }

    pub(super) fn run_vpp(
        &self,
        input: &FrameSurface,
        output: &FrameSurface,
    ) -> Result<vpl::mfxSyncPoint, VplError> {
        let mut syncp = std::ptr::null_mut();
        check_status_allow_warnings("MFXVideoVPP_RunFrameVPPAsync", unsafe {
            vpl::MFXVideoVPP_RunFrameVPPAsync(
                self.raw(),
                input.raw(),
                output.raw(),
                std::ptr::null_mut(),
                &mut syncp,
            )
        })?;
        Ok(syncp)
    }

    pub(super) fn export_va_surface(
        &self,
        surface: &FrameSurface,
    ) -> Result<ExportedSurface, VplError> {
        let frame_interface = frame_surface_interface(surface.surface)?;
        let export = required_function(
            unsafe { frame_interface.as_ref().Export },
            "mfxFrameSurfaceInterface::Export",
        )?;
        let mut header: vpl::mfxSurfaceHeader = unsafe { std::mem::zeroed() };
        header.SurfaceType = vpl::mfxSurfaceType_MFX_SURFACE_TYPE_VAAPI;
        header.SurfaceFlags = vpl::MFX_SURFACE_FLAG_EXPORT_SHARED;
        header.StructSize = std::mem::size_of::<vpl::mfxSurfaceVAAPI>() as u32;
        let mut exported = std::ptr::null_mut();
        check_status("mfxFrameSurfaceInterface::Export", unsafe {
            export(surface.raw(), header, &mut exported)
        })?;
        let mut surface =
            non_null(exported.cast::<vpl::mfxSurfaceVAAPI>(), "exported surface")?;
        let release = required_function(
            unsafe { surface.as_mut().SurfaceInterface.Release },
            "mfxSurfaceInterface::Release",
        )?;
        Ok(ExportedSurface { surface, release })
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        unsafe {
            let _ = vpl::MFXClose(self.raw());
        }
    }
}

pub(super) struct FrameSurface {
    surface: NonNull<vpl::mfxFrameSurface1>,
    release: FrameSurfaceRelease,
}

impl FrameSurface {
    pub(super) fn new(surface: *mut vpl::mfxFrameSurface1) -> Result<Self, VplError> {
        let surface = non_null(surface, "mfxFrameSurface1")?;
        let frame_interface = frame_surface_interface(surface)?;
        let release = required_function(
            unsafe { frame_interface.as_ref().Release },
            "mfxFrameSurfaceInterface::Release",
        )?;
        Ok(Self { surface, release })
    }

    pub(super) fn raw(&self) -> *mut vpl::mfxFrameSurface1 {
        self.surface.as_ptr()
    }

    pub(super) fn set_timestamp(&mut self, timestamp: u64) {
        unsafe {
            self.surface.as_mut().Data.TimeStamp = timestamp;
        }
    }

    pub(super) fn timestamp(&self) -> u64 {
        unsafe { self.surface.as_ref().Data.TimeStamp }
    }
}

impl Drop for FrameSurface {
    fn drop(&mut self) {
        unsafe {
            let _ = (self.release)(self.raw());
        }
    }
}

pub(super) struct ExportedSurface {
    surface: NonNull<vpl::mfxSurfaceVAAPI>,
    release: ExportedSurfaceRelease,
}

impl ExportedSurface {
    pub(super) fn va_surface_id(&self) -> SurfaceId {
        unsafe { self.surface.as_ref().vaSurfaceID }
    }
}

impl Drop for ExportedSurface {
    fn drop(&mut self) {
        unsafe {
            let interface = &mut self.surface.as_mut().SurfaceInterface;
            let _ = (self.release)(interface);
        }
    }
}

fn fill_vpp_frame_info(
    frame_info: &mut vpl::mfxFrameInfo,
    fourcc: u32,
    chroma_format: u16,
    coded_width: u16,
    coded_height: u16,
    crop_width: u16,
    crop_height: u16,
) {
    frame_info.FourCC = fourcc;
    frame_info.ChromaFormat = chroma_format;
    frame_info.PicStruct = vpl::MFX_PICSTRUCT_PROGRESSIVE as u16;
    frame_info.FrameRateExtN = 30;
    frame_info.FrameRateExtD = 1;
    unsafe {
        let dims = &mut frame_info.__bindgen_anon_1.__bindgen_anon_1;
        dims.Width = coded_width;
        dims.Height = coded_height;
        dims.CropW = crop_width;
        dims.CropH = crop_height;
    }
}

fn frame_surface_interface(
    surface: NonNull<vpl::mfxFrameSurface1>,
) -> Result<NonNull<vpl::mfxFrameSurfaceInterface>, VplError> {
    let frame_interface = unsafe { surface.as_ref().__bindgen_anon_1.FrameInterface };
    non_null(frame_interface, "FrameInterface")
}

struct Loader(NonNull<vpl::_mfxLoader>);

impl Loader {
    fn new() -> Result<Self, VplError> {
        let loader = unsafe { vpl::MFXLoad() };
        non_null(loader, "MFXLoad").map(Self)
    }

    fn raw(&self) -> vpl::mfxLoader {
        self.0.as_ptr()
    }
}

impl Drop for Loader {
    fn drop(&mut self) {
        unsafe { vpl::MFXUnload(self.raw()) };
    }
}

fn set_filter_u32(
    loader: vpl::mfxLoader,
    name: &'static [u8],
    value: u32,
) -> Result<(), VplError> {
    let cfg = create_config(loader)?;
    set_config_filter_u32(cfg, name, value)
}

fn create_config(loader: vpl::mfxLoader) -> Result<vpl::mfxConfig, VplError> {
    let cfg = unsafe { vpl::MFXCreateConfig(loader) };
    Ok(non_null(cfg, "MFXCreateConfig")?.as_ptr())
}

fn non_null<T>(ptr: *mut T, label: &'static str) -> Result<NonNull<T>, VplError> {
    NonNull::new(ptr).ok_or(VplError::Null(label))
}

fn required_function<T>(function: Option<T>, label: &'static str) -> Result<T, VplError> {
    function.ok_or(VplError::MissingFunction(label))
}

fn set_config_filter_u32(
    cfg: vpl::mfxConfig,
    name: &'static [u8],
    value: u32,
) -> Result<(), VplError> {
    check_status("MFXSetConfigFilterProperty", unsafe {
        vpl::MFXSetConfigFilterProperty(cfg, name.as_ptr(), variant_u32(value))
    })
}

fn variant_u32(value: u32) -> vpl::mfxVariant {
    let mut variant: vpl::mfxVariant = unsafe { std::mem::zeroed() };
    variant.Type = vpl::mfxVariantType_MFX_VARIANT_TYPE_U32;
    variant.Data.U32 = value;
    variant
}

fn vpl_version(major: u32, minor: u32) -> u32 {
    (major << 16) | minor
}

fn check_status(function: &'static str, status: vpl::mfxStatus) -> Result<(), VplError> {
    if status == vpl::mfxStatus_MFX_ERR_NONE {
        Ok(())
    } else {
        Err(VplError::Status { function, status })
    }
}

pub(super) fn check_status_allow_warnings(
    function: &'static str,
    status: vpl::mfxStatus,
) -> Result<(), VplError> {
    if status >= vpl::mfxStatus_MFX_ERR_NONE {
        Ok(())
    } else {
        Err(VplError::Status { function, status })
    }
}
