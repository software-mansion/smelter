use std::{
    fmt,
    os::fd::{AsFd, AsRawFd, FromRawFd, IntoRawFd, OwnedFd},
    sync::Arc,
};

use ash::vk;
use wgpu::hal::api::Vulkan as VkApi;

use crate::VideoResolution;

pub const DRM_FORMAT_NV12: u32 = u32::from_le_bytes(*b"NV12");

#[derive(Debug, thiserror::Error)]
pub enum DmaBufError {
    #[error("invalid NV12 DMA-BUF layout: {0}")]
    InvalidLayout(String),

    #[error("unsupported DMA-BUF device: {0}")]
    UnsupportedDevice(String),

    #[error("Vulkan DMA-BUF error: {0}")]
    Vulkan(String),

    #[error("failed to duplicate DMA-BUF fd: {0}")]
    DuplicateFd(#[from] std::io::Error),
}

#[derive(Clone)]
pub struct DmaBufFrame {
    fourcc: u32,
    width: u32,
    height: u32,
    objects: Vec<DmaBufObject>,
    layers: Vec<DmaBufLayer>,
    texture: Arc<wgpu::Texture>,
    _owner: Option<Arc<dyn Send + Sync>>,
}

impl DmaBufFrame {
    pub(crate) fn new_with_owner(
        texture: Arc<wgpu::Texture>,
        fourcc: u32,
        width: u32,
        height: u32,
        objects: Vec<DmaBufObject>,
        layers: Vec<DmaBufLayer>,
        owner: Option<Arc<dyn Send + Sync>>,
    ) -> Self {
        assert!(
            !objects.is_empty() && objects.len() <= 4,
            "DMA-BUF frame must have 1..=4 objects"
        );
        assert!(
            !layers.is_empty() && layers.len() <= 4,
            "DMA-BUF frame must have 1..=4 layers"
        );
        for layer in &layers {
            assert!(
                !layer.planes.is_empty() && layer.planes.len() <= 4,
                "DMA-BUF layer must have 1..=4 planes"
            );
            for plane in &layer.planes {
                assert!(
                    plane.object_index < objects.len(),
                    "DMA-BUF plane references a missing object"
                );
                assert!(
                    plane.offset <= objects[plane.object_index].size,
                    "DMA-BUF plane offset exceeds object size"
                );
            }
        }
        Self { fourcc, width, height, objects, layers, texture, _owner: owner }
    }

    pub fn texture_arc(&self) -> Arc<wgpu::Texture> {
        Arc::clone(&self.texture)
    }

    pub fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }

    pub fn fourcc(&self) -> u32 {
        self.fourcc
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn resolution(&self) -> VideoResolution {
        VideoResolution { width: self.width, height: self.height }
    }

    pub(crate) fn objects(&self) -> &[DmaBufObject] {
        &self.objects
    }

    pub(crate) fn layers(&self) -> &[DmaBufLayer] {
        &self.layers
    }
}

impl fmt::Debug for DmaBufFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DMA-BUF frame")
            .field("fourcc", &self.fourcc)
            .field("width", &self.width)
            .field("height", &self.height)
            .field("objects", &self.objects)
            .field("layers", &self.layers)
            .finish()
    }
}

#[derive(Clone)]
pub(crate) struct DmaBufObject {
    pub(crate) fd: Arc<OwnedFd>,
    pub(crate) size: u32,
    pub(crate) modifier: u64,
}

impl fmt::Debug for DmaBufObject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DMA-BUF object")
            .field("size", &self.size)
            .field("modifier", &self.modifier)
            .finish()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct DmaBufLayer {
    pub(crate) drm_format: u32,
    pub(crate) planes: Vec<DmaBufPlane>,
}

#[derive(Debug, Clone)]
pub(crate) struct DmaBufPlane {
    pub(crate) object_index: usize,
    pub(crate) offset: u32,
    pub(crate) pitch: u32,
}

pub(crate) fn validate_nv12_dmabuf_frame(
    frame: &DmaBufFrame,
    expected_resolution: VideoResolution,
) -> Result<(), DmaBufError> {
    if frame.resolution() != expected_resolution {
        return Err(DmaBufError::InvalidLayout(format!(
            "expected NV12 DMA-BUF resolution {:?}, got {:?}",
            expected_resolution,
            frame.resolution()
        )));
    }

    validate_nv12_dmabuf_layout(
        frame.fourcc(),
        frame.width(),
        frame.height(),
        frame.objects(),
        frame.layers(),
    )
}

pub(crate) fn validate_nv12_dmabuf_layout(
    fourcc: u32,
    width: u32,
    height: u32,
    objects: &[DmaBufObject],
    layers: &[DmaBufLayer],
) -> Result<(), DmaBufError> {
    if fourcc != DRM_FORMAT_NV12 {
        return Err(DmaBufError::InvalidLayout(format!(
            "expected NV12 DMA-BUF fourcc {DRM_FORMAT_NV12}, got {fourcc}"
        )));
    }
    if width == 0 || height == 0 {
        return Err(DmaBufError::InvalidLayout(format!(
            "NV12 DMA-BUF has invalid size {width}x{height}"
        )));
    }
    if objects.is_empty() || objects.len() > 4 {
        return Err(DmaBufError::InvalidLayout(format!(
            "NV12 DMA-BUF object count {} is outside supported limit 1..=4",
            objects.len()
        )));
    }
    if layers.len() != 1 {
        return Err(DmaBufError::InvalidLayout(format!(
            "NV12 DMA-BUF requires one layer, got {}",
            layers.len()
        )));
    }

    let layer = &layers[0];
    if layer.drm_format != DRM_FORMAT_NV12 {
        return Err(DmaBufError::InvalidLayout(format!(
            "expected NV12 DMA-BUF layer drm format {DRM_FORMAT_NV12}, got {}",
            layer.drm_format
        )));
    }
    if layer.planes.len() != 2 {
        return Err(DmaBufError::InvalidLayout(format!(
            "NV12 DMA-BUF requires two planes, got {}",
            layer.planes.len()
        )));
    }

    validate_nv12_plane("Y", &layer.planes[0], objects, width, height)?;
    validate_nv12_plane("UV", &layer.planes[1], objects, width, height.div_ceil(2))
}

fn validate_nv12_plane(
    name: &str,
    plane: &DmaBufPlane,
    objects: &[DmaBufObject],
    min_pitch: u32,
    rows: u32,
) -> Result<(), DmaBufError> {
    let object = objects.get(plane.object_index).ok_or_else(|| {
        DmaBufError::InvalidLayout(format!(
            "NV12 DMA-BUF {name} plane references object {}, but only {} objects exist",
            plane.object_index,
            objects.len()
        ))
    })?;
    if plane.pitch < min_pitch {
        return Err(DmaBufError::InvalidLayout(format!(
            "NV12 DMA-BUF {name} plane pitch {} is smaller than required width {min_pitch}",
            plane.pitch
        )));
    }
    let plane_end = u64::from(plane.offset)
        .checked_add(u64::from(plane.pitch) * u64::from(rows))
        .ok_or_else(|| {
            DmaBufError::InvalidLayout(format!(
                "NV12 DMA-BUF {name} plane byte range overflows"
            ))
        })?;
    if plane_end > u64::from(object.size) {
        return Err(DmaBufError::InvalidLayout(format!(
            "NV12 DMA-BUF {name} plane range {plane_end} exceeds object {} size {}",
            plane.object_index, object.size
        )));
    }
    Ok(())
}

fn nv12_import_image_usage() -> vk::ImageUsageFlags {
    vk::ImageUsageFlags::SAMPLED
        | vk::ImageUsageFlags::TRANSFER_DST
        | vk::ImageUsageFlags::TRANSFER_SRC
}

fn nv12_import_format_features() -> vk::FormatFeatureFlags2 {
    vk::FormatFeatureFlags2::SAMPLED_IMAGE
        | vk::FormatFeatureFlags2::TRANSFER_DST
        | vk::FormatFeatureFlags2::TRANSFER_SRC
}

pub fn export_nv12_dmabuf_texture(
    wgpu_device: &wgpu::Device,
    resolution: VideoResolution,
) -> Result<Arc<DmaBufFrame>, DmaBufError> {
    unsafe {
        let hal_device_guard = wgpu_device.as_hal::<VkApi>().ok_or_else(|| {
            DmaBufError::UnsupportedDevice(
                "NV12 DMA-BUF output requires a Vulkan wgpu device".into(),
            )
        })?;
        let hal_device = &*hal_device_guard;
        let vk_device = hal_device.raw_device().clone();
        let instance = hal_device.shared_instance().raw_instance();
        let physical_device = hal_device.raw_physical_device();
        let size =
            vk::Extent3D { width: resolution.width, height: resolution.height, depth: 1 };

        let modifier = select_nv12_modifier(instance, physical_device)?;
        let modifiers = [modifier];
        let mut external_info = vk::ExternalMemoryImageCreateInfo::default()
            .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
        let mut drm_info = vk::ImageDrmFormatModifierListCreateInfoEXT::default()
            .drm_format_modifiers(&modifiers);
        let create_info = vk::ImageCreateInfo::default()
            .flags(
                vk::ImageCreateFlags::MUTABLE_FORMAT
                    | vk::ImageCreateFlags::EXTENDED_USAGE,
            )
            .image_type(vk::ImageType::TYPE_2D)
            .format(vk::Format::G8_B8R8_2PLANE_420_UNORM)
            .extent(size)
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
            .usage(
                vk::ImageUsageFlags::SAMPLED
                    | vk::ImageUsageFlags::COLOR_ATTACHMENT
                    | vk::ImageUsageFlags::TRANSFER_DST
                    | vk::ImageUsageFlags::TRANSFER_SRC,
            )
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .push_next(&mut external_info)
            .push_next(&mut drm_info);

        let image = vk_device.create_image(&create_info, None).map_err(|err| {
            DmaBufError::Vulkan(format!(
                "failed to create exportable NV12 Vulkan image: {err}"
            ))
        })?;
        let mem_requirements = vk_device.get_image_memory_requirements(image);
        let memory_type_index =
            find_memory_type_index(instance, physical_device, &mem_requirements)?;
        let mut export_info = vk::ExportMemoryAllocateInfo::default()
            .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
        let mut dedicated_info = vk::MemoryDedicatedAllocateInfo::default().image(image);
        let allocate_info = vk::MemoryAllocateInfo::default()
            .allocation_size(mem_requirements.size)
            .memory_type_index(memory_type_index)
            .push_next(&mut export_info)
            .push_next(&mut dedicated_info);
        let memory = match vk_device.allocate_memory(&allocate_info, None) {
            Ok(memory) => memory,
            Err(err) => {
                vk_device.destroy_image(image, None);
                return Err(DmaBufError::Vulkan(format!(
                    "failed to allocate exportable NV12 Vulkan memory: {err}"
                )));
            }
        };
        if let Err(err) = vk_device.bind_image_memory(image, memory, 0) {
            vk_device.destroy_image(image, None);
            vk_device.free_memory(memory, None);
            return Err(DmaBufError::Vulkan(format!(
                "failed to bind exportable NV12 Vulkan memory: {err}"
            )));
        }

        let external_memory_fd =
            ash::khr::external_memory_fd::Device::new(instance, &vk_device);
        let fd_info = vk::MemoryGetFdInfoKHR::default()
            .memory(memory)
            .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
        let fd = Arc::new(OwnedFd::from_raw_fd(
            external_memory_fd.get_memory_fd(&fd_info).map_err(|err| {
                DmaBufError::Vulkan(format!("failed to export NV12 DMA-BUF fd: {err}"))
            })?,
        ));

        let plane0 = image_plane_layout(&vk_device, image, vk::ImageAspectFlags::PLANE_0);
        let plane1 = image_plane_layout(&vk_device, image, vk::ImageAspectFlags::PLANE_1);
        let texture = Arc::new(wrap_nv12_image_as_wgpu_texture(
            wgpu_device,
            image,
            memory,
            vk_device,
            resolution,
            "nv12 dma-buf output texture",
            true,
        )?);

        Ok(Arc::new(DmaBufFrame::new_with_owner(
            texture,
            DRM_FORMAT_NV12,
            resolution.width,
            resolution.height,
            vec![DmaBufObject {
                fd,
                size: mem_requirements.size.try_into().map_err(|_| {
                    DmaBufError::InvalidLayout(
                        "DMA-BUF allocation is larger than VA-API can describe".into(),
                    )
                })?,
                modifier,
            }],
            vec![DmaBufLayer {
                drm_format: DRM_FORMAT_NV12,
                planes: vec![
                    DmaBufPlane {
                        object_index: 0,
                        offset: plane0.offset.try_into().map_err(|_| {
                            DmaBufError::InvalidLayout(
                                "NV12 Y offset does not fit u32".into(),
                            )
                        })?,
                        pitch: plane0.row_pitch.try_into().map_err(|_| {
                            DmaBufError::InvalidLayout(
                                "NV12 Y pitch does not fit u32".into(),
                            )
                        })?,
                    },
                    DmaBufPlane {
                        object_index: 0,
                        offset: plane1.offset.try_into().map_err(|_| {
                            DmaBufError::InvalidLayout(
                                "NV12 UV offset does not fit u32".into(),
                            )
                        })?,
                        pitch: plane1.row_pitch.try_into().map_err(|_| {
                            DmaBufError::InvalidLayout(
                                "NV12 UV pitch does not fit u32".into(),
                            )
                        })?,
                    },
                ],
            }],
            None,
        )))
    }
}

pub(crate) fn import_nv12_dmabuf_texture(
    wgpu_device: &wgpu::Device,
    fourcc: u32,
    width: u32,
    height: u32,
    objects: Vec<DmaBufObject>,
    layers: Vec<DmaBufLayer>,
    owner: Option<Arc<dyn Send + Sync>>,
) -> Result<Arc<DmaBufFrame>, DmaBufError> {
    validate_nv12_dmabuf_layout(fourcc, width, height, &objects, &layers)?;
    if objects.len() != 1 {
        return Err(DmaBufError::UnsupportedDevice(format!(
            "WGPU NV12 DMA-BUF import supports one object, got {}",
            objects.len()
        )));
    }

    unsafe {
        let hal_device_guard = wgpu_device.as_hal::<VkApi>().ok_or_else(|| {
            DmaBufError::UnsupportedDevice(
                "NV12 DMA-BUF import requires a Vulkan wgpu device".into(),
            )
        })?;
        let hal_device = &*hal_device_guard;
        let vk_device = hal_device.raw_device().clone();
        let instance = hal_device.shared_instance().raw_instance();
        let physical_device = hal_device.raw_physical_device();
        let size = vk::Extent3D { width, height, depth: 1 };
        let modifier = objects[0].modifier;
        validate_nv12_modifier_support(
            instance,
            physical_device,
            modifier,
            nv12_import_format_features(),
        )?;
        let plane_layouts = layers[0]
            .planes
            .iter()
            .map(|plane| vk::SubresourceLayout {
                offset: plane.offset as u64,
                size: objects[plane.object_index].size as u64 - plane.offset as u64,
                row_pitch: plane.pitch as u64,
                array_pitch: 0,
                depth_pitch: 0,
            })
            .collect::<Vec<_>>();

        let mut external_info = vk::ExternalMemoryImageCreateInfo::default()
            .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
        let mut drm_info = vk::ImageDrmFormatModifierExplicitCreateInfoEXT::default()
            .drm_format_modifier(modifier)
            .plane_layouts(&plane_layouts);

        let create_info = vk::ImageCreateInfo::default()
            .flags(
                vk::ImageCreateFlags::MUTABLE_FORMAT
                    | vk::ImageCreateFlags::EXTENDED_USAGE,
            )
            .image_type(vk::ImageType::TYPE_2D)
            .format(vk::Format::G8_B8R8_2PLANE_420_UNORM)
            .extent(size)
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
            .usage(nv12_import_image_usage())
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .push_next(&mut external_info)
            .push_next(&mut drm_info);

        let image = vk_device.create_image(&create_info, None).map_err(|err| {
            DmaBufError::Vulkan(format!(
                "failed to create imported NV12 Vulkan image: {err}"
            ))
        })?;
        let mem_requirements = vk_device.get_image_memory_requirements(image);
        let memory_type_index =
            find_memory_type_index(instance, physical_device, &mem_requirements)?;
        let import_fd = objects[0].fd.as_fd().try_clone_to_owned()?;
        let mut import_info = vk::ImportMemoryFdInfoKHR::default()
            .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT)
            .fd(import_fd.as_raw_fd());
        let mut dedicated_info = vk::MemoryDedicatedAllocateInfo::default().image(image);
        let allocate_info = vk::MemoryAllocateInfo::default()
            .allocation_size(mem_requirements.size)
            .memory_type_index(memory_type_index)
            .push_next(&mut import_info)
            .push_next(&mut dedicated_info);
        let memory = match vk_device.allocate_memory(&allocate_info, None) {
            Ok(memory) => {
                let _ = import_fd.into_raw_fd();
                memory
            }
            Err(err) => {
                vk_device.destroy_image(image, None);
                return Err(DmaBufError::Vulkan(format!(
                    "failed to import NV12 DMA-BUF memory: {err}"
                )));
            }
        };
        if let Err(err) = vk_device.bind_image_memory(image, memory, 0) {
            vk_device.destroy_image(image, None);
            vk_device.free_memory(memory, None);
            return Err(DmaBufError::Vulkan(format!(
                "failed to bind imported NV12 DMA-BUF memory: {err}"
            )));
        }

        let texture = Arc::new(wrap_nv12_image_as_wgpu_texture(
            wgpu_device,
            image,
            memory,
            vk_device,
            VideoResolution { width, height },
            "imported nv12 dma-buf texture",
            false,
        )?);

        Ok(Arc::new(DmaBufFrame::new_with_owner(
            texture, fourcc, width, height, objects, layers, owner,
        )))
    }
}

unsafe fn wrap_nv12_image_as_wgpu_texture(
    wgpu_device: &wgpu::Device,
    image: vk::Image,
    memory: vk::DeviceMemory,
    device: ash::Device,
    resolution: VideoResolution,
    label: &'static str,
    render_attachment: bool,
) -> Result<wgpu::Texture, DmaBufError> {
    let hal_device_guard = unsafe {
        wgpu_device.as_hal::<VkApi>().ok_or_else(|| {
            DmaBufError::UnsupportedDevice(
                "NV12 DMA-BUF requires a Vulkan wgpu device".into(),
            )
        })?
    };
    let hal_texture = unsafe {
        let mut hal_usage = wgpu::TextureUses::RESOURCE
            | wgpu::TextureUses::COPY_DST
            | wgpu::TextureUses::COPY_SRC;
        if render_attachment {
            hal_usage |= wgpu::TextureUses::COLOR_TARGET;
        }

        (*hal_device_guard).texture_from_raw(
            image,
            &wgpu::hal::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: resolution.width,
                    height: resolution.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::NV12,
                usage: hal_usage,
                memory_flags: wgpu::hal::MemoryFlags::empty(),
                view_formats: vec![
                    wgpu::TextureFormat::R8Unorm,
                    wgpu::TextureFormat::Rg8Unorm,
                ],
            },
            Some(Box::new(move || {
                device.destroy_image(image, None);
                device.free_memory(memory, None);
            })),
            wgpu::hal::vulkan::TextureMemory::External,
        )
    };

    let mut wgpu_usage = wgpu::TextureUsages::TEXTURE_BINDING
        | wgpu::TextureUsages::COPY_DST
        | wgpu::TextureUsages::COPY_SRC;
    if render_attachment {
        wgpu_usage |= wgpu::TextureUsages::RENDER_ATTACHMENT;
    }
    let initial_state = if render_attachment {
        wgpu::TextureUses::UNINITIALIZED
    } else {
        wgpu::TextureUses::RESOURCE
    };

    Ok(unsafe {
        wgpu_device.create_texture_from_hal::<VkApi>(
            hal_texture,
            &wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: resolution.width,
                    height: resolution.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::NV12,
                usage: wgpu_usage,
                view_formats: &[
                    wgpu::TextureFormat::R8Unorm,
                    wgpu::TextureFormat::Rg8Unorm,
                ],
            },
            initial_state,
        )
    })
}

unsafe fn image_plane_layout(
    device: &ash::Device,
    image: vk::Image,
    aspect_mask: vk::ImageAspectFlags,
) -> vk::SubresourceLayout {
    unsafe {
        device.get_image_subresource_layout(
            image,
            vk::ImageSubresource { aspect_mask, mip_level: 0, array_layer: 0 },
        )
    }
}

fn select_nv12_modifier(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
) -> Result<u64, DmaBufError> {
    let required = nv12_import_format_features();
    let modifiers = nv12_modifier_properties(instance, physical_device);
    modifiers
        .iter()
        .find(|modifier| {
            supports_nv12_modifier(modifier, required)
                && modifier.drm_format_modifier != 0
        })
        .or_else(|| {
            modifiers.iter().find(|modifier| supports_nv12_modifier(modifier, required))
        })
        .copied()
        .map(|modifier| modifier.drm_format_modifier)
        .ok_or_else(|| {
            DmaBufError::UnsupportedDevice(
                "no exportable NV12 DRM modifier with sampled and transfer support available"
                    .into(),
            )
        })
}

fn validate_nv12_modifier_support(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    modifier: u64,
    required: vk::FormatFeatureFlags2,
) -> Result<(), DmaBufError> {
    let modifiers = nv12_modifier_properties(instance, physical_device);
    let Some(properties) =
        modifiers.iter().find(|properties| properties.drm_format_modifier == modifier)
    else {
        return Err(DmaBufError::UnsupportedDevice(format!(
            "NV12 DMA-BUF modifier {modifier:#x} is not supported by the WGPU Vulkan device; available modifiers: {}",
            format_nv12_modifiers(&modifiers)
        )));
    };

    if supports_nv12_modifier(properties, required) {
        return Ok(());
    }

    Err(DmaBufError::UnsupportedDevice(format!(
        "NV12 DMA-BUF modifier {modifier:#x} has {:?} with {} planes, but import requires {required:?}",
        properties.drm_format_modifier_tiling_features,
        properties.drm_format_modifier_plane_count,
    )))
}

fn supports_nv12_modifier(
    modifier: &vk::DrmFormatModifierProperties2EXT,
    required: vk::FormatFeatureFlags2,
) -> bool {
    modifier.drm_format_modifier_plane_count == 2
        && modifier.drm_format_modifier_tiling_features.contains(required)
}

fn nv12_modifier_properties(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
) -> Vec<vk::DrmFormatModifierProperties2EXT> {
    unsafe {
        let mut count = vk::DrmFormatModifierPropertiesList2EXT::default();
        let mut properties = vk::FormatProperties2::default().push_next(&mut count);
        instance.get_physical_device_format_properties2(
            physical_device,
            vk::Format::G8_B8R8_2PLANE_420_UNORM,
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
            vk::Format::G8_B8R8_2PLANE_420_UNORM,
            &mut properties,
        );

        modifiers
    }
}

fn format_nv12_modifiers(modifiers: &[vk::DrmFormatModifierProperties2EXT]) -> String {
    modifiers
        .iter()
        .map(|modifier| {
            format!(
                "{:#x}({:?}, planes={})",
                modifier.drm_format_modifier,
                modifier.drm_format_modifier_tiling_features,
                modifier.drm_format_modifier_plane_count,
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn find_memory_type_index(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    mem_requirements: &vk::MemoryRequirements,
) -> Result<u32, DmaBufError> {
    let memory_properties =
        unsafe { instance.get_physical_device_memory_properties(physical_device) };

    (0..memory_properties.memory_type_count)
        .find(|index| {
            let allowed = mem_requirements.memory_type_bits & (1 << index) != 0;
            let flags = memory_properties.memory_types[*index as usize].property_flags;
            allowed && flags.contains(vk::MemoryPropertyFlags::DEVICE_LOCAL)
        })
        .ok_or_else(|| {
            DmaBufError::UnsupportedDevice(
                "no device-local memory type available for NV12 DMA-BUF image".into(),
            )
        })
}
