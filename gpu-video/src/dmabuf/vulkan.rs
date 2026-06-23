use ash::vk;
use wgpu::hal::api::Vulkan as VkApi;

use crate::VideoResolution;

use super::{DmaBufError, DmaBufInterop};

pub(crate) unsafe fn wrap_nv12_image_as_wgpu_texture(
    interop: &DmaBufInterop,
    image: vk::Image,
    memories: Vec<vk::DeviceMemory>,
    device: ash::Device,
    resolution: VideoResolution,
    label: &'static str,
    initial_state: wgpu::TextureUses,
) -> Result<wgpu::Texture, DmaBufError> {
    let wgpu_device = &interop.device;
    let hal_device_guard = unsafe {
        wgpu_device.as_hal::<VkApi>().ok_or_else(|| {
            DmaBufError::UnsupportedDevice(
                "NV12 DMA-BUF requires a Vulkan wgpu device".into(),
            )
        })?
    };
    let hal_texture = unsafe {
        let hal_usage = wgpu::TextureUses::RESOURCE
            | wgpu::TextureUses::COPY_DST
            | wgpu::TextureUses::COPY_SRC;

        (*hal_device_guard).texture_from_raw(
            image,
            &wgpu::hal::TextureDescriptor {
                label: Some(label),
                size: resolution.extent_2d(),
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
                for memory in memories {
                    device.free_memory(memory, None);
                }
            })),
            wgpu::hal::vulkan::TextureMemory::External,
        )
    };

    let wgpu_usage = wgpu::TextureUsages::TEXTURE_BINDING
        | wgpu::TextureUsages::COPY_DST
        | wgpu::TextureUsages::COPY_SRC;

    Ok(unsafe {
        wgpu_device.create_texture_from_hal::<VkApi>(
            hal_texture,
            &wgpu::TextureDescriptor {
                label: Some(label),
                size: resolution.extent_2d(),
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

pub(crate) unsafe fn image_plane_memory_requirements(
    device: &ash::Device,
    image: vk::Image,
    plane_aspect: vk::ImageAspectFlags,
) -> vk::MemoryRequirements {
    let mut plane_info =
        vk::ImagePlaneMemoryRequirementsInfo::default().plane_aspect(plane_aspect);
    let info = vk::ImageMemoryRequirementsInfo2::default()
        .image(image)
        .push_next(&mut plane_info);
    let mut requirements = vk::MemoryRequirements2::default();
    unsafe { device.get_image_memory_requirements2(&info, &mut requirements) };
    requirements.memory_requirements
}
