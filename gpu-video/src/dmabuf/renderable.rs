//! Allocate a RENDERABLE NV12 dma-buf the compositor can render into
//! (COLOR_ATTACHMENT on the R8/RG8 plane views) AND export it so VA can import it
//! for zero-copy encode. This backs the Quick Sync zero-copy encoder pool: the
//! compositor renders the NV12 output directly into these dma-buf textures and
//! oneVPL encodes the exact same surface with no per-frame copy.
//!
//! Strategy: create an exportable `G8_B8R8_2PLANE_420_UNORM` image with
//! `DRM_FORMAT_MODIFIER` tiling (driver-chosen from a candidate list, tiled
//! preferred), `MUTABLE_FORMAT | EXTENDED_USAGE` so the single-plane views can be
//! color attachments, export the fd via `vkGetMemoryFdKHR`, read the chosen
//! modifier + per-plane layout, and wrap it as a wgpu texture with
//! `RENDER_ATTACHMENT` usage.

use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

use ash::vk;
use wgpu::hal::api::Vulkan as VkApi;

use std::sync::Arc;

use crate::VideoResolution;

use super::{
    DmaBufError, DmaBufFrame, DmaBufInterop, DmaBufObject, DmaBufPlane,
    Nv12DmaBufDescriptor, Nv12DmaBufLayer, interop::VulkanDmaBufDevice,
};

const VK_NV12_FORMAT: vk::Format = vk::Format::G8_B8R8_2PLANE_420_UNORM;
const DRM_FORMAT_MOD_LINEAR: u64 = 0;

pub(crate) struct RenderableNv12DmaBuf {
    pub(crate) texture: wgpu::Texture,
    pub(crate) fd: OwnedFd,
    pub(crate) modifier: u64,
    pub(crate) size: u32,
    pub(crate) y_offset: u32,
    pub(crate) y_pitch: u32,
    pub(crate) uv_offset: u32,
    pub(crate) uv_pitch: u32,
}

struct RawExport {
    image: vk::Image,
    memory: vk::DeviceMemory,
    modifier: u64,
    size: u32,
    y_offset: u32,
    y_pitch: u32,
    uv_offset: u32,
    uv_pitch: u32,
    fd: OwnedFd,
}

impl RenderableNv12DmaBuf {
    /// Raw exported dma-buf fd, borrowed for the duration of a VA surface import
    /// (VA dups it internally). Ownership is retained until [`into_dmabuf_frame`].
    pub(crate) fn fd_raw(&self) -> i32 {
        self.fd.as_raw_fd()
    }

    /// Wrap the exported renderable texture as a [`DmaBufFrame`] (taking ownership
    /// of the exported fd) so the Quick Sync sync machinery can fence GPU writes
    /// to it. The fd is duplicated by VA during surface import, so callers must
    /// import into VA *before* calling this (it moves the fd into the frame).
    pub(crate) fn into_dmabuf_frame(
        self,
        coded: VideoResolution,
    ) -> Result<(Arc<wgpu::Texture>, Arc<DmaBufFrame>), DmaBufError> {
        let texture = Arc::new(self.texture);
        let object =
            DmaBufObject { fd: Arc::new(self.fd), size: self.size, modifier: self.modifier };
        let layer = Nv12DmaBufLayer {
            planes: [
                DmaBufPlane {
                    object_index: 0,
                    offset: self.y_offset,
                    pitch: self.y_pitch,
                },
                DmaBufPlane {
                    object_index: 0,
                    offset: self.uv_offset,
                    pitch: self.uv_pitch,
                },
            ],
        };
        let descriptor = Nv12DmaBufDescriptor::new(coded, Box::new([object]), layer)?;
        let frame = Arc::new(DmaBufFrame::new(Arc::clone(&texture), descriptor));
        Ok((texture, frame))
    }
}

pub(crate) fn export_renderable_nv12(
    interop: &DmaBufInterop,
    resolution: VideoResolution,
) -> Result<RenderableNv12DmaBuf, DmaBufError> {
    let vulkan = &interop.vulkan;
    let all: Vec<u64> = vulkan
        .nv12_modifiers
        .iter()
        .filter(|m| m.drm_format_modifier_plane_count == 2)
        .map(|m| m.drm_format_modifier)
        .collect();
    let tiled: Vec<u64> =
        all.iter().copied().filter(|&m| m != DRM_FORMAT_MOD_LINEAR).collect();

    // Prefer a tiled modifier; only fall back to the full list (incl. LINEAR) if
    // no tiled modifier supports COLOR_ATTACHMENT for NV12 plane views.
    let raw = match (!tiled.is_empty())
        .then(|| unsafe { try_create_exportable(vulkan, resolution, &tiled) })
    {
        Some(Ok(raw)) => raw,
        _ => unsafe { try_create_exportable(vulkan, resolution, &all)? },
    };

    let RawExport {
        image,
        memory,
        modifier,
        size,
        y_offset,
        y_pitch,
        uv_offset,
        uv_pitch,
        fd,
    } = raw;
    let texture = unsafe { wrap_renderable_nv12(interop, image, memory, resolution)? };
    Ok(RenderableNv12DmaBuf {
        texture,
        fd,
        modifier,
        size,
        y_offset,
        y_pitch,
        uv_offset,
        uv_pitch,
    })
}

unsafe fn try_create_exportable(
    vulkan: &VulkanDmaBufDevice,
    resolution: VideoResolution,
    modifiers: &[u64],
) -> Result<RawExport, DmaBufError> {
    let device = &vulkan.device;
    let usage =
        vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC;
    let flags =
        vk::ImageCreateFlags::MUTABLE_FORMAT | vk::ImageCreateFlags::EXTENDED_USAGE;

    let mut external = vk::ExternalMemoryImageCreateInfo::default()
        .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
    let mut modifier_list = vk::ImageDrmFormatModifierListCreateInfoEXT::default()
        .drm_format_modifiers(modifiers);
    let create_info = nv12_image_create_info(resolution, flags, usage)
        .push_next(&mut external)
        .push_next(&mut modifier_list);

    let image = unsafe { device.create_image(&create_info, None) }.map_err(|err| {
        DmaBufError::UnsupportedDevice(format!(
            "failed to create renderable exportable NV12 image: {err}"
        ))
    })?;

    match unsafe { finish_export(vulkan, image) } {
        Ok(raw) => Ok(raw),
        Err(err) => {
            unsafe { device.destroy_image(image, None) };
            Err(err)
        }
    }
}

unsafe fn finish_export(
    vulkan: &VulkanDmaBufDevice,
    image: vk::Image,
) -> Result<RawExport, DmaBufError> {
    let device = &vulkan.device;
    let requirements = unsafe { device.get_image_memory_requirements(image) };
    let memory_index = find_memory_type(&vulkan.memory_properties, requirements.memory_type_bits)
        .ok_or_else(|| {
            DmaBufError::UnsupportedDevice("no memory type for exportable NV12 image".into())
        })?;

    let mut export_info = vk::ExportMemoryAllocateInfo::default()
        .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
    let mut dedicated = vk::MemoryDedicatedAllocateInfo::default().image(image);
    let allocate_info = vk::MemoryAllocateInfo::default()
        .allocation_size(requirements.size)
        .memory_type_index(memory_index)
        .push_next(&mut export_info)
        .push_next(&mut dedicated);
    let memory = unsafe { device.allocate_memory(&allocate_info, None) }
        .map_err(|err| DmaBufError::Vulkan(format!("failed to allocate export memory: {err}")))?;

    let result = (|| {
        unsafe { device.bind_image_memory(image, memory, 0) }.map_err(|err| {
            DmaBufError::Vulkan(format!("failed to bind export memory: {err}"))
        })?;

        let modifier_ext =
            ash::ext::image_drm_format_modifier::Device::new(&vulkan.instance, device);
        let mut modifier_props = vk::ImageDrmFormatModifierPropertiesEXT::default();
        unsafe {
            modifier_ext
                .get_image_drm_format_modifier_properties(image, &mut modifier_props)
        }
        .map_err(|err| {
            DmaBufError::Vulkan(format!("failed to query chosen DRM modifier: {err}"))
        })?;

        let y = unsafe {
            device.get_image_subresource_layout(
                image,
                vk::ImageSubresource {
                    aspect_mask: vk::ImageAspectFlags::MEMORY_PLANE_0_EXT,
                    mip_level: 0,
                    array_layer: 0,
                },
            )
        };
        let uv = unsafe {
            device.get_image_subresource_layout(
                image,
                vk::ImageSubresource {
                    aspect_mask: vk::ImageAspectFlags::MEMORY_PLANE_1_EXT,
                    mip_level: 0,
                    array_layer: 0,
                },
            )
        };

        let fd = unsafe {
            vulkan.external_memory_fd.get_memory_fd(
                &vk::MemoryGetFdInfoKHR::default()
                    .memory(memory)
                    .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT),
            )
        }
        .map_err(|err| DmaBufError::Vulkan(format!("failed to export dma-buf fd: {err}")))?;

        Ok(RawExport {
            image,
            memory,
            modifier: modifier_props.drm_format_modifier,
            size: requirements.size as u32,
            y_offset: y.offset as u32,
            y_pitch: y.row_pitch as u32,
            uv_offset: uv.offset as u32,
            uv_pitch: uv.row_pitch as u32,
            fd: unsafe { OwnedFd::from_raw_fd(fd) },
        })
    })();

    if result.is_err() {
        unsafe { device.free_memory(memory, None) };
    }
    result
}

unsafe fn wrap_renderable_nv12(
    interop: &DmaBufInterop,
    image: vk::Image,
    memory: vk::DeviceMemory,
    resolution: VideoResolution,
) -> Result<wgpu::Texture, DmaBufError> {
    let wgpu_device = &interop.device;
    let vk_device = interop.vulkan.device.clone();
    let label = "poc renderable nv12 dma-buf texture";
    let size = resolution.extent_2d();
    let view_formats = vec![wgpu::TextureFormat::R8Unorm, wgpu::TextureFormat::Rg8Unorm];

    let hal_device_guard = unsafe {
        wgpu_device.as_hal::<VkApi>().ok_or_else(|| {
            DmaBufError::UnsupportedDevice("renderable NV12 export requires a Vulkan wgpu device".into())
        })?
    };
    let hal_texture = unsafe {
        let hal_usage = wgpu::TextureUses::COLOR_TARGET
            | wgpu::TextureUses::COPY_SRC
            | wgpu::TextureUses::RESOURCE;
        (*hal_device_guard).texture_from_raw(
            image,
            &wgpu::hal::TextureDescriptor {
                label: Some(label),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::NV12,
                usage: hal_usage,
                memory_flags: wgpu::hal::MemoryFlags::empty(),
                view_formats: view_formats.clone(),
            },
            Some(Box::new(move || {
                vk_device.destroy_image(image, None);
                vk_device.free_memory(memory, None);
            })),
            wgpu::hal::vulkan::TextureMemory::External,
        )
    };

    Ok(unsafe {
        wgpu_device.create_texture_from_hal::<VkApi>(
            hal_texture,
            &wgpu::TextureDescriptor {
                label: Some(label),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::NV12,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &view_formats,
            },
            wgpu::TextureUses::UNINITIALIZED,
        )
    })
}

fn nv12_image_create_info(
    resolution: VideoResolution,
    flags: vk::ImageCreateFlags,
    usage: vk::ImageUsageFlags,
) -> vk::ImageCreateInfo<'static> {
    vk::ImageCreateInfo::default()
        .flags(flags)
        .image_type(vk::ImageType::TYPE_2D)
        .format(VK_NV12_FORMAT)
        .extent(vk::Extent3D { width: resolution.width, height: resolution.height, depth: 1 })
        .mip_levels(1)
        .array_layers(1)
        .samples(vk::SampleCountFlags::TYPE_1)
        .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
        .usage(usage)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .initial_layout(vk::ImageLayout::UNDEFINED)
}

fn find_memory_type(
    properties: &vk::PhysicalDeviceMemoryProperties,
    bits: u32,
) -> Option<u32> {
    // Prefer DEVICE_LOCAL, fall back to any allowed type.
    (0..properties.memory_type_count)
        .find(|index| {
            bits & (1 << index) != 0
                && properties.memory_types[*index as usize]
                    .property_flags
                    .contains(vk::MemoryPropertyFlags::DEVICE_LOCAL)
        })
        .or_else(|| (0..properties.memory_type_count).find(|index| bits & (1 << index) != 0))
}

// ============================ Phase 0 feasibility ============================
// Throwaway probe (test-only): can we allocate an exportable RENDERABLE NV12
// surface on an Intel render-compression (CCS, plane_count > 2) modifier and have
// VA import the SAME compressed surface zero-copy? Logs the full modifier list and
// reads ALL memory-plane layouts (incl. aux CCS plane(s)) so the orchestrating
// test can describe them to `vaCreateSurfaces`.

#[cfg(test)]
pub(crate) struct CcsProbePlane {
    pub(crate) offset: u32,
    pub(crate) pitch: u32,
}

#[cfg(test)]
pub(crate) struct CcsProbe {
    pub(crate) modifier: u64,
    pub(crate) plane_count: u32,
    pub(crate) size: u32,
    pub(crate) planes: Vec<CcsProbePlane>,
    pub(crate) fd: OwnedFd,
    _guard: ProbeImageGuard,
}

#[cfg(test)]
struct ProbeImageGuard {
    device: ash::Device,
    image: vk::Image,
    memory: vk::DeviceMemory,
}

#[cfg(test)]
impl Drop for ProbeImageGuard {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_image(self.image, None);
            self.device.free_memory(self.memory, None);
        }
    }
}

#[cfg(test)]
pub(crate) fn probe_ccs_renderable_nv12(
    interop: &DmaBufInterop,
    resolution: VideoResolution,
) -> Result<CcsProbe, DmaBufError> {
    let vulkan = &interop.vulkan;
    for m in vulkan.nv12_modifiers.iter() {
        tracing::warn!(
            target: "gpu_video",
            "nv12 drm modifier {:#018x} plane_count={} tiling={:?}",
            m.drm_format_modifier,
            m.drm_format_modifier_plane_count,
            m.drm_format_modifier_tiling_features,
        );
    }

    let ccs: Vec<u64> = vulkan
        .nv12_modifiers
        .iter()
        .filter(|m| m.drm_format_modifier_plane_count > 2)
        .map(|m| m.drm_format_modifier)
        .collect();
    if ccs.is_empty() {
        return Err(DmaBufError::UnsupportedDevice(
            "no render-compression (CCS, plane_count>2) NV12 modifier is advertised".into(),
        ));
    }
    tracing::warn!(
        target: "gpu_video",
        "probing {} CCS modifier(s): {:#x?}",
        ccs.len(),
        ccs,
    );

    unsafe { create_ccs_probe(vulkan, resolution, &ccs) }
}

#[cfg(test)]
unsafe fn create_ccs_probe(
    vulkan: &VulkanDmaBufDevice,
    resolution: VideoResolution,
    modifiers: &[u64],
) -> Result<CcsProbe, DmaBufError> {
    let device = &vulkan.device;
    let usage =
        vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC;
    let flags =
        vk::ImageCreateFlags::MUTABLE_FORMAT | vk::ImageCreateFlags::EXTENDED_USAGE;

    let mut external = vk::ExternalMemoryImageCreateInfo::default()
        .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
    let mut modifier_list = vk::ImageDrmFormatModifierListCreateInfoEXT::default()
        .drm_format_modifiers(modifiers);
    let create_info = nv12_image_create_info(resolution, flags, usage)
        .push_next(&mut external)
        .push_next(&mut modifier_list);

    let image = unsafe { device.create_image(&create_info, None) }.map_err(|err| {
        DmaBufError::UnsupportedDevice(format!(
            "failed to create CCS renderable NV12 image: {err}"
        ))
    })?;

    let requirements = unsafe { device.get_image_memory_requirements(image) };
    let memory_index =
        match find_memory_type(&vulkan.memory_properties, requirements.memory_type_bits) {
            Some(index) => index,
            None => {
                unsafe { device.destroy_image(image, None) };
                return Err(DmaBufError::UnsupportedDevice(
                    "no memory type for CCS NV12 image".into(),
                ));
            }
        };

    let mut export_info = vk::ExportMemoryAllocateInfo::default()
        .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
    let mut dedicated = vk::MemoryDedicatedAllocateInfo::default().image(image);
    let allocate_info = vk::MemoryAllocateInfo::default()
        .allocation_size(requirements.size)
        .memory_type_index(memory_index)
        .push_next(&mut export_info)
        .push_next(&mut dedicated);
    let memory = match unsafe { device.allocate_memory(&allocate_info, None) } {
        Ok(memory) => memory,
        Err(err) => {
            unsafe { device.destroy_image(image, None) };
            return Err(DmaBufError::Vulkan(format!(
                "failed to allocate CCS export memory: {err}"
            )));
        }
    };
    let guard = ProbeImageGuard { device: device.clone(), image, memory };

    unsafe { device.bind_image_memory(image, memory, 0) }
        .map_err(|err| DmaBufError::Vulkan(format!("failed to bind CCS memory: {err}")))?;

    let modifier_ext =
        ash::ext::image_drm_format_modifier::Device::new(&vulkan.instance, device);
    let mut modifier_props = vk::ImageDrmFormatModifierPropertiesEXT::default();
    unsafe {
        modifier_ext.get_image_drm_format_modifier_properties(image, &mut modifier_props)
    }
    .map_err(|err| {
        DmaBufError::Vulkan(format!("failed to query chosen CCS modifier: {err}"))
    })?;
    let modifier = modifier_props.drm_format_modifier;

    let plane_count = vulkan
        .nv12_modifiers
        .iter()
        .find(|m| m.drm_format_modifier == modifier)
        .map(|m| m.drm_format_modifier_plane_count)
        .unwrap_or(2);

    const ASPECTS: [vk::ImageAspectFlags; 4] = [
        vk::ImageAspectFlags::MEMORY_PLANE_0_EXT,
        vk::ImageAspectFlags::MEMORY_PLANE_1_EXT,
        vk::ImageAspectFlags::MEMORY_PLANE_2_EXT,
        vk::ImageAspectFlags::MEMORY_PLANE_3_EXT,
    ];
    let planes: Vec<CcsProbePlane> = (0..plane_count as usize)
        .map(|i| {
            let layout = unsafe {
                device.get_image_subresource_layout(
                    image,
                    vk::ImageSubresource {
                        aspect_mask: ASPECTS[i],
                        mip_level: 0,
                        array_layer: 0,
                    },
                )
            };
            CcsProbePlane {
                offset: layout.offset as u32,
                pitch: layout.row_pitch as u32,
            }
        })
        .collect();

    let fd = unsafe {
        vulkan.external_memory_fd.get_memory_fd(
            &vk::MemoryGetFdInfoKHR::default()
                .memory(memory)
                .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT),
        )
    }
    .map_err(|err| DmaBufError::Vulkan(format!("failed to export CCS dma-buf fd: {err}")))?;

    Ok(CcsProbe {
        modifier,
        plane_count,
        size: requirements.size as u32,
        planes,
        fd: unsafe { OwnedFd::from_raw_fd(fd) },
        _guard: guard,
    })
}
