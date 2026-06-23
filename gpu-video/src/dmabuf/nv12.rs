use std::{
    fmt,
    os::fd::{AsFd, AsRawFd, IntoRawFd, OwnedFd},
    sync::{Arc, Mutex, MutexGuard},
};

use ash::vk;
use drm_fourcc::DrmFourcc;

use crate::VideoResolution;

use super::{DmaBufInterop, interop::VulkanDmaBufDevice, vulkan};

pub(crate) const DRM_FORMAT_NV12: u32 = DrmFourcc::Nv12 as u32;
const VK_NV12_FORMAT: vk::Format = vk::Format::G8_B8R8_2PLANE_420_UNORM;

#[derive(Debug, thiserror::Error)]
pub(crate) enum DmaBufError {
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
pub(crate) struct DmaBufFrame {
    descriptor: Nv12DmaBufDescriptor,
    texture: Arc<wgpu::Texture>,
}

impl DmaBufFrame {
    fn new(texture: Arc<wgpu::Texture>, descriptor: Nv12DmaBufDescriptor) -> Self {
        assert_eq!(
            texture.format(),
            wgpu::TextureFormat::NV12,
            "DMA-BUF frame must wrap an NV12 texture"
        );
        Self { descriptor, texture }
    }

    pub(crate) fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }

    pub(crate) fn objects(&self) -> &[DmaBufObject] {
        self.descriptor.objects()
    }

    pub(crate) fn sync_guard(&self) -> MutexGuard<'_, ()> {
        self.descriptor.sync_guard()
    }
}

impl fmt::Debug for DmaBufFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DMA-BUF frame")
            .field("format", &self.texture.format())
            .field("size", &self.texture.size())
            .field("objects", &self.descriptor.objects())
            .field("layer", &self.descriptor.layer())
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

#[derive(Debug, Clone, Copy)]
pub(crate) struct Nv12DmaBufLayer {
    pub(crate) planes: [DmaBufPlane; 2],
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct DmaBufPlane {
    pub(crate) object_index: usize,
    pub(crate) offset: u32,
    pub(crate) pitch: u32,
}

#[derive(Clone)]
pub(crate) struct Nv12DmaBufDescriptor {
    pub(crate) resolution: VideoResolution,
    memory: Nv12DmaBufMemory,
    sync_lock: Arc<Mutex<()>>,
}

impl Nv12DmaBufDescriptor {
    pub(crate) fn new(
        resolution: VideoResolution,
        objects: Box<[DmaBufObject]>,
        layer: Nv12DmaBufLayer,
    ) -> Result<Self, DmaBufError> {
        let memory = validate_nv12_dmabuf_layout(resolution, objects, layer)?;
        Ok(Self { resolution, memory, sync_lock: Arc::new(Mutex::new(())) })
    }

    fn objects(&self) -> &[DmaBufObject] {
        self.memory.objects()
    }

    fn layer(&self) -> Nv12DmaBufLayer {
        self.memory.layer()
    }

    fn modifier(&self) -> u64 {
        self.memory.modifier()
    }

    fn plane_layouts(&self) -> [vk::SubresourceLayout; 2] {
        self.memory.plane_layouts()
    }

    fn sync_guard(&self) -> MutexGuard<'_, ()> {
        self.sync_lock.lock().expect("DMA-BUF frame sync lock poisoned")
    }
}

impl DmaBufPlane {
    fn subresource_layout(self) -> vk::SubresourceLayout {
        vk::SubresourceLayout {
            offset: self.offset as u64,
            size: 0,
            row_pitch: self.pitch as u64,
            array_pitch: 0,
            depth_pitch: 0,
        }
    }
}

#[derive(Debug, Clone)]
enum Nv12DmaBufMemory {
    Single { objects: [DmaBufObject; 1], planes: [DmaBufPlane; 2] },
    Disjoint { objects: [DmaBufObject; 2], planes: [DmaBufPlane; 2] },
}

impl Nv12DmaBufMemory {
    fn objects(&self) -> &[DmaBufObject] {
        match self {
            Self::Single { objects, .. } => objects,
            Self::Disjoint { objects, .. } => objects,
        }
    }

    fn planes(&self) -> [DmaBufPlane; 2] {
        match self {
            Self::Single { planes, .. } | Self::Disjoint { planes, .. } => *planes,
        }
    }

    fn modifier(&self) -> u64 {
        self.objects()[0].modifier
    }

    fn image_flags(&self) -> vk::ImageCreateFlags {
        let flags =
            vk::ImageCreateFlags::MUTABLE_FORMAT | vk::ImageCreateFlags::EXTENDED_USAGE;
        match self {
            Self::Single { .. } => flags,
            Self::Disjoint { .. } => flags | vk::ImageCreateFlags::DISJOINT,
        }
    }

    fn required_import_features(&self) -> vk::FormatFeatureFlags2 {
        let features = nv12_format_features();
        match self {
            Self::Single { .. } => features,
            Self::Disjoint { .. } => features | vk::FormatFeatureFlags2::DISJOINT,
        }
    }

    fn layer(&self) -> Nv12DmaBufLayer {
        Nv12DmaBufLayer { planes: self.planes() }
    }

    fn plane_layouts(&self) -> [vk::SubresourceLayout; 2] {
        self.planes().map(DmaBufPlane::subresource_layout)
    }
}

struct VulkanImageMemory {
    device: ash::Device,
    image: vk::Image,
    memories: Vec<vk::DeviceMemory>,
}

impl VulkanImageMemory {
    fn new(device: ash::Device, image: vk::Image) -> Self {
        Self { device, image, memories: Vec::new() }
    }

    fn push_memory(&mut self, memory: vk::DeviceMemory) {
        self.memories.push(memory);
    }

    fn into_raw(mut self) -> (vk::Image, Vec<vk::DeviceMemory>, ash::Device) {
        let image = std::mem::replace(&mut self.image, vk::Image::null());
        let memories = std::mem::take(&mut self.memories);
        let device = self.device.clone();
        (image, memories, device)
    }
}

impl Drop for VulkanImageMemory {
    fn drop(&mut self) {
        if self.image == vk::Image::null() {
            return;
        }
        unsafe {
            self.device.destroy_image(self.image, None);
            for memory in self.memories.drain(..) {
                self.device.free_memory(memory, None);
            }
        }
    }
}

fn validate_nv12_dmabuf_layout(
    resolution: VideoResolution,
    objects: Box<[DmaBufObject]>,
    layer: Nv12DmaBufLayer,
) -> Result<Nv12DmaBufMemory, DmaBufError> {
    let VideoResolution { width, height } = resolution;
    if width == 0 || height == 0 {
        return Err(DmaBufError::InvalidLayout(format!(
            "NV12 DMA-BUF has invalid size {width}x{height}"
        )));
    }
    if width % 2 != 0 || height % 2 != 0 {
        return Err(DmaBufError::InvalidLayout(format!(
            "NV12 DMA-BUF dimensions must be even, got {width}x{height}"
        )));
    }
    if objects.is_empty() || objects.len() > 2 {
        return Err(DmaBufError::InvalidLayout(format!(
            "NV12 DMA-BUF object count {} is outside supported limit 1..=2",
            objects.len()
        )));
    }
    if objects.iter().any(|object| object.modifier != objects[0].modifier) {
        return Err(DmaBufError::InvalidLayout(
            "NV12 DMA-BUF objects must share a single DRM modifier".into(),
        ));
    }
    let [y_plane, uv_plane] = layer.planes;
    validate_nv12_plane("Y", y_plane, &objects, width, height)?;
    validate_nv12_plane("UV", uv_plane, &objects, width, height / 2)?;

    match (y_plane.object_index == uv_plane.object_index, objects.len()) {
        (true, 1) => Ok(Nv12DmaBufMemory::Single {
            objects: [objects[0].clone()],
            planes: canonical_nv12_planes(y_plane, uv_plane, [0, 0]),
        }),
        (false, 2) => Ok(Nv12DmaBufMemory::Disjoint {
            objects: [
                objects[y_plane.object_index].clone(),
                objects[uv_plane.object_index].clone(),
            ],
            planes: canonical_nv12_planes(y_plane, uv_plane, [0, 1]),
        }),
        (true, count) => Err(DmaBufError::InvalidLayout(format!(
            "single-object NV12 DMA-BUF uses object {}, but descriptor has {count} objects",
            y_plane.object_index,
        ))),
        (false, count) => Err(DmaBufError::InvalidLayout(format!(
            "disjoint NV12 DMA-BUF requires one object per plane, got {count} objects",
        ))),
    }
}

fn canonical_nv12_planes(
    y_plane: DmaBufPlane,
    uv_plane: DmaBufPlane,
    object_indexes: [usize; 2],
) -> [DmaBufPlane; 2] {
    [
        DmaBufPlane {
            object_index: object_indexes[0],
            offset: y_plane.offset,
            pitch: y_plane.pitch,
        },
        DmaBufPlane {
            object_index: object_indexes[1],
            offset: uv_plane.offset,
            pitch: uv_plane.pitch,
        },
    ]
}

fn validate_nv12_plane(
    name: &str,
    plane: DmaBufPlane,
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

fn nv12_image_usage() -> vk::ImageUsageFlags {
    vk::ImageUsageFlags::SAMPLED
        | vk::ImageUsageFlags::TRANSFER_DST
        | vk::ImageUsageFlags::TRANSFER_SRC
}

fn nv12_format_features() -> vk::FormatFeatureFlags2 {
    vk::FormatFeatureFlags2::SAMPLED_IMAGE
        | vk::FormatFeatureFlags2::TRANSFER_DST
        | vk::FormatFeatureFlags2::TRANSFER_SRC
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
        .extent(vk::Extent3D {
            width: resolution.width,
            height: resolution.height,
            depth: 1,
        })
        .mip_levels(1)
        .array_layers(1)
        .samples(vk::SampleCountFlags::TYPE_1)
        .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
        .usage(usage)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .initial_layout(vk::ImageLayout::UNDEFINED)
}

pub(super) fn import_nv12_dmabuf_texture(
    interop: &DmaBufInterop,
    descriptor: Nv12DmaBufDescriptor,
) -> Result<Arc<DmaBufFrame>, DmaBufError> {
    let resolution = descriptor.resolution;
    let memory = &descriptor.memory;

    unsafe {
        let vulkan = &interop.vulkan;
        let vk_device = vulkan.device.clone();
        let image_usage = nv12_image_usage();
        validate_nv12_modifier_support(
            vulkan,
            Nv12ModifierSupportQuery {
                modifier: descriptor.modifier(),
                usage: image_usage,
                flags: memory.image_flags(),
                format_features: memory.required_import_features(),
                external_features: vk::ExternalMemoryFeatureFlags::IMPORTABLE,
                label: "NV12 DMA-BUF import",
            },
        )?;
        let plane_layouts = descriptor.plane_layouts();

        let mut external_info = vk::ExternalMemoryImageCreateInfo::default()
            .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
        let mut drm_info = vk::ImageDrmFormatModifierExplicitCreateInfoEXT::default()
            .drm_format_modifier(descriptor.modifier())
            .plane_layouts(&plane_layouts);
        let create_info =
            nv12_image_create_info(resolution, memory.image_flags(), image_usage)
                .push_next(&mut external_info)
                .push_next(&mut drm_info);

        let image = vk_device.create_image(&create_info, None).map_err(|err| {
            DmaBufError::Vulkan(format!(
                "failed to create imported NV12 Vulkan image: {err}"
            ))
        })?;
        let mut image_memory = VulkanImageMemory::new(vk_device.clone(), image);
        let memory_import = Nv12MemoryImport {
            device: &vk_device,
            external_memory_fd: &vulkan.external_memory_fd,
            memory_properties: &vulkan.memory_properties,
            image,
        };
        match memory {
            Nv12DmaBufMemory::Single { .. } => {
                memory_import.import_single(&descriptor.objects()[0], &mut image_memory)
            }
            Nv12DmaBufMemory::Disjoint { .. } => {
                memory_import.import_disjoint(descriptor.objects(), &mut image_memory)
            }
        }?;

        let (image, memories, vk_device) = image_memory.into_raw();
        let texture = Arc::new(vulkan::wrap_nv12_image_as_wgpu_texture(
            interop,
            image,
            memories,
            vk_device,
            descriptor.resolution,
            "imported nv12 dma-buf texture",
            wgpu::TextureUses::RESOURCE,
        )?);

        Ok(Arc::new(DmaBufFrame::new(texture, descriptor)))
    }
}

struct Nv12MemoryImport<'a> {
    device: &'a ash::Device,
    external_memory_fd: &'a ash::khr::external_memory_fd::Device,
    memory_properties: &'a vk::PhysicalDeviceMemoryProperties,
    image: vk::Image,
}

impl Nv12MemoryImport<'_> {
    unsafe fn import_single(
        &self,
        object: &DmaBufObject,
        image_memory: &mut VulkanImageMemory,
    ) -> Result<(), DmaBufError> {
        let requirements =
            unsafe { self.device.get_image_memory_requirements(self.image) };
        let memory = unsafe {
            self.import_dmabuf_memory(object, &requirements, "NV12 DMA-BUF import")?
        };
        image_memory.push_memory(memory);
        if let Err(err) = unsafe { self.device.bind_image_memory(self.image, memory, 0) }
        {
            return Err(DmaBufError::Vulkan(format!(
                "failed to bind imported NV12 DMA-BUF memory: {err}"
            )));
        }
        Ok(())
    }

    unsafe fn import_disjoint(
        &self,
        objects: &[DmaBufObject],
        image_memory: &mut VulkanImageMemory,
    ) -> Result<(), DmaBufError> {
        let planes = [
            (&objects[0], vk::ImageAspectFlags::PLANE_0, "NV12 Y plane DMA-BUF import"),
            (&objects[1], vk::ImageAspectFlags::PLANE_1, "NV12 UV plane DMA-BUF import"),
        ];
        for (object, aspect, label) in planes {
            let requirements = unsafe {
                vulkan::image_plane_memory_requirements(self.device, self.image, aspect)
            };
            let memory =
                unsafe { self.import_dmabuf_memory(object, &requirements, label) }?;
            image_memory.push_memory(memory);
        }

        let mut bind_plane0 = vk::BindImagePlaneMemoryInfo::default()
            .plane_aspect(vk::ImageAspectFlags::PLANE_0);
        let mut bind_plane1 = vk::BindImagePlaneMemoryInfo::default()
            .plane_aspect(vk::ImageAspectFlags::PLANE_1);
        let bind0 = vk::BindImageMemoryInfo::default()
            .image(self.image)
            .memory(image_memory.memories[0])
            .push_next(&mut bind_plane0);
        let bind1 = vk::BindImageMemoryInfo::default()
            .image(self.image)
            .memory(image_memory.memories[1])
            .push_next(&mut bind_plane1);
        if let Err(err) = unsafe { self.device.bind_image_memory2(&[bind0, bind1]) } {
            return Err(DmaBufError::Vulkan(format!(
                "failed to bind disjoint imported NV12 DMA-BUF memory: {err}"
            )));
        }

        Ok(())
    }

    unsafe fn import_dmabuf_memory(
        &self,
        object: &DmaBufObject,
        requirements: &vk::MemoryRequirements,
        label: &str,
    ) -> Result<vk::DeviceMemory, DmaBufError> {
        let import_fd = object.fd.as_fd().try_clone_to_owned()?;
        let mut fd_properties = vk::MemoryFdPropertiesKHR::default();
        unsafe {
            self.external_memory_fd.get_memory_fd_properties(
                vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT,
                import_fd.as_raw_fd(),
                &mut fd_properties,
            )
        }
        .map_err(|err| {
            DmaBufError::Vulkan(format!(
                "failed to query {label} memory properties: {err}"
            ))
        })?;

        let memory_type_index = find_memory_type_index_for_bits(
            self.memory_properties,
            requirements.memory_type_bits & fd_properties.memory_type_bits,
            vk::MemoryPropertyFlags::empty(),
            label,
        )?;
        let mut import_info = vk::ImportMemoryFdInfoKHR::default()
            .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT)
            .fd(import_fd.as_raw_fd());
        let mut dedicated_info =
            vk::MemoryDedicatedAllocateInfo::default().image(self.image);
        let allocate_info = vk::MemoryAllocateInfo::default()
            .allocation_size(requirements.size)
            .memory_type_index(memory_type_index)
            .push_next(&mut import_info)
            .push_next(&mut dedicated_info);
        match unsafe { self.device.allocate_memory(&allocate_info, None) } {
            Ok(memory) => {
                let _ = import_fd.into_raw_fd();
                Ok(memory)
            }
            Err(err) => Err(DmaBufError::Vulkan(format!(
                "failed to import {label} memory: {err}"
            ))),
        }
    }
}

#[derive(Clone, Copy)]
struct Nv12ModifierSupportQuery {
    modifier: u64,
    usage: vk::ImageUsageFlags,
    flags: vk::ImageCreateFlags,
    format_features: vk::FormatFeatureFlags2,
    external_features: vk::ExternalMemoryFeatureFlags,
    label: &'static str,
}

fn validate_nv12_modifier_support(
    vulkan: &VulkanDmaBufDevice,
    query: Nv12ModifierSupportQuery,
) -> Result<(), DmaBufError> {
    let modifiers = &vulkan.nv12_modifiers;
    let Some(properties) = modifiers
        .iter()
        .find(|properties| properties.drm_format_modifier == query.modifier)
    else {
        return Err(DmaBufError::UnsupportedDevice(format!(
            "NV12 DMA-BUF modifier {:#x} is not supported by the WGPU Vulkan device; available modifiers: {}",
            query.modifier,
            format_nv12_modifiers(modifiers)
        )));
    };

    if !supports_nv12_modifier(properties, query.format_features) {
        return Err(DmaBufError::UnsupportedDevice(format!(
            "NV12 DMA-BUF modifier {:#x} has {:?} with {} planes, but {} requires {:?}",
            query.modifier,
            properties.drm_format_modifier_tiling_features,
            properties.drm_format_modifier_plane_count,
            query.label,
            query.format_features,
        )));
    }

    validate_nv12_external_image_support(&vulkan.instance, vulkan.physical_device, query)
}

fn supports_nv12_modifier(
    modifier: &vk::DrmFormatModifierProperties2EXT,
    required: vk::FormatFeatureFlags2,
) -> bool {
    modifier.drm_format_modifier_plane_count == 2
        && modifier.drm_format_modifier_tiling_features.contains(required)
}

fn validate_nv12_external_image_support(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    query: Nv12ModifierSupportQuery,
) -> Result<(), DmaBufError> {
    unsafe {
        let mut external_info = vk::PhysicalDeviceExternalImageFormatInfo::default()
            .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
        let mut modifier_info =
            vk::PhysicalDeviceImageDrmFormatModifierInfoEXT::default()
                .drm_format_modifier(query.modifier)
                .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let format_info = vk::PhysicalDeviceImageFormatInfo2::default()
            .format(VK_NV12_FORMAT)
            .ty(vk::ImageType::TYPE_2D)
            .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
            .usage(query.usage)
            .flags(query.flags)
            .push_next(&mut external_info)
            .push_next(&mut modifier_info);
        let mut external_properties = vk::ExternalImageFormatProperties::default();
        let mut properties =
            vk::ImageFormatProperties2::default().push_next(&mut external_properties);

        instance
            .get_physical_device_image_format_properties2(
                physical_device,
                &format_info,
                &mut properties,
            )
            .map_err(|err| {
                DmaBufError::UnsupportedDevice(format!(
                    "{} does not support NV12 modifier {:#x} with usage {:?}: {err}",
                    query.label, query.modifier, query.usage
                ))
            })?;

        let features =
            external_properties.external_memory_properties.external_memory_features;
        if features.contains(query.external_features) {
            return Ok(());
        }

        Err(DmaBufError::UnsupportedDevice(format!(
            "{} modifier {:#x} external memory features {:?} do not include {:?}",
            query.label, query.modifier, features, query.external_features
        )))
    }
}

pub(super) fn nv12_modifier_properties(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
) -> Vec<vk::DrmFormatModifierProperties2EXT> {
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

fn find_memory_type_index_for_bits(
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    memory_type_bits: u32,
    required_flags: vk::MemoryPropertyFlags,
    label: &str,
) -> Result<u32, DmaBufError> {
    find_memory_type_index_for_properties(
        memory_properties,
        memory_type_bits,
        required_flags,
    )
    .ok_or_else(|| {
        DmaBufError::UnsupportedDevice(format!(
            "no compatible memory type available for {label}"
        ))
    })
}

fn find_memory_type_index_for_properties(
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    memory_type_bits: u32,
    required_flags: vk::MemoryPropertyFlags,
) -> Option<u32> {
    (0..memory_properties.memory_type_count).find(|index| {
        let allowed = memory_type_bits & (1 << index) != 0;
        let flags = memory_properties.memory_types[*index as usize].property_flags;
        allowed && flags.contains(required_flags)
    })
}

#[cfg(test)]
mod tests {
    use std::fs::File;

    use super::*;

    fn object(size: u32, modifier: u64) -> DmaBufObject {
        let file = File::open("/dev/null").unwrap();
        DmaBufObject { fd: Arc::new(file.into()), size, modifier }
    }

    fn resolution(width: u32, height: u32) -> VideoResolution {
        VideoResolution { width, height }
    }

    fn nv12_layer(y_object: usize, uv_object: usize) -> Nv12DmaBufLayer {
        Nv12DmaBufLayer {
            planes: [
                DmaBufPlane { object_index: y_object, offset: 0, pitch: 64 },
                DmaBufPlane { object_index: uv_object, offset: 0, pitch: 64 },
            ],
        }
    }

    fn validate_layout(
        resolution: VideoResolution,
        objects: impl Into<Box<[DmaBufObject]>>,
        layer: Nv12DmaBufLayer,
    ) -> Result<Nv12DmaBufMemory, DmaBufError> {
        validate_nv12_dmabuf_layout(resolution, objects.into(), layer)
    }

    #[test]
    fn validate_nv12_layout_rejects_odd_dimensions() {
        let objects = [object(4096, 0)];
        let layer = Nv12DmaBufLayer {
            planes: [
                DmaBufPlane { object_index: 0, offset: 0, pitch: 64 },
                DmaBufPlane { object_index: 0, offset: 2048, pitch: 64 },
            ],
        };

        let err = validate_layout(resolution(63, 64), objects, layer)
            .expect_err("odd NV12 dimensions must be rejected");
        assert_invalid_layout(err, "NV12 DMA-BUF dimensions must be even, got 63x64");
    }

    #[test]
    fn validate_nv12_layout_rejects_mixed_modifiers() {
        let objects = [object(4096, 0), object(2048, 1)];
        let layer = nv12_layer(0, 1);

        let err = validate_layout(resolution(64, 64), objects, layer)
            .expect_err("one Vulkan image cannot import multiple modifiers");
        assert_invalid_layout(
            err,
            "NV12 DMA-BUF objects must share a single DRM modifier",
        );
    }

    #[test]
    fn validate_nv12_layout_rejects_unused_objects() {
        let objects = [object(4096, 0), object(4096, 0)];
        let layer = nv12_layer(0, 0);

        let err = validate_layout(resolution(64, 64), objects, layer)
            .expect_err("disjoint import needs unique plane objects");
        assert_invalid_layout(
            err,
            "single-object NV12 DMA-BUF uses object 0, but descriptor has 2 objects",
        );
    }

    #[test]
    fn validate_nv12_layout_canonicalizes_disjoint_plane_objects() {
        let objects = [object(4096, 0), object(4096, 0)];
        let memory =
            validate_layout(resolution(64, 64), objects.clone(), nv12_layer(1, 0))
                .expect("disjoint NV12 planes may arrive in either object order");

        let Nv12DmaBufMemory::Disjoint { objects: canonical_objects, .. } = memory else {
            panic!("expected disjoint NV12 memory");
        };

        assert!(Arc::ptr_eq(&canonical_objects[0].fd, &objects[1].fd));
        assert!(Arc::ptr_eq(&canonical_objects[1].fd, &objects[0].fd));
    }

    fn assert_invalid_layout(err: DmaBufError, expected: &str) {
        let DmaBufError::InvalidLayout(message) = err else {
            panic!("unexpected error: {err:?}");
        };

        assert_eq!(message, expected);
    }
}
