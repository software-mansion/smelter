mod decoder;
mod encoder;

use std::{
    collections::VecDeque,
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
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
    VideoResolution,
    dmabuf::{DmaBufInterop, DmaBufSyncFd, QuickSyncDmaBufSync},
    quicksync::sys as vpl,
    quicksync::{
        display::{DrmRenderNode, quicksync_drm_render_node},
        va::{ExternalNv12DmaBuf, VaDisplay, VaError, VaSurface},
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

    pub(super) fn import_bgr4_surface(
        &self,
        device: &wgpu::Device,
        surface: &FrameSurface,
        usage: wgpu::TextureUsages,
        initial_state: wgpu::TextureUses,
    ) -> Result<ImportedRgbaSurface, H264SessionError> {
        const LABEL: &str = "Intel Quick Sync BGR4 decoder DMA-BUF import";
        let exported = self.session.export_va_surface(surface)?;
        let dma_buf = self
            .display
            .export_single_plane_surface(exported.va_surface_id())?;
        if dma_buf.fourcc.to_le_bytes() != *b"ABGR" {
            return Err(H264SessionError::DmaBuf(format!(
                "expected BGR4 VA surface to export as ABGR DRM fourcc, got {:?}",
                dma_buf.fourcc.to_le_bytes()
            )));
        }
        let texture = import_dma_buf_texture(
            device,
            dma_buf.fd,
            DmaBufTextureLayout {
                label: LABEL,
                format: wgpu::TextureFormat::Rgba8Unorm,
                width: dma_buf.width,
                height: dma_buf.height,
                modifier: dma_buf.modifier,
                pitch: dma_buf.pitch,
                offset: dma_buf.offset,
            },
            usage,
            initial_state,
        )
        .map_err(H264SessionError::DmaBuf)?;
        Ok(ImportedRgbaSurface {
            frame: Arc::new(RgbaDmaBufFrame { texture }),
            _exported: exported,
        })
    }

    /// Allocates one encoder input surface: a native Vulkan NV12 dma-buf image
    /// imported zero-copy into VA and oneVPL. GPU writes must land in a
    /// Vulkan-allocated image — writes into a foreign imported image run on a
    /// degraded path on Intel — so the interop direction here is
    /// Vulkan-allocates, VA/VPL-import, never the reverse.
    pub(super) fn allocate_nv12_input(
        &self,
        device: &wgpu::Device,
        coded: VideoResolution,
    ) -> Result<EncodeInputSurface, H264SessionError> {
        let exported =
            allocate_exportable_nv12(device, coded).map_err(H264SessionError::DmaBuf)?;
        let va_surface = self.display.import_nv12_surface(ExternalNv12DmaBuf {
            fd: exported.fd.as_raw_fd(),
            size: exported.size,
            modifier: exported.modifier,
            width: coded.width,
            height: coded.height,
            y_offset: exported.y_offset,
            y_pitch: exported.y_pitch,
            uv_offset: exported.uv_offset,
            uv_pitch: exported.uv_pitch,
        })?;
        let surface = self.session.import_va_surface(
            self.display.handle(),
            va_surface.id(),
            Component::Encode,
        )?;
        tracing::info!(
            modifier = format!("{:#x}", exported.modifier),
            y_pitch = exported.y_pitch,
            size = exported.size,
            "Allocated Quick Sync NV12 encode surface"
        );
        Ok(EncodeInputSurface {
            surface,
            _va_surface: va_surface,
            frame: Arc::new(Nv12DmaBufFrame {
                texture: exported.texture,
                sync: DmaBufSyncFd::new(exported.fd),
            }),
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

pub(super) struct RgbaDmaBufFrame {
    texture: wgpu::Texture,
}

impl RgbaDmaBufFrame {
    pub(super) fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }
}

/// One pooled encoder input. Declaration order is drop order: release the
/// oneVPL surface, destroy the VA surface, then free the Vulkan image the
/// texture wraps.
pub(super) struct EncodeInputSurface {
    pub(super) surface: FrameSurface,
    _va_surface: VaSurface,
    pub(super) frame: Arc<Nv12DmaBufFrame>,
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

struct DmaBufTextureLayout {
    label: &'static str,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
    modifier: u64,
    pitch: u32,
    offset: u32,
}

fn import_dma_buf_texture(
    device: &wgpu::Device,
    fd: std::os::fd::OwnedFd,
    layout: DmaBufTextureLayout,
    usage: wgpu::TextureUsages,
    initial_state: wgpu::TextureUses,
) -> Result<wgpu::Texture, String> {
    let size = wgpu::Extent3d {
        width: layout.width,
        height: layout.height,
        depth_or_array_layers: 1,
    };
    let hal_texture = unsafe {
        let hal_device = device
            .as_hal::<VkApi>()
            .ok_or_else(|| format!("{} requires a Vulkan wgpu device", layout.label))?;
        (*hal_device)
            .texture_from_dmabuf_fd(
                fd,
                &wgpu::hal::TextureDescriptor {
                    label: Some(layout.label),
                    size,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: layout.format,
                    usage: texture_uses(usage),
                    memory_flags: wgpu::hal::MemoryFlags::empty(),
                    view_formats: Vec::new(),
                },
                layout.modifier,
                u64::from(layout.pitch),
                u64::from(layout.offset),
            )
            .map_err(|err| format!("{}: DMA-BUF import failed: {err}", layout.label))?
    };
    Ok(unsafe {
        device.create_texture_from_hal::<VkApi>(
            hal_texture,
            &wgpu::TextureDescriptor {
                label: Some(layout.label),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: layout.format,
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

const VK_NV12_FORMAT: vk::Format = vk::Format::G8_B8R8_2PLANE_420_UNORM;
const DRM_FORMAT_MOD_LINEAR: u64 = 0;

struct ExportedNv12Texture {
    texture: wgpu::Texture,
    fd: OwnedFd,
    size: u32,
    modifier: u64,
    y_offset: u32,
    y_pitch: u32,
    uv_offset: u32,
    uv_pitch: u32,
}

struct RawNv12Export {
    image: vk::Image,
    memory: vk::DeviceMemory,
    fd: OwnedFd,
    size: u32,
    modifier: u64,
    y_offset: u32,
    y_pitch: u32,
    uv_offset: u32,
    uv_pitch: u32,
}

/// Allocates an exportable NV12 Vulkan image with a driver-chosen DRM modifier
/// (tiled preferred: LINEAR starves the media engine) and wraps it as a wgpu
/// `COPY_DST` texture that owns the image and its memory.
fn allocate_exportable_nv12(
    device: &wgpu::Device,
    coded: VideoResolution,
) -> Result<ExportedNv12Texture, String> {
    const LABEL: &str = "Intel Quick Sync NV12 encode surface";
    let (raw, vk_device) = unsafe {
        let hal_device_guard = device
            .as_hal::<VkApi>()
            .ok_or_else(|| format!("{LABEL} requires a Vulkan wgpu device"))?;
        let hal_device = &*hal_device_guard;
        let instance = hal_device.shared_instance().raw_instance();
        let vk_device = hal_device.raw_device().clone();
        let physical_device = hal_device.raw_physical_device();

        let modifiers = nv12_two_plane_modifiers(instance, physical_device);
        if modifiers.is_empty() {
            return Err(format!("{LABEL}: device advertises no 2-plane NV12 DRM modifier"));
        }
        let tiled: Vec<u64> = modifiers
            .iter()
            .copied()
            .filter(|&modifier| modifier != DRM_FORMAT_MOD_LINEAR)
            .collect();
        let raw = match (!tiled.is_empty())
            .then(|| create_exportable_nv12(instance, &vk_device, physical_device, coded, &tiled))
        {
            Some(Ok(raw)) => raw,
            _ => create_exportable_nv12(instance, &vk_device, physical_device, coded, &modifiers)?,
        };
        (raw, vk_device)
    };

    let size = coded.extent_2d();
    let (image, memory) = (raw.image, raw.memory);
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
                usage: wgpu::TextureUses::COPY_DST,
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
    let texture = unsafe {
        device.create_texture_from_hal::<VkApi>(
            hal_texture,
            &wgpu::TextureDescriptor {
                label: Some(LABEL),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::NV12,
                usage: wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::TextureUses::UNINITIALIZED,
        )
    };
    Ok(ExportedNv12Texture {
        texture,
        fd: raw.fd,
        size: raw.size,
        modifier: raw.modifier,
        y_offset: raw.y_offset,
        y_pitch: raw.y_pitch,
        uv_offset: raw.uv_offset,
        uv_pitch: raw.uv_pitch,
    })
}

unsafe fn create_exportable_nv12(
    instance: &ash::Instance,
    device: &ash::Device,
    physical_device: vk::PhysicalDevice,
    coded: VideoResolution,
    modifiers: &[u64],
) -> Result<RawNv12Export, String> {
    let mut external_info = vk::ExternalMemoryImageCreateInfo::default()
        .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
    let mut modifier_list =
        vk::ImageDrmFormatModifierListCreateInfoEXT::default().drm_format_modifiers(modifiers);
    let create_info = vk::ImageCreateInfo::default()
        .image_type(vk::ImageType::TYPE_2D)
        .format(VK_NV12_FORMAT)
        .extent(vk::Extent3D { width: coded.width, height: coded.height, depth: 1 })
        .mip_levels(1)
        .array_layers(1)
        .samples(vk::SampleCountFlags::TYPE_1)
        .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
        .usage(vk::ImageUsageFlags::TRANSFER_DST)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .push_next(&mut external_info)
        .push_next(&mut modifier_list);
    let image = unsafe { device.create_image(&create_info, None) }
        .map_err(|err| format!("failed to create exportable NV12 image: {err}"))?;

    match unsafe { export_nv12_image(instance, device, physical_device, image) } {
        Ok(raw) => Ok(raw),
        Err(err) => {
            unsafe { device.destroy_image(image, None) };
            Err(err)
        }
    }
}

unsafe fn export_nv12_image(
    instance: &ash::Instance,
    device: &ash::Device,
    physical_device: vk::PhysicalDevice,
    image: vk::Image,
) -> Result<RawNv12Export, String> {
    let requirements = unsafe { device.get_image_memory_requirements(image) };
    let memory_properties =
        unsafe { instance.get_physical_device_memory_properties(physical_device) };
    let memory_type_index =
        find_device_local_memory_type(&memory_properties, requirements.memory_type_bits)
            .ok_or("no memory type for exportable NV12 image")?;

    let mut export_info = vk::ExportMemoryAllocateInfo::default()
        .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
    let mut dedicated_info = vk::MemoryDedicatedAllocateInfo::default().image(image);
    let allocate_info = vk::MemoryAllocateInfo::default()
        .allocation_size(requirements.size)
        .memory_type_index(memory_type_index)
        .push_next(&mut export_info)
        .push_next(&mut dedicated_info);
    let memory = unsafe { device.allocate_memory(&allocate_info, None) }
        .map_err(|err| format!("failed to allocate exportable NV12 memory: {err}"))?;

    let result = (|| {
        unsafe { device.bind_image_memory(image, memory, 0) }
            .map_err(|err| format!("failed to bind exportable NV12 memory: {err}"))?;

        let modifier_ext = ash::ext::image_drm_format_modifier::Device::new(instance, device);
        let mut modifier_props = vk::ImageDrmFormatModifierPropertiesEXT::default();
        unsafe { modifier_ext.get_image_drm_format_modifier_properties(image, &mut modifier_props) }
            .map_err(|err| format!("failed to query chosen NV12 DRM modifier: {err}"))?;

        let plane_layout = |aspect| unsafe {
            device.get_image_subresource_layout(
                image,
                vk::ImageSubresource { aspect_mask: aspect, mip_level: 0, array_layer: 0 },
            )
        };
        let y = plane_layout(vk::ImageAspectFlags::MEMORY_PLANE_0_EXT);
        let uv = plane_layout(vk::ImageAspectFlags::MEMORY_PLANE_1_EXT);

        let external_memory_fd = ash::khr::external_memory_fd::Device::new(instance, device);
        let fd = unsafe {
            external_memory_fd.get_memory_fd(
                &vk::MemoryGetFdInfoKHR::default()
                    .memory(memory)
                    .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT),
            )
        }
        .map_err(|err| format!("failed to export NV12 dma-buf fd: {err}"))?;

        Ok(RawNv12Export {
            image,
            memory,
            fd: unsafe { OwnedFd::from_raw_fd(fd) },
            size: requirements.size as u32,
            modifier: modifier_props.drm_format_modifier,
            y_offset: y.offset as u32,
            y_pitch: y.row_pitch as u32,
            uv_offset: uv.offset as u32,
            uv_pitch: uv.row_pitch as u32,
        })
    })();
    if result.is_err() {
        unsafe { device.free_memory(memory, None) };
    }
    result
}

fn nv12_two_plane_modifiers(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
) -> Vec<u64> {
    unsafe {
        let mut count = vk::DrmFormatModifierPropertiesList2EXT::default();
        let mut properties = vk::FormatProperties2::default().push_next(&mut count);
        instance.get_physical_device_format_properties2(
            physical_device,
            VK_NV12_FORMAT,
            &mut properties,
        );

        let mut modifiers = vec![
            vk::DrmFormatModifierProperties2EXT::default();
            count.drm_format_modifier_count as usize
        ];
        let mut list = vk::DrmFormatModifierPropertiesList2EXT::default()
            .drm_format_modifier_properties(&mut modifiers);
        let mut properties = vk::FormatProperties2::default().push_next(&mut list);
        instance.get_physical_device_format_properties2(
            physical_device,
            VK_NV12_FORMAT,
            &mut properties,
        );

        modifiers
            .into_iter()
            .filter(|properties| properties.drm_format_modifier_plane_count == 2)
            .map(|properties| properties.drm_format_modifier)
            .collect()
    }
}

fn find_device_local_memory_type(
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    memory_type_bits: u32,
) -> Option<u32> {
    let allowed = |index: &u32| memory_type_bits & (1 << index) != 0;
    (0..memory_properties.memory_type_count)
        .filter(allowed)
        .find(|&index| {
            memory_properties.memory_types[index as usize]
                .property_flags
                .contains(vk::MemoryPropertyFlags::DEVICE_LOCAL)
        })
        .or_else(|| (0..memory_properties.memory_type_count).find(allowed))
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
