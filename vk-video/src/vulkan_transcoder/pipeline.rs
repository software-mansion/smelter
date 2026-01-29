use std::sync::Arc;

use ash::vk::{self, DescriptorImageInfo};

use crate::{
    VulkanDevice,
    vulkan_encoder::{EncoderTracker, H264EncodeProfileInfo},
    vulkan_transcoder::TranscoderError,
    wrappers::{DescriptorPool, DescriptorSetLayout, Image, ImageView},
};

const MAX_OUTPUTS: usize = 8;

pub(crate) struct OutputConfig<'a> {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) tracker: &'a mut EncoderTracker,
    pub(crate) profile: &'a H264EncodeProfileInfo<'a>,
}

pub(crate) struct Pipeline {
    images: Vec<Arc<Image>>,
}

impl Pipeline {
    pub(crate) fn new(
        device: Arc<VulkanDevice>,
        configs: &[OutputConfig<'_>],
    ) -> Result<Self, TranscoderError> {
        let images = configs
            .iter()
            .map(|c| {
                let mut profile_list_info = vk::VideoProfileListInfoKHR::default()
                    .profiles(std::slice::from_ref(&c.profile.profile_info));
                let queue_indices = [
                    device.queues.h264_encode.as_ref().unwrap().idx as u32,
                    device.queues.wgpu.idx as u32,
                ];
                let create_info = vk::ImageCreateInfo::default()
                    .flags(
                        vk::ImageCreateFlags::EXTENDED_USAGE | vk::ImageCreateFlags::MUTABLE_FORMAT,
                    )
                    .image_type(vk::ImageType::TYPE_2D)
                    .format(vk::Format::G8_B8R8_2PLANE_420_UNORM)
                    .extent(vk::Extent3D {
                        width: c.width,
                        height: c.height,
                        depth: 1,
                    })
                    .mip_levels(1)
                    .array_layers(1)
                    .samples(vk::SampleCountFlags::TYPE_1)
                    .tiling(vk::ImageTiling::OPTIMAL)
                    .usage(vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::VIDEO_ENCODE_SRC_KHR)
                    .sharing_mode(vk::SharingMode::CONCURRENT)
                    .queue_family_indices(&queue_indices)
                    .initial_layout(vk::ImageLayout::UNDEFINED)
                    .push_next(&mut profile_list_info);

                Image::new(
                    device.allocator.clone(),
                    &create_info,
                    c.tracker.image_layout_tracker.clone(),
                )
                .map(Arc::new)
            })
            .collect::<Result<Vec<_>, _>>()?;

        let views_y = images
            .iter()
            .map(|i| {
                i.create_plane_view(vk::ImageAspectFlags::PLANE_0, vk::ImageUsageFlags::STORAGE)
            })
            .collect::<Result<Vec<_>, _>>()?;

        let views_uv = images
            .iter()
            .map(|i| {
                i.create_plane_view(vk::ImageAspectFlags::PLANE_1, vk::ImageUsageFlags::STORAGE)
            })
            .collect::<Result<Vec<_>, _>>()?;

        let pool_sizes = [vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::STORAGE_IMAGE)
            .descriptor_count(2 * MAX_OUTPUTS as u32)];
        let descriptor_pool = DescriptorPool::new(
            device.device.clone(),
            &vk::DescriptorPoolCreateInfo::default()
                .max_sets(1)
                .pool_sizes(&pool_sizes),
        )?;

        let binding = vk::DescriptorSetLayoutBinding::default()
            .descriptor_count(2 * MAX_OUTPUTS as u32)
            .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
            .binding(0)
            .stage_flags(vk::ShaderStageFlags::COMPUTE);

        let layout = DescriptorSetLayout::new(
            device.device.clone(),
            &vk::DescriptorSetLayoutCreateInfo::default().bindings(std::slice::from_ref(&binding)),
        )?;

        let descriptor_set = unsafe {
            device.device.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(descriptor_pool.pool)
                    .set_layouts(&[layout.set_layout]),
            )
        }?[0];

        let image_infos = views_y
            .iter()
            .zip(views_uv.iter())
            .flat_map(|(y, uv)| [y, uv])
            .map(|view| vk::DescriptorImageInfo {
                sampler: vk::Sampler::null(),
                image_view: view.view,
                image_layout: vk::ImageLayout::GENERAL,
            })
            .collect::<Vec<_>>();

        let write = vk::WriteDescriptorSet::default()
            .dst_set(descriptor_set)
            .dst_binding(0)
            .dst_array_element(0)
            .descriptor_count(image_infos.len() as u32)
            .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
            .image_info(&image_infos);

        unsafe { device.device.update_descriptor_sets(&[write], &[]) };

        Ok(Self { images })
    }
}
