use std::sync::Arc;

use ash::vk;

use crate::{wrappers::{Buffer, Image}, Frame, RawFrame, VulkanCtxError, VulkanDevice};

#[derive(Debug, thiserror::Error)]
pub enum VulkanEncoderError {
    #[error("Vulkan error: {0}")]
    VkError(#[from] ash::vk::Result),

    #[error("Cannot find enough memory of the right type on the deivce")]
    NoMemory,

    #[error("The supplied textures format is {0:?}, when it should be NV12")]
    NotNV12Texture(wgpu::TextureFormat),

    #[error(transparent)]
    VulkanCtxError(#[from] VulkanCtxError),
}

pub(crate) struct VulkanEncoder {
    device: Arc<VulkanDevice>,
}

impl VulkanEncoder {
    pub fn new(device: Arc<VulkanDevice>) -> Result<Self, VulkanEncoderError> {
        Ok(Self { device })
    }

    pub fn encode_bytes(&mut self, frame: Frame<RawFrame>) -> Result<Vec<u8>, VulkanEncoderError> {
        let extent = vk::Extent3D {
            width: frame.frame.width,
            height: frame.frame.height,
            depth: 1,
        };

        let image_create_info = vk::ImageCreateInfo::default()
            .flags(vk::ImageCreateFlags::empty())
            .image_type(vk::ImageType::TYPE_2D)
            .format(vk::Format::G8_B8R8_2PLANE_420_UNORM)
            .extent(extent)
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::VIDEO_ENCODE_SRC_KHR | vk::ImageUsageFlags::TRANSFER_DST)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let bytes = Image::new(self.device.allocator.clone(), &image_create_info);
        todo!();
    }
}
