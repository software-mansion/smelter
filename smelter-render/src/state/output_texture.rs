use std::io::Write;

#[cfg(target_os = "linux")]
use std::os::fd::{AsFd, FromRawFd, IntoRawFd, OwnedFd};
#[cfg(target_os = "linux")]
use std::sync::Arc;

#[cfg(target_os = "linux")]
use ash::vk;
use bytes::BufMut;
use crossbeam_channel::bounded;
use tracing::error;
#[cfg(target_os = "linux")]
use tracing::{info, warn};
#[cfg(target_os = "linux")]
use wgpu::hal::api::Vulkan as VkApi;
use wgpu::{Buffer, BufferAsyncError};

#[cfg(target_os = "linux")]
use crate::wgpu::texture::NV12Texture;
#[cfg(target_os = "linux")]
use crate::{
    DRM_FORMAT_NV12, DmaBufFrame, DmaBufLayer, DmaBufObject, DmaBufPlane,
    Nv12DmaBufImportUsage, validate_nv12_dmabuf_layout,
};
use crate::{
    OutputFrameFormat, Resolution,
    wgpu::{
        WgpuCtx,
        texture::{
            PlanarYuvPendingDownload, PlanarYuvTextures, PlanarYuvVariant,
            utils::pad_to_256,
        },
    },
};

pub enum OutputTexture {
    PlanarYuvTextures(Box<PlanarYuvOutput>),
    Rgba8UnormWgpuTexture {
        resolution: Resolution,
    },
    Nv12WgpuTexture {
        resolution: Resolution,
    },
    #[cfg(target_os = "linux")]
    Nv12DmaBuf(Box<Nv12DmaBufOutput>),
}

#[derive(Debug, thiserror::Error)]
pub enum CreateOutputTextureError {
    #[error("NV12 DMA-BUF output frame has fourcc {0}, expected NV12")]
    InvalidDmaBufFourcc(u32),

    #[error("NV12 DMA-BUF output frame has resolution {actual:?}, expected {expected:?}")]
    InvalidDmaBufResolution { actual: Resolution, expected: Resolution },

    #[error("NV12 DMA-BUF output frame has a non-NV12 wgpu texture")]
    InvalidDmaBufTexture,
}

impl OutputTexture {
    pub fn new(
        ctx: &WgpuCtx,
        resolution: Resolution,
        format: OutputFrameFormat,
    ) -> Result<Self, CreateOutputTextureError> {
        match format {
            OutputFrameFormat::PlanarYuv420Bytes => Ok(Self::PlanarYuvTextures(
                Box::new(PlanarYuvOutput::new(ctx, resolution, PlanarYuvVariant::YUV420)),
            )),
            OutputFrameFormat::PlanarYuv422Bytes => Ok(Self::PlanarYuvTextures(
                Box::new(PlanarYuvOutput::new(ctx, resolution, PlanarYuvVariant::YUV422)),
            )),
            OutputFrameFormat::PlanarYuv444Bytes => Ok(Self::PlanarYuvTextures(
                Box::new(PlanarYuvOutput::new(ctx, resolution, PlanarYuvVariant::YUV444)),
            )),
            OutputFrameFormat::RgbaWgpuTexture => {
                Ok(Self::Rgba8UnormWgpuTexture { resolution })
            }
            OutputFrameFormat::Nv12WgpuTexture => {
                Ok(Self::Nv12WgpuTexture { resolution })
            }
            #[cfg(target_os = "linux")]
            OutputFrameFormat::Nv12DmaBuf => {
                info!(?resolution, "creating zero-copy NV12 DMA-BUF output texture");
                Ok(Self::Nv12DmaBuf(Box::new(Nv12DmaBufOutput::new(ctx, resolution)?)))
            }
        }
    }
}

#[cfg(target_os = "linux")]
pub struct Nv12DmaBufOutput {
    device: Arc<wgpu::Device>,
    frames: Vec<PooledNv12DmaBufFrame>,
    next_index: usize,
    resolution: Resolution,
}

#[cfg(target_os = "linux")]
struct PooledNv12DmaBufFrame {
    dmabuf: Arc<DmaBufFrame>,
    texture: NV12Texture,
}

#[cfg(target_os = "linux")]
impl Nv12DmaBufOutput {
    const POOL_SIZE: usize = 16;

    fn new(
        ctx: &WgpuCtx,
        resolution: Resolution,
    ) -> Result<Self, CreateOutputTextureError> {
        let dmabufs = (0..Self::POOL_SIZE)
            .map(|_| export_nv12_dmabuf_texture(&ctx.device, resolution))
            .collect::<Vec<_>>();
        let frames = dmabufs
            .into_iter()
            .map(|dmabuf| {
                validate_nv12_dmabuf(&dmabuf, resolution)?;
                let texture = NV12Texture::from_wgpu_texture(dmabuf.texture_arc())
                    .map_err(|_| CreateOutputTextureError::InvalidDmaBufTexture)?;
                Ok(PooledNv12DmaBufFrame { dmabuf, texture })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { device: Arc::clone(&ctx.device), frames, next_index: 0, resolution })
    }

    pub fn resolution(&self) -> Resolution {
        self.resolution
    }

    pub fn next_frame(&mut self) -> (&NV12Texture, Arc<DmaBufFrame>) {
        let index = self.next_available_frame_index();
        let frame = &self.frames[index];
        (&frame.texture, Arc::clone(&frame.dmabuf))
    }

    fn next_available_frame_index(&mut self) -> usize {
        for _ in 0..self.frames.len() {
            let index = self.next_index;
            self.next_index = (self.next_index + 1) % self.frames.len();
            if Arc::strong_count(&self.frames[index].dmabuf) == 1 {
                return index;
            }
        }

        self.grow_pool();
        self.frames.len() - 1
    }

    fn grow_pool(&mut self) {
        let dmabuf = export_nv12_dmabuf_texture(&self.device, self.resolution);
        validate_nv12_dmabuf(&dmabuf, self.resolution)
            .expect("exported NV12 DMA-BUF output frame is invalid");
        let texture = NV12Texture::from_wgpu_texture(dmabuf.texture_arc())
            .expect("exported NV12 DMA-BUF output frame has invalid wgpu texture");
        self.frames.push(PooledNv12DmaBufFrame { dmabuf, texture });
        warn!(
            pool_size = self.frames.len(),
            resolution = ?self.resolution,
            "grew zero-copy NV12 DMA-BUF output pool because every frame is still in flight"
        );
    }
}

#[cfg(target_os = "linux")]
fn validate_nv12_dmabuf(
    dmabuf: &DmaBufFrame,
    expected: Resolution,
) -> Result<(), CreateOutputTextureError> {
    if dmabuf.fourcc() != DRM_FORMAT_NV12 {
        return Err(CreateOutputTextureError::InvalidDmaBufFourcc(dmabuf.fourcc()));
    }
    if dmabuf.resolution() != expected {
        return Err(CreateOutputTextureError::InvalidDmaBufResolution {
            actual: dmabuf.resolution(),
            expected,
        });
    }
    Ok(())
}

#[cfg(target_os = "linux")]
pub fn export_nv12_dmabuf_texture(
    wgpu_device: &wgpu::Device,
    resolution: Resolution,
) -> Arc<DmaBufFrame> {
    unsafe {
        let hal_device_guard = wgpu_device
            .as_hal::<VkApi>()
            .expect("NV12 DMA-BUF output requires a Vulkan wgpu device");
        let hal_device = &*hal_device_guard;
        let vk_device = hal_device.raw_device().clone();
        let instance = hal_device.shared_instance().raw_instance();
        let physical_device = hal_device.raw_physical_device();
        let size = vk::Extent3D {
            width: resolution.width as u32,
            height: resolution.height as u32,
            depth: 1,
        };

        let modifier = select_nv12_modifier(instance, physical_device);
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

        let image = vk_device
            .create_image(&create_info, None)
            .expect("failed to create exportable NV12 Vulkan image");
        let mem_requirements = vk_device.get_image_memory_requirements(image);
        let memory_type_index =
            find_memory_type_index(instance, physical_device, &mem_requirements);
        let mut export_info = vk::ExportMemoryAllocateInfo::default()
            .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
        let mut dedicated_info = vk::MemoryDedicatedAllocateInfo::default().image(image);
        let allocate_info = vk::MemoryAllocateInfo::default()
            .allocation_size(mem_requirements.size)
            .memory_type_index(memory_type_index)
            .push_next(&mut export_info)
            .push_next(&mut dedicated_info);
        let memory = vk_device
            .allocate_memory(&allocate_info, None)
            .expect("failed to allocate exportable NV12 Vulkan memory");
        vk_device
            .bind_image_memory(image, memory, 0)
            .expect("failed to bind exportable NV12 Vulkan memory");

        let external_memory_fd =
            ash::khr::external_memory_fd::Device::new(instance, &vk_device);
        let fd_info = vk::MemoryGetFdInfoKHR::default()
            .memory(memory)
            .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
        let fd = Arc::new(OwnedFd::from_raw_fd(
            external_memory_fd
                .get_memory_fd(&fd_info)
                .expect("failed to export NV12 DMA-BUF fd"),
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
        ));

        Arc::new(DmaBufFrame::new_with_owner(
            texture,
            DRM_FORMAT_NV12,
            resolution.width as u32,
            resolution.height as u32,
            vec![DmaBufObject {
                fd,
                size: mem_requirements
                    .size
                    .try_into()
                    .expect("DMA-BUF allocation is larger than VA-API can describe"),
                modifier,
            }],
            vec![DmaBufLayer {
                drm_format: DRM_FORMAT_NV12,
                planes: vec![
                    DmaBufPlane {
                        object_index: 0,
                        offset: plane0
                            .offset
                            .try_into()
                            .expect("NV12 Y offset does not fit u32"),
                        pitch: plane0
                            .row_pitch
                            .try_into()
                            .expect("NV12 Y pitch does not fit u32"),
                    },
                    DmaBufPlane {
                        object_index: 0,
                        offset: plane1
                            .offset
                            .try_into()
                            .expect("NV12 UV offset does not fit u32"),
                        pitch: plane1
                            .row_pitch
                            .try_into()
                            .expect("NV12 UV pitch does not fit u32"),
                    },
                ],
            }],
            None,
        ))
    }
}

#[cfg(target_os = "linux")]
pub fn import_nv12_dmabuf_texture(
    wgpu_device: &wgpu::Device,
    fourcc: u32,
    width: u32,
    height: u32,
    objects: Vec<DmaBufObject>,
    layers: Vec<DmaBufLayer>,
    owner: Option<Arc<dyn Send + Sync>>,
    import_usage: Nv12DmaBufImportUsage,
) -> Result<Arc<DmaBufFrame>, String> {
    validate_nv12_dmabuf_layout(fourcc, width, height, &objects, &layers)?;
    if objects.len() != 1 {
        return Err(format!(
            "WGPU NV12 DMA-BUF import supports one object, got {}",
            objects.len()
        ));
    }

    unsafe {
        let hal_device_guard = wgpu_device
            .as_hal::<VkApi>()
            .expect("NV12 DMA-BUF import requires a Vulkan wgpu device");
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
            import_usage.required_format_features(),
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
            .usage(import_usage.image_usage())
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .push_next(&mut external_info)
            .push_next(&mut drm_info);

        let image = vk_device.create_image(&create_info, None).map_err(|err| {
            format!("failed to create imported NV12 Vulkan image: {err}")
        })?;
        let mem_requirements = vk_device.get_image_memory_requirements(image);
        let memory_type_index =
            find_memory_type_index(instance, physical_device, &mem_requirements);
        let import_fd = objects[0]
            .fd
            .as_fd()
            .try_clone_to_owned()
            .map_err(|err| format!("failed to duplicate DMA-BUF fd: {err}"))?
            .into_raw_fd();
        let mut import_info = vk::ImportMemoryFdInfoKHR::default()
            .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT)
            .fd(import_fd);
        let mut dedicated_info = vk::MemoryDedicatedAllocateInfo::default().image(image);
        let allocate_info = vk::MemoryAllocateInfo::default()
            .allocation_size(mem_requirements.size)
            .memory_type_index(memory_type_index)
            .push_next(&mut import_info)
            .push_next(&mut dedicated_info);
        let memory = match vk_device.allocate_memory(&allocate_info, None) {
            Ok(memory) => memory,
            Err(err) => {
                vk_device.destroy_image(image, None);
                return Err(format!("failed to import NV12 DMA-BUF memory: {err}"));
            }
        };
        if let Err(err) = vk_device.bind_image_memory(image, memory, 0) {
            vk_device.destroy_image(image, None);
            vk_device.free_memory(memory, None);
            return Err(format!("failed to bind imported NV12 DMA-BUF memory: {err}"));
        }

        let texture = Arc::new(wrap_nv12_image_as_wgpu_texture(
            wgpu_device,
            image,
            memory,
            vk_device,
            Resolution { width: width as usize, height: height as usize },
            "imported nv12 dma-buf texture",
            import_usage.render_attachment(),
        ));

        Ok(Arc::new(DmaBufFrame::new_with_owner(
            texture, fourcc, width, height, objects, layers, owner,
        )))
    }
}

#[cfg(target_os = "linux")]
#[derive(Clone, Copy)]
pub enum Nv12DmaBufImportUsage {
    Sampled,
    RenderAttachment,
}

#[cfg(target_os = "linux")]
impl Nv12DmaBufImportUsage {
    fn image_usage(self) -> vk::ImageUsageFlags {
        let usage = vk::ImageUsageFlags::SAMPLED
            | vk::ImageUsageFlags::TRANSFER_DST
            | vk::ImageUsageFlags::TRANSFER_SRC;
        match self {
            Self::Sampled => usage,
            Self::RenderAttachment => usage | vk::ImageUsageFlags::COLOR_ATTACHMENT,
        }
    }

    fn render_attachment(self) -> bool {
        matches!(self, Self::RenderAttachment)
    }

    fn required_format_features(self) -> vk::FormatFeatureFlags2 {
        match self {
            Self::Sampled | Self::RenderAttachment => {
                vk::FormatFeatureFlags2::SAMPLED_IMAGE
                    | vk::FormatFeatureFlags2::TRANSFER_DST
                    | vk::FormatFeatureFlags2::TRANSFER_SRC
            }
        }
    }
}

#[cfg(target_os = "linux")]
unsafe fn wrap_nv12_image_as_wgpu_texture(
    wgpu_device: &wgpu::Device,
    image: vk::Image,
    memory: vk::DeviceMemory,
    device: ash::Device,
    resolution: Resolution,
    label: &'static str,
    render_attachment: bool,
) -> wgpu::Texture {
    let hal_device_guard = unsafe {
        wgpu_device.as_hal::<VkApi>().expect("NV12 DMA-BUF requires a Vulkan wgpu device")
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
                    width: resolution.width as u32,
                    height: resolution.height as u32,
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

    unsafe {
        let mut wgpu_usage = wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC;
        if render_attachment {
            wgpu_usage |= wgpu::TextureUsages::RENDER_ATTACHMENT;
        }

        wgpu_device.create_texture_from_hal::<VkApi>(
            hal_texture,
            &wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: resolution.width as u32,
                    height: resolution.height as u32,
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
        )
    }
}

#[cfg(target_os = "linux")]
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

#[cfg(target_os = "linux")]
fn select_nv12_modifier(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
) -> u64 {
    let required = Nv12DmaBufImportUsage::RenderAttachment.required_format_features();
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
        .expect("no exportable NV12 DRM modifier with transfer support available")
        .drm_format_modifier
}

#[cfg(target_os = "linux")]
fn validate_nv12_modifier_support(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    modifier: u64,
    required: vk::FormatFeatureFlags2,
) -> Result<(), String> {
    let modifiers = nv12_modifier_properties(instance, physical_device);
    let Some(properties) =
        modifiers.iter().find(|properties| properties.drm_format_modifier == modifier)
    else {
        return Err(format!(
            "NV12 DMA-BUF modifier {modifier:#x} is not supported by the WGPU Vulkan device; available modifiers: {}",
            format_nv12_modifiers(&modifiers)
        ));
    };

    if supports_nv12_modifier(properties, required) {
        return Ok(());
    }

    Err(format!(
        "NV12 DMA-BUF modifier {modifier:#x} has {:?} with {} planes, but import requires {required:?}",
        properties.drm_format_modifier_tiling_features,
        properties.drm_format_modifier_plane_count,
    ))
}

#[cfg(target_os = "linux")]
fn supports_nv12_modifier(
    modifier: &vk::DrmFormatModifierProperties2EXT,
    required: vk::FormatFeatureFlags2,
) -> bool {
    modifier.drm_format_modifier_plane_count == 2
        && modifier.drm_format_modifier_tiling_features.contains(required)
}

#[cfg(target_os = "linux")]
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

#[cfg(target_os = "linux")]
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

#[cfg(target_os = "linux")]
fn find_memory_type_index(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    mem_requirements: &vk::MemoryRequirements,
) -> u32 {
    let memory_properties =
        unsafe { instance.get_physical_device_memory_properties(physical_device) };

    (0..memory_properties.memory_type_count)
        .find(|index| {
            let allowed = mem_requirements.memory_type_bits & (1 << index) != 0;
            let flags = memory_properties.memory_types[*index as usize].property_flags;
            allowed && flags.contains(vk::MemoryPropertyFlags::DEVICE_LOCAL)
        })
        .expect("no device-local memory type available for exportable NV12 image")
}

pub struct PlanarYuvOutput {
    textures: PlanarYuvTextures,
    buffers: [wgpu::Buffer; 3],
    resolution: Resolution,
}

impl PlanarYuvOutput {
    pub fn new(
        ctx: &WgpuCtx,
        resolution: Resolution,
        pixel_format: PlanarYuvVariant,
    ) -> Self {
        let textures = PlanarYuvTextures::new(ctx, resolution, pixel_format);
        let buffers = textures.new_download_buffers(ctx);

        Self { textures, buffers, resolution }
    }

    pub fn yuv_textures(&self) -> &PlanarYuvTextures {
        &self.textures
    }

    pub fn resolution(&self) -> Resolution {
        self.resolution
    }

    pub fn start_download<'a>(
        &'a self,
        ctx: &WgpuCtx,
    ) -> PlanarYuvPendingDownload<
        'a,
        impl FnOnce() -> Result<bytes::Bytes, BufferAsyncError> + 'a,
        BufferAsyncError,
    > {
        self.textures.copy_to_buffers(ctx, &self.buffers);

        PlanarYuvPendingDownload::new(
            self.download_buffer(self.textures.plane_texture(0).size(), &self.buffers[0]),
            self.download_buffer(self.textures.plane_texture(1).size(), &self.buffers[1]),
            self.download_buffer(self.textures.plane_texture(2).size(), &self.buffers[2]),
        )
    }

    fn download_buffer<'a>(
        &'a self,
        size: wgpu::Extent3d,
        source: &'a Buffer,
    ) -> impl FnOnce() -> Result<bytes::Bytes, BufferAsyncError> + 'a {
        let buffer = bytes::BytesMut::with_capacity((size.width * size.height) as usize);
        let (s, r) = bounded(1);
        source.slice(..).map_async(wgpu::MapMode::Read, move |result| {
            if let Err(err) = s.send(result) {
                error!("channel send error: {err}")
            }
        });

        move || {
            r.recv().unwrap()?;
            let mut buffer = buffer.writer();
            {
                let range = source.slice(..).get_mapped_range().unwrap();
                let chunks = range.chunks(pad_to_256(size.width) as usize);
                for chunk in chunks {
                    buffer.write_all(&chunk[..size.width as usize]).unwrap();
                }
            };
            source.unmap();
            Ok(buffer.into_inner().into())
        }
    }
}
