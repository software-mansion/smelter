mod decoder;
mod encoder;

use std::{collections::VecDeque, os::fd::AsFd, sync::Arc, time::Duration};

pub use decoder::{QuickSyncH264DecoderError, WgpuTexturesDecoderH264};
pub use encoder::{
    H264EncodedOutputChunk, H264EncoderConfig, H264EncoderPreset, H264EncoderRateControl,
    H264RateControlError, H264VariableBitrate, QuickSyncH264EncoderError,
    WgpuTexturesEncoderH264,
};

use crate::{
    dmabuf::{DmaBufInterop, DmaBufObject, DmaBufSyncTarget, QuickSyncDmaBufSync},
    quicksync::sys as vpl,
    quicksync::{
        display::{DrmRenderNode, quicksync_drm_render_nodes},
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

fn nv12_progressive_frame_info() -> vpl::mfxFrameInfo {
    let mut frame_info = unsafe { std::mem::zeroed::<vpl::mfxFrameInfo>() };
    frame_info.FourCC = vpl::MFX_FOURCC_NV12;
    frame_info.ChromaFormat = vpl::MFX_CHROMAFORMAT_YUV420 as u16;
    frame_info.PicStruct = vpl::MFX_PICSTRUCT_PROGRESSIVE as u16;
    frame_info
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
        let session =
            Session::new(drm_node.render_node, Codec::H264, component, display.handle())?;
        Ok(Self { session, display })
    }

    pub(super) fn import_rgb4_surface(
        &self,
        device: &wgpu::Device,
        surface: &FrameSurface,
        usage: wgpu::TextureUsages,
        initial_state: wgpu::TextureUses,
    ) -> Result<ImportedRgbaSurface, H264SessionError> {
        self.import_rgba_surface(
            device,
            surface,
            usage,
            initial_state,
            RgbaDmaBufFormat::Rgb4,
        )
    }

    pub(super) fn import_bgr4_surface(
        &self,
        device: &wgpu::Device,
        surface: &FrameSurface,
        usage: wgpu::TextureUsages,
        initial_state: wgpu::TextureUses,
    ) -> Result<ImportedRgbaSurface, H264SessionError> {
        self.import_rgba_surface(
            device,
            surface,
            usage,
            initial_state,
            RgbaDmaBufFormat::Bgr4,
        )
    }

    fn import_rgba_surface(
        &self,
        device: &wgpu::Device,
        surface: &FrameSurface,
        usage: wgpu::TextureUsages,
        initial_state: wgpu::TextureUses,
        format: RgbaDmaBufFormat,
    ) -> Result<ImportedRgbaSurface, H264SessionError> {
        let exported = self.session.export_va_surface(surface)?;
        let dma_buf =
            self.display.export_single_plane_surface(exported.va_surface_id())?;
        let sync_fd = dma_buf
            .fd
            .as_fd()
            .try_clone_to_owned()
            .map_err(|err| H264SessionError::DmaBuf(err.to_string()))?;
        let object = DmaBufObject {
            fd: Arc::new(sync_fd),
        };
        let texture =
            import_rgba_dma_buf_texture(device, dma_buf, usage, initial_state, format)
                .map_err(H264SessionError::DmaBuf)?;
        Ok(ImportedRgbaSurface {
            frame: Arc::new(RgbaDmaBufFrame {
                texture: Arc::new(texture),
                objects: Box::new([object]),
                sync_lock: Arc::new(std::sync::Mutex::new(())),
            }),
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

pub(super) struct ImportedRgbaSurface {
    pub(super) frame: Arc<RgbaDmaBufFrame>,
    _exported: ExportedSurface,
}

#[derive(Clone)]
pub(super) struct RgbaDmaBufFrame {
    texture: Arc<wgpu::Texture>,
    objects: Box<[DmaBufObject]>,
    sync_lock: Arc<std::sync::Mutex<()>>,
}

impl RgbaDmaBufFrame {
    pub(super) fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }
}

impl DmaBufSyncTarget for RgbaDmaBufFrame {
    fn objects(&self) -> &[DmaBufObject] {
        &self.objects
    }

    fn sync_guard(&self) -> std::sync::MutexGuard<'_, ()> {
        self.sync_lock.lock().expect("RGBA DMA-BUF sync lock poisoned")
    }
}

#[derive(Debug, Clone, Copy)]
enum RgbaDmaBufFormat {
    Rgb4,
    Bgr4,
}

impl RgbaDmaBufFormat {
    fn label(self) -> &'static str {
        match self {
            Self::Rgb4 => "Intel Quick Sync RGB4 encode DMA-BUF import",
            Self::Bgr4 => "Intel Quick Sync BGR4 decoder DMA-BUF import",
        }
    }

    fn drm_fourcc(self) -> &'static [u8; 4] {
        match self {
            Self::Rgb4 => b"ARGB",
            Self::Bgr4 => b"ABGR",
        }
    }

    fn texture_format(self) -> wgpu::TextureFormat {
        match self {
            Self::Rgb4 => wgpu::TextureFormat::Bgra8Unorm,
            Self::Bgr4 => wgpu::TextureFormat::Rgba8Unorm,
        }
    }

    fn vpl_name(self) -> &'static str {
        match self {
            Self::Rgb4 => "RGB4",
            Self::Bgr4 => "BGR4",
        }
    }
}

fn import_rgba_dma_buf_texture(
    device: &wgpu::Device,
    dma_buf: super::va::DrmPrimeSinglePlaneSurface,
    usage: wgpu::TextureUsages,
    initial_state: wgpu::TextureUses,
    format: RgbaDmaBufFormat,
) -> Result<wgpu::Texture, String> {
    if dma_buf.fourcc.to_le_bytes() != *format.drm_fourcc() {
        return Err(format!(
            "expected {} VA surface to export as {} DRM fourcc, got {:?}",
            format.vpl_name(),
            std::str::from_utf8(format.drm_fourcc()).expect("DRM fourcc must be ASCII"),
            dma_buf.fourcc.to_le_bytes()
        ));
    }
    let texture_format = format.texture_format();
    let size = wgpu::Extent3d {
        width: dma_buf.width,
        height: dma_buf.height,
        depth_or_array_layers: 1,
    };
    let label = format.label();
    let hal_texture = unsafe {
        let hal_device = device.as_hal::<VkApi>().ok_or_else(|| {
            format!("{} requires a Vulkan wgpu device", format.vpl_name())
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
                format!("failed to import {} DMA-BUF into wgpu-hal: {err}", format.vpl_name())
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

pub(super) struct TextureSwizzleRenderer {
    label: &'static str,
    device: wgpu::Device,
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
}

impl TextureSwizzleRenderer {
    pub(super) fn new(
        device: &wgpu::Device,
        label: &'static str,
        output_format: wgpu::TextureFormat,
    ) -> Self {
        let shader =
            device.create_shader_module(wgpu::include_wgsl!("../shaders/rgba_bgra.wgsl"));
        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some(label),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                }],
            });
        let pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: &[Some(&bind_group_layout)],
                immediate_size: 0,
            });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(&pipeline_layout),
            cache: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(output_format.into())],
            }),
            primitive: wgpu::PrimitiveState::default(),
            multiview_mask: None,
            multisample: wgpu::MultisampleState::default(),
            depth_stencil: None,
        });
        Self { label, device: device.clone(), pipeline, bind_group_layout }
    }

    pub(super) fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        input: &wgpu::Texture,
        output: &wgpu::Texture,
    ) {
        let input_view = input.create_view(&Default::default());
        let output_view = output.create_view(&Default::default());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(self.label),
            layout: &self.bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&input_view),
            }],
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(self.label),
            timestamp_writes: None,
            occlusion_query_set: None,
            depth_stencil_attachment: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &output_view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
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
            completed.extend(self.drain_completed(
                session,
                SyncWait::Block,
                &mut complete,
            )?);
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
        decoding: H264Session::from_render_nodes(render_nodes.iter(), Component::Decode)
            .is_ok(),
        encoding: H264Session::from_render_nodes(render_nodes.iter(), Component::Encode)
            .is_ok(),
    }
}
