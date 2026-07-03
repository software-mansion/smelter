mod decoder;
mod encoder;

use std::{
    collections::VecDeque,
    os::fd::{AsFd, AsRawFd, IntoRawFd, OwnedFd},
    sync::Arc,
    time::Duration,
};

use ash::vk;

pub use decoder::{QuickSyncH264DecoderError, WgpuTexturesDecoderH264};
pub use encoder::{
    H264EncodedOutputChunk, H264EncoderConfig, H264EncoderPreset, H264EncoderRateControl,
    H264RateControlError, H264VariableBitrate, QuickSyncH264EncoderError, WgpuTexturesEncoderH264,
};

use crate::{
    dmabuf::{DmaBufInterop, DmaBufSyncFd, QuickSyncDmaBufSync},
    quicksync::sys as vpl,
    quicksync::{
        display::{DrmRenderNode, quicksync_drm_render_node},
        va::{DrmPrimeNv12Surface, VaDisplay, VaError},
        vpl::{Codec, Component, ExportedSurface, FrameSurface, Session, SyncStatus, SyncWait},
    },
};
use wgpu::hal::api::Vulkan as VkApi;

const DEVICE_BUSY_RETRIES: usize = 500;
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
        std::thread::sleep(Duration::from_micros(200));
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

fn vpl_u16_dimension(name: &str, value: u32) -> Result<u16, String> {
    value
        .try_into()
        .map_err(|_| format!("H264 {name} {value} exceeds oneVPL limit"))
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
        let render_node =
            quicksync_drm_render_node(adapter_info).ok_or(H264SessionError::NoRenderNode)?;
        Self::for_drm_node(&render_node, component)
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

    /// Acquires one encoder input surface: oneVPL allocates it, VA exports the
    /// backing NV12 dma-buf, and the single object is imported as one wgpu
    /// NV12 texture the caller copies into.
    pub(super) fn acquire_nv12_input(
        &self,
        device: &wgpu::Device,
    ) -> Result<EncodeInputSurface, H264SessionError> {
        let surface = self.session.get_surface_for_encode()?;
        let exported = self.session.export_va_surface(&surface)?;
        let dma_buf = self.display.export_nv12_surface(exported.va_surface_id())?;
        let sync_fd = clone_dma_buf_fd(&dma_buf.fd)?;
        let texture =
            import_nv12_dma_buf_texture(device, dma_buf, wgpu::TextureUsages::COPY_DST)
                .map_err(H264SessionError::DmaBuf)?;
        Ok(EncodeInputSurface {
            surface,
            _exported: exported,
            frame: Arc::new(Nv12DmaBufFrame {
                texture,
                sync: DmaBufSyncFd::new(sync_fd),
            }),
        })
    }

    /// Imports a decoded NV12 surface as one wgpu texture the decoder copies
    /// out of. The caller CPU-syncs the decode before reading, so no dma-buf
    /// fences are involved.
    pub(super) fn import_nv12_frame(
        &self,
        device: &wgpu::Device,
        surface: &FrameSurface,
    ) -> Result<ImportedNv12Frame, H264SessionError> {
        let exported = self.session.export_va_surface(surface)?;
        let dma_buf = self.display.export_nv12_surface(exported.va_surface_id())?;
        let texture =
            import_nv12_dma_buf_texture(device, dma_buf, wgpu::TextureUsages::COPY_SRC)
                .map_err(H264SessionError::DmaBuf)?;
        Ok(ImportedNv12Frame { texture, _exported: exported })
    }

    pub(super) fn sync_status(
        &self,
        syncp: vpl::mfxSyncPoint,
        wait: SyncWait,
    ) -> Result<SyncStatus, H264SessionError> {
        Ok(self.session.sync_status(syncp, wait)?)
    }
}

/// One pooled encoder input. Declaration order is drop order, consumers
/// before producers: free the imported Vulkan image, release the VA export
/// mapping, then release the owning oneVPL surface — releasing the surface
/// while its export mapping is alive corrupts the runtime's refcounts.
pub(super) struct EncodeInputSurface {
    pub(super) frame: Arc<Nv12DmaBufFrame>,
    _exported: ExportedSurface,
    pub(super) surface: FrameSurface,
}

/// A decoded frame's imported texture. Declaration order is drop order:
/// free the imported Vulkan image before releasing the VA export mapping.
pub(super) struct ImportedNv12Frame {
    texture: wgpu::Texture,
    _exported: ExportedSurface,
}

impl ImportedNv12Frame {
    pub(super) fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }
}

fn clone_dma_buf_fd(fd: &OwnedFd) -> Result<OwnedFd, H264SessionError> {
    fd.as_fd()
        .try_clone_to_owned()
        .map_err(|err| H264SessionError::DmaBuf(err.to_string()))
}

pub(super) struct Nv12DmaBufFrame {
    texture: wgpu::Texture,
    sync: DmaBufSyncFd,
}

impl Nv12DmaBufFrame {
    pub(super) fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }

    pub(super) fn sync(&self) -> &DmaBufSyncFd {
        &self.sync
    }
}

fn vk_transfer_usage(usage: wgpu::TextureUsages) -> vk::ImageUsageFlags {
    let mut flags = vk::ImageUsageFlags::empty();
    if usage.contains(wgpu::TextureUsages::COPY_DST) {
        flags |= vk::ImageUsageFlags::TRANSFER_DST;
    }
    if usage.contains(wgpu::TextureUsages::COPY_SRC) {
        flags |= vk::ImageUsageFlags::TRANSFER_SRC;
    }
    flags
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

/// Imports the single-object NV12 dma-buf exported from a VA surface as one
/// multi-planar wgpu texture. wgpu's `texture_from_dmabuf_fd` only handles
/// single-plane formats (https://github.com/gfx-rs/wgpu/issues/9801), so the
/// image is created and bound with raw Vulkan (explicit per-plane DRM
/// modifier layouts) and wrapped via `texture_from_raw`; the wrapper owns the
/// image and its imported memory.
fn import_nv12_dma_buf_texture(
    device: &wgpu::Device,
    dma_buf: DrmPrimeNv12Surface,
    usage: wgpu::TextureUsages,
) -> Result<wgpu::Texture, String> {
    const LABEL: &str = "Intel Quick Sync NV12 encode DMA-BUF import";
    let size = wgpu::Extent3d {
        width: dma_buf.width,
        height: dma_buf.height,
        depth_or_array_layers: 1,
    };
    let (image, memory, vk_device) = unsafe {
        let hal_device_guard = device
            .as_hal::<VkApi>()
            .ok_or_else(|| format!("{LABEL} requires a Vulkan wgpu device"))?;
        let hal_device = &*hal_device_guard;
        let instance = hal_device.shared_instance().raw_instance();
        let vk_device = hal_device.raw_device().clone();
        let physical_device = hal_device.raw_physical_device();

        let plane_layout = |offset: u32, pitch: u32| vk::SubresourceLayout {
            offset: u64::from(offset),
            row_pitch: u64::from(pitch),
            ..Default::default()
        };
        let plane_layouts = [
            plane_layout(dma_buf.y_offset, dma_buf.y_pitch),
            plane_layout(dma_buf.uv_offset, dma_buf.uv_pitch),
        ];
        let mut external_info = vk::ExternalMemoryImageCreateInfo::default()
            .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
        let mut modifier_info = vk::ImageDrmFormatModifierExplicitCreateInfoEXT::default()
            .drm_format_modifier(dma_buf.modifier)
            .plane_layouts(&plane_layouts);
        let create_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(vk::Format::G8_B8R8_2PLANE_420_UNORM)
            .extent(vk::Extent3D {
                width: dma_buf.width,
                height: dma_buf.height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
            .usage(vk_transfer_usage(usage))
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .push_next(&mut external_info)
            .push_next(&mut modifier_info);
        let image = vk_device
            .create_image(&create_info, None)
            .map_err(|err| format!("{LABEL}: failed to create image: {err}"))?;

        let import_fd = dma_buf
            .fd
            .as_fd()
            .try_clone_to_owned()
            .map_err(|err| format!("{LABEL}: failed to duplicate fd: {err}"))?;
        let imported = (|| {
            let external_memory_fd =
                ash::khr::external_memory_fd::Device::new(instance, &vk_device);
            let mut fd_properties = vk::MemoryFdPropertiesKHR::default();
            external_memory_fd
                .get_memory_fd_properties(
                    vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT,
                    import_fd.as_raw_fd(),
                    &mut fd_properties,
                )
                .map_err(|err| format!("{LABEL}: failed to query fd properties: {err}"))?;
            let requirements = vk_device.get_image_memory_requirements(image);
            let memory_properties =
                instance.get_physical_device_memory_properties(physical_device);
            let allowed = requirements.memory_type_bits & fd_properties.memory_type_bits;
            let memory_type_index = (0..memory_properties.memory_type_count)
                .find(|index| allowed & (1 << index) != 0)
                .ok_or_else(|| format!("{LABEL}: no compatible memory type"))?;

            let mut import_info = vk::ImportMemoryFdInfoKHR::default()
                .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT)
                .fd(import_fd.as_raw_fd());
            let mut dedicated_info = vk::MemoryDedicatedAllocateInfo::default().image(image);
            let allocate_info = vk::MemoryAllocateInfo::default()
                .allocation_size(requirements.size)
                .memory_type_index(memory_type_index)
                .push_next(&mut import_info)
                .push_next(&mut dedicated_info);
            let memory = vk_device
                .allocate_memory(&allocate_info, None)
                .map_err(|err| format!("{LABEL}: failed to import memory: {err}"))?;
            if let Err(err) = vk_device.bind_image_memory(image, memory, 0) {
                vk_device.free_memory(memory, None);
                return Err(format!("{LABEL}: failed to bind memory: {err}"));
            }
            Ok(memory)
        })();
        match imported {
            Ok(memory) => {
                let _ = import_fd.into_raw_fd();
                (image, memory, vk_device)
            }
            Err(err) => {
                vk_device.destroy_image(image, None);
                return Err(err);
            }
        }
    };

    let hal_texture = unsafe {
        let hal_device_guard = device
            .as_hal::<VkApi>()
            .ok_or_else(|| format!("{LABEL} requires a Vulkan wgpu device"))?;
        (*hal_device_guard).texture_from_raw(
            image,
            &wgpu::hal::TextureDescriptor {
                label: Some(LABEL),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::NV12,
                usage: texture_uses(usage),
                memory_flags: wgpu::hal::MemoryFlags::empty(),
                view_formats: Vec::new(),
            },
            Some(Box::new(move || {
                vk_device.destroy_image(image, None);
                vk_device.free_memory(memory, None);
            })),
            wgpu::hal::vulkan::TextureMemory::External,
        )
    };
    Ok(unsafe {
        device.create_texture_from_hal::<VkApi>(
            hal_texture,
            &wgpu::TextureDescriptor {
                label: Some(LABEL),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::NV12,
                usage,
                view_formats: &[],
            },
            texture_uses(usage),
        )
    })
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

    pub(super) fn is_empty(&self) -> bool {
        self.pending.is_empty()
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

    pub(super) fn drain_all_completed(&mut self, session: &H264Session) -> Result<Vec<T>, String> {
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
        decoding: render_node
            .as_ref()
            .is_some_and(|node| H264Session::for_drm_node(node, Component::Decode).is_ok()),
        encoding: render_node
            .as_ref()
            .is_some_and(|node| H264Session::for_drm_node(node, Component::Encode).is_ok()),
    }
}
