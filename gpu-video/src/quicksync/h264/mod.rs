mod decoder;
mod encoder;

use std::{
    collections::VecDeque,
    sync::{Arc, OnceLock},
    time::Duration,
};

pub use decoder::{QuickSyncH264DecoderError, WgpuTexturesDecoderH264};
pub use encoder::{
    H264EncodedOutputChunk, H264EncoderConfig, H264EncoderPreset, H264EncoderRateControl,
    H264RateControlError, H264VariableBitrate, QuickSyncH264EncoderError, WgpuTexturesEncoderH264,
};

use crate::{
    dmabuf::{DmaBufFrame, DmaBufInterop, QuickSyncDmaBufSync},
    quicksync::sys as vpl,
    quicksync::{
        display::{DrmRenderNode, quicksync_drm_render_nodes},
        va::{VaDisplay, VaError},
        vpl::{Codec, Component, ExportedSurface, FrameSurface, Session, SyncStatus, SyncWait},
    },
};

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

fn nv12_progressive_frame_info() -> vpl::mfxFrameInfo {
    let mut frame_info = unsafe { std::mem::zeroed::<vpl::mfxFrameInfo>() };
    frame_info.FourCC = vpl::MFX_FOURCC_NV12;
    frame_info.ChromaFormat = vpl::MFX_CHROMAFORMAT_YUV420 as u16;
    frame_info.PicStruct = vpl::MFX_PICSTRUCT_PROGRESSIVE as u16;
    frame_info
}

fn init_dmabuf_interop(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> Result<(DmaBufInterop, QuickSyncDmaBufSync), H264SessionError> {
    static PROBE_RESULT: OnceLock<Result<(), String>> = OnceLock::new();

    let interop = DmaBufInterop::new(device)?;
    let sync = QuickSyncDmaBufSync::new(&interop, queue);
    PROBE_RESULT
        .get_or_init(|| {
            super::probe::probe_nv12_dmabuf_wgpu_roundtrip(device, &interop, &sync, queue)
        })
        .clone()
        .map_err(H264SessionError::Probe)?;
    Ok((interop, sync))
}

#[derive(Debug, thiserror::Error)]
pub enum H264SessionError {
    #[error("no Intel Quick Sync DRM render node found")]
    NoRenderNode,

    #[error("DMA-BUF interop failed: {0}")]
    DmaBuf(String),

    #[error("{0}")]
    Probe(String),

    #[error("{function} failed with VA status {status}")]
    VaStatus {
        function: &'static str,
        status: i32,
    },

    #[error("VA interop failed: {0}")]
    Va(String),

    #[error("{function} failed with oneVPL status {status}")]
    VplStatus {
        function: &'static str,
        status: i32,
    },

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
        let render_nodes = quicksync_drm_render_nodes(adapter_info);
        Self::from_render_nodes(render_nodes.iter(), component)
    }

    fn from_render_nodes<'a>(
        render_nodes: impl IntoIterator<Item = &'a DrmRenderNode>,
        component: Component,
    ) -> Result<Self, H264SessionError> {
        let mut error = H264SessionError::NoRenderNode;
        for drm_node in render_nodes {
            match Self::for_drm_node(drm_node, component) {
                Ok(session) => return Ok(session),
                Err(err) => error = err,
            }
        }
        Err(error)
    }

    fn for_drm_node(
        drm_node: &DrmRenderNode,
        component: Component,
    ) -> Result<Self, H264SessionError> {
        let display = VaDisplay::open(&drm_node.path)?;
        let session = Session::new(
            drm_node.render_node,
            Codec::H264,
            component,
            display.handle(),
        )?;
        Ok(Self { session, display })
    }

    pub(super) fn import_surface(
        &self,
        interop: &DmaBufInterop,
        surface: &FrameSurface,
    ) -> Result<ImportedSurface, H264SessionError> {
        let exported = self.session.export_va_surface(surface)?;
        let descriptor = self.display.export_surface(exported.va_surface_id())?;
        let frame = interop.import_nv12_texture(descriptor.nv12)?;
        Ok(ImportedSurface {
            frame,
            _exported: exported,
        })
    }

    pub(super) fn sync_status(
        &self,
        syncp: vpl::mfxSyncPoint,
        wait: SyncWait,
    ) -> Result<SyncStatus, H264SessionError> {
        Ok(self.session.sync_status(syncp, wait)?)
    }
}

pub(super) struct ImportedSurface {
    pub(super) frame: Arc<DmaBufFrame>,
    _exported: ExportedSurface,
}

pub(super) struct VplSyncQueue<T> {
    pending: VecDeque<VplSync<T>>,
    capacity: usize,
}

impl<T> VplSyncQueue<T> {
    pub(super) fn new(capacity: usize) -> Self {
        Self {
            pending: VecDeque::new(),
            capacity,
        }
    }

    pub(super) fn push(&mut self, syncp: vpl::mfxSyncPoint, payload: T) {
        self.pending.push_back(VplSync { syncp, payload });
    }

    pub(super) fn len(&self) -> usize {
        self.pending.len()
    }

    pub(super) fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub(super) fn is_full(&self) -> bool {
        self.len() >= self.capacity
    }

    pub(super) fn clear(&mut self) {
        self.pending.clear();
    }

    pub(super) fn drain_completed<R>(
        &mut self,
        session: &H264Session,
        mut wait: SyncWait,
        mut complete: impl FnMut(T) -> Result<R, String>,
    ) -> Result<Vec<R>, String> {
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
                    completed.push(complete(pending.payload)?);
                    wait = SyncWait::Poll;
                }
            }
        }
        Ok(completed)
    }

    pub(super) fn drain_all_completed<R>(
        &mut self,
        session: &H264Session,
        mut complete: impl FnMut(T) -> Result<R, String>,
    ) -> Result<Vec<R>, String> {
        let mut completed = Vec::new();
        while !self.is_empty() {
            completed.extend(self.drain_completed(session, SyncWait::Block, &mut complete)?);
        }
        Ok(completed)
    }
}

struct VplSync<T> {
    syncp: vpl::mfxSyncPoint,
    payload: T,
}

pub fn support(adapter_info: &wgpu::AdapterInfo) -> H264Support {
    let render_nodes = quicksync_drm_render_nodes(adapter_info);
    H264Support {
        decoding: H264Session::from_render_nodes(render_nodes.iter(), Component::Decode).is_ok(),
        encoding: H264Session::from_render_nodes(render_nodes.iter(), Component::Encode).is_ok(),
    }
}
