mod decoder;
mod encoder;
// POC(dmabuf-import): throwaway spike proving external dma-buf -> VA -> oneVPL encode.
#[cfg(test)]
mod import_poc;

use std::{collections::VecDeque, os::fd::AsFd, sync::Arc, time::Duration};

pub use decoder::{QuickSyncH264DecoderError, WgpuTexturesDecoderH264};
pub use crate::dmabuf::StagedDmaBufWrite;
pub use encoder::{
    AcquiredNv12Slot, H264EncodedOutputChunk, H264EncoderConfig, H264EncoderPreset,
    H264EncoderRateControl, H264RateControlError, H264VariableBitrate,
    QuickSyncH264EncoderError, WgpuTexturesEncoderH264, ZeroCopyNv12Pool,
};

use crate::{
    dmabuf::{DmaBufFrame, DmaBufInterop, DmaBufSyncFd, QuickSyncDmaBufSync},
    quicksync::sys as vpl,
    quicksync::{
        display::{DrmRenderNode, quicksync_drm_render_node},
        va::{VaDisplay, VaError},
        vpl::{
            Codec, Component, ExportedSurface, FrameSurface, Session, SyncStatus,
            SyncWait,
        },
    },
};
use wgpu::hal::api::Vulkan as VkApi;

const DEVICE_BUSY_RETRIES: usize = 100;
const QUICKSYNC_ASYNC_DEPTH: u16 = 4;

fn retry_device_busy(
    function: &'static str,
    mut call: impl FnMut() -> vpl::mfxStatus,
) -> Result<vpl::mfxStatus, String> {
    for _ in 0..DEVICE_BUSY_RETRIES {
        let status = call();
        if status != vpl::mfxStatus_MFX_WRN_DEVICE_BUSY {
            return Ok(status);
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    Err(format!("{function} stayed busy after retries"))
}

fn progressive_frame_info(fourcc: u32, chroma_format: u16) -> vpl::mfxFrameInfo {
    let mut frame_info = unsafe { std::mem::zeroed::<vpl::mfxFrameInfo>() };
    frame_info.FourCC = fourcc;
    frame_info.ChromaFormat = chroma_format;
    frame_info.PicStruct = vpl::MFX_PICSTRUCT_PROGRESSIVE as u16;
    frame_info
}

fn nv12_progressive_frame_info() -> vpl::mfxFrameInfo {
    progressive_frame_info(vpl::MFX_FOURCC_NV12, vpl::MFX_CHROMAFORMAT_YUV420 as u16)
}

fn vpl_u16_dimension(name: &str, value: u32) -> Result<u16, String> {
    value.try_into().map_err(|_| format!("H264 {name} {value} exceeds oneVPL limit"))
}

fn init_dmabuf_interop(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> Result<(DmaBufInterop, QuickSyncDmaBufSync), H264SessionError> {
    let interop = DmaBufInterop::new(device)?;
    let sync = QuickSyncDmaBufSync::new(&interop, queue);
    Ok((interop, sync))
}

fn init_dmabuf_sync(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> Result<QuickSyncDmaBufSync, H264SessionError> {
    let interop = DmaBufInterop::new(device)?;
    Ok(QuickSyncDmaBufSync::new(&interop, queue))
}

#[derive(Debug, thiserror::Error)]
pub enum H264SessionError {
    #[error("no Intel Quick Sync DRM render node found")]
    NoRenderNode,

    #[error("DMA-BUF interop failed: {0}")]
    DmaBuf(String),

    #[error("{function} failed with VA status {status}")]
    VaStatus { function: &'static str, status: i32 },

    #[error("VA interop failed: {0}")]
    Va(String),

    #[error("{function} failed with oneVPL status {status}")]
    VplStatus { function: &'static str, status: i32 },

    #[error("oneVPL interop failed: {0}")]
    Vpl(String),
}

impl From<crate::dmabuf::DmaBufError> for H264SessionError {
    fn from(err: crate::dmabuf::DmaBufError) -> Self {
        Self::DmaBuf(err.to_string())
    }
}

impl From<VaError> for H264SessionError {
    fn from(err: VaError) -> Self {
        match err {
            VaError::Status { function, status } => Self::VaStatus { function, status },
            err => Self::Va(err.to_string()),
        }
    }
}

impl From<crate::quicksync::vpl::VplError> for H264SessionError {
    fn from(err: crate::quicksync::vpl::VplError) -> Self {
        match err {
            crate::quicksync::vpl::VplError::Status { function, status } => {
                Self::VplStatus { function, status }
            }
            err => Self::Vpl(err.to_string()),
        }
    }
}

pub(super) struct H264Session {
    pub(super) session: Session,
    display: VaDisplay,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct H264Support {
    pub decoding: bool,
    pub encoding: bool,
}

impl H264Session {
    pub(super) fn new(
        adapter_info: &wgpu::AdapterInfo,
        component: Component,
    ) -> Result<Self, H264SessionError> {
        let render_node = quicksync_drm_render_node(adapter_info)
            .ok_or(H264SessionError::NoRenderNode)?;
        Self::for_drm_node(&render_node, component)
    }

    fn for_drm_node(
        drm_node: &DrmRenderNode,
        component: Component,
    ) -> Result<Self, H264SessionError> {
        let display = VaDisplay::open(&drm_node.path)?;
        let session =
            Session::new(drm_node.render_node, Codec::H264, component, display.handle())?;
        Ok(Self { session, display })
    }

    pub(super) fn display(&self) -> &VaDisplay {
        &self.display
    }

    pub(super) fn import_bgr4_surface(
        &self,
        device: &wgpu::Device,
        surface: &FrameSurface,
        usage: wgpu::TextureUsages,
        initial_state: wgpu::TextureUses,
    ) -> Result<ImportedRgbaSurface, H264SessionError> {
        let exported = self.session.export_va_surface(surface)?;
        let dma_buf =
            self.display.export_single_plane_surface(exported.va_surface_id())?;
        let sync_fd = dma_buf
            .fd
            .as_fd()
            .try_clone_to_owned()
            .map_err(|err| H264SessionError::DmaBuf(err.to_string()))?;
        let texture = import_bgr4_dma_buf_texture(device, dma_buf, usage, initial_state)
            .map_err(H264SessionError::DmaBuf)?;
        Ok(ImportedRgbaSurface {
            frame: Arc::new(RgbaDmaBufFrame {
                texture,
                sync: DmaBufSyncFd::new(sync_fd),
            }),
            _exported: exported,
        })
    }

    pub(super) fn import_nv12_surface(
        &self,
        interop: &DmaBufInterop,
        surface: &FrameSurface,
    ) -> Result<ImportedNv12Surface, H264SessionError> {
        let exported = self.session.export_va_surface(surface)?;
        let descriptor = self.display.export_surface(exported.va_surface_id())?;
        let frame = interop.import_nv12_texture(descriptor.nv12)?;
        Ok(ImportedNv12Surface { frame, _exported: exported })
    }

    pub(super) fn sync_status(
        &self,
        syncp: vpl::mfxSyncPoint,
        wait: SyncWait,
    ) -> Result<SyncStatus, H264SessionError> {
        Ok(self.session.sync_status(syncp, wait)?)
    }
}

pub(super) struct ImportedRgbaSurface {
    pub(super) frame: Arc<RgbaDmaBufFrame>,
    _exported: ExportedSurface,
}

pub(super) struct ImportedNv12Surface {
    pub(super) frame: Arc<DmaBufFrame>,
    _exported: ExportedSurface,
}

pub(super) struct RgbaDmaBufFrame {
    texture: wgpu::Texture,
    sync: DmaBufSyncFd,
}

impl RgbaDmaBufFrame {
    pub(super) fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }

    pub(super) fn sync(&self) -> &DmaBufSyncFd {
        &self.sync
    }
}

fn import_bgr4_dma_buf_texture(
    device: &wgpu::Device,
    dma_buf: super::va::DrmPrimeSinglePlaneSurface,
    usage: wgpu::TextureUsages,
    initial_state: wgpu::TextureUses,
) -> Result<wgpu::Texture, String> {
    if dma_buf.fourcc.to_le_bytes() != *b"ABGR" {
        return Err(format!(
            "expected BGR4 VA surface to export as ABGR DRM fourcc, got {:?}",
            dma_buf.fourcc.to_le_bytes()
        ));
    }
    let texture_format = wgpu::TextureFormat::Rgba8Unorm;
    let size = wgpu::Extent3d {
        width: dma_buf.width,
        height: dma_buf.height,
        depth_or_array_layers: 1,
    };
    let label = "Intel Quick Sync BGR4 DMA-BUF import";
    let hal_texture = unsafe {
        let hal_device = device.as_hal::<VkApi>().ok_or_else(|| {
            "BGR4 DMA-BUF import requires a Vulkan wgpu device".to_string()
        })?;
        (*hal_device)
            .texture_from_dmabuf_fd(
                dma_buf.fd,
                &wgpu::hal::TextureDescriptor {
                    label: Some(label),
                    size,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: texture_format,
                    usage: texture_uses(usage),
                    memory_flags: wgpu::hal::MemoryFlags::empty(),
                    view_formats: Vec::new(),
                },
                dma_buf.modifier,
                u64::from(dma_buf.pitch),
                u64::from(dma_buf.offset),
            )
            .map_err(|err| {
                format!("failed to import BGR4 DMA-BUF into wgpu-hal: {err}")
            })?
    };
    Ok(unsafe {
        device.create_texture_from_hal::<VkApi>(
            hal_texture,
            &wgpu::TextureDescriptor {
                label: Some(label),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: texture_format,
                usage,
                view_formats: &[],
            },
            initial_state,
        )
    })
}

fn texture_uses(usage: wgpu::TextureUsages) -> wgpu::TextureUses {
    let mut uses = wgpu::TextureUses::empty();
    if usage.contains(wgpu::TextureUsages::TEXTURE_BINDING) {
        uses |= wgpu::TextureUses::RESOURCE;
    }
    if usage.contains(wgpu::TextureUsages::COPY_SRC) {
        uses |= wgpu::TextureUses::COPY_SRC;
    }
    if usage.contains(wgpu::TextureUsages::COPY_DST) {
        uses |= wgpu::TextureUses::COPY_DST;
    }
    if usage.contains(wgpu::TextureUsages::RENDER_ATTACHMENT) {
        uses |= wgpu::TextureUses::COLOR_TARGET;
    }
    uses
}

pub(super) struct VplSyncQueue<T> {
    pending: VecDeque<VplSync<T>>,
    capacity: usize,
}

impl<T> VplSyncQueue<T> {
    pub(super) fn new(capacity: usize) -> Self {
        Self { pending: VecDeque::new(), capacity }
    }

    pub(super) fn push(&mut self, syncp: vpl::mfxSyncPoint, payload: T) {
        self.pending.push_back(VplSync { syncp, payload });
    }

    pub(super) fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub(super) fn len(&self) -> usize {
        self.pending.len()
    }

    pub(super) fn capacity(&self) -> usize {
        self.capacity
    }

    pub(super) fn is_full(&self) -> bool {
        self.pending.len() >= self.capacity
    }

    pub(super) fn clear(&mut self) {
        self.pending.clear();
    }

    pub(super) fn drain_completed(
        &mut self,
        session: &H264Session,
        mut wait: SyncWait,
    ) -> Result<Vec<T>, String> {
        let mut completed = Vec::new();
        while let Some(pending) = self.pending.pop_front() {
            match session
                .sync_status(pending.syncp, wait)
                .map_err(|err| err.to_string())?
            {
                SyncStatus::Pending => {
                    self.pending.push_front(pending);
                    break;
                }
                SyncStatus::Complete => {
                    completed.push(pending.payload);
                    wait = SyncWait::Poll;
                }
            }
        }
        Ok(completed)
    }

    pub(super) fn drain_all_completed(
        &mut self,
        session: &H264Session,
    ) -> Result<Vec<T>, String> {
        let mut completed = Vec::new();
        while !self.is_empty() {
            completed.extend(self.drain_completed(session, SyncWait::Block)?);
        }
        Ok(completed)
    }
}

struct VplSync<T> {
    syncp: vpl::mfxSyncPoint,
    payload: T,
}

pub fn support(adapter_info: &wgpu::AdapterInfo) -> H264Support {
    let render_node = quicksync_drm_render_node(adapter_info);
    H264Support {
        decoding: render_node.as_ref().is_some_and(|node| {
            H264Session::for_drm_node(node, Component::Decode).is_ok()
        }),
        encoding: render_node.as_ref().is_some_and(|node| {
            H264Session::for_drm_node(node, Component::Encode).is_ok()
        }),
    }
}
