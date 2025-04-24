use std::sync::Arc;

use ash::vk;

use crate::{wrappers::{Image, Semaphore}, VulkanCtxError, VulkanDevice};

use super::H264EncodeProfileInfo;

#[derive(Debug, thiserror::Error)]
pub enum YuvConverterError {
    #[error(transparent)]
    VulkanCtxError(#[from] VulkanCtxError),
}

pub(crate) struct Converter {
    device: Arc<VulkanDevice>,
}

impl Converter {
    pub(crate) fn new(device: Arc<VulkanDevice>) -> Result<Self, YuvConverterError> {
        Ok(Self { device })
    }

    /// The returned image is NV12 with encoding layout
    pub(crate) fn convert(&self, texture: wgpu::Texture, signal_semaphores: &[&Semaphore], profile: &H264EncodeProfileInfo) -> Result<Image, YuvConverterError> {
        let extent = vk::Extent3D {
            width: texture.width(),
            height: texture.height(),
            depth: 1,
        };

        let mut profile_list_info = vk::VideoProfileListInfoKHR::default().profiles(&[profile.profile_info]);

        let queue_indices = [self.device.queues.h264_encode.idx, self.device.queues.wgpu.idx].map(|i| i as u32);

        let create_info = vk::ImageCreateInfo::default()
            .flags(vk::ImageCreateFlags::MUTABLE_FORMAT | vk::ImageCreateFlags::EXTENDED_USAGE)
            .image_type(vk::ImageType::TYPE_2D)
            .format(vk::Format::G8_B8R8_2PLANE_420_UNORM)
            .extent(extent)
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::VIDEO_ENCODE_SRC_KHR)
            .sharing_mode(vk::SharingMode::CONCURRENT)
            .queue_family_indices(&queue_indices)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .push_next(&mut profile_list_info);

        let mut image = Image::new(self.device.allocator.clone(), &create_info)?;

        // transition layout
        // actually, instead of doing this, create this once when initializing. then it's not a
        // problem that we submit and wait for a fence.

        let mut enc = self.device.wgpu_device.create_command_encoder(&Default::default());

        unsafe {
            enc.as_hal_mut::<wgpu::hal::vulkan::Api>(|e| {
                let e = e.unwrap();
                e.transition_textures
            });

            self.device.wgpu_device.as_hal::<wgpu::hal::vulkan::Api>(|d| {
                let d = d.unwrap();

            })
        }
    }
}
