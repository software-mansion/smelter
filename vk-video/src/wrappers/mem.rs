use std::sync::Arc;

use ash::vk;
use vk_mem::Alloc;

use crate::VulkanCtxError;

use super::{Device, H264ProfileInfo, Instance};

pub(crate) struct Allocator {
    allocator: vk_mem::Allocator,
    _instance: Arc<Instance>,
    _device: Arc<Device>,
}

impl Allocator {
    pub(crate) fn new(
        instance: Arc<Instance>,
        physical_device: vk::PhysicalDevice,
        device: Arc<Device>,
    ) -> Result<Self, VulkanCtxError> {
        let mut allocator_create_info =
            vk_mem::AllocatorCreateInfo::new(&instance, &device, physical_device);
        allocator_create_info.vulkan_api_version = vk::API_VERSION_1_3;

        let allocator = unsafe { vk_mem::Allocator::new(allocator_create_info)? };

        Ok(Self {
            allocator,
            _device: device,
            _instance: instance,
        })
    }
}

impl std::ops::Deref for Allocator {
    type Target = vk_mem::Allocator;

    fn deref(&self) -> &Self::Target {
        &self.allocator
    }
}

pub(crate) struct MemoryAllocation {
    pub(crate) allocation: vk_mem::Allocation,
    allocator: Arc<Allocator>,
}

impl MemoryAllocation {
    pub(crate) fn new(
        allocator: Arc<Allocator>,
        memory_requirements: &vk::MemoryRequirements,
        alloc_info: &vk_mem::AllocationCreateInfo,
    ) -> Result<Self, VulkanCtxError> {
        let allocation = unsafe { allocator.allocate_memory(memory_requirements, alloc_info)? };

        Ok(Self {
            allocation,
            allocator,
        })
    }

    pub(crate) fn allocation_info(&self) -> vk_mem::AllocationInfo {
        self.allocator.get_allocation_info(&self.allocation)
    }
}

impl std::ops::Deref for MemoryAllocation {
    type Target = vk_mem::Allocation;

    fn deref(&self) -> &Self::Target {
        &self.allocation
    }
}

impl Drop for MemoryAllocation {
    fn drop(&mut self) {
        unsafe { self.allocator.free_memory(&mut self.allocation) };
    }
}

pub(crate) struct Buffer {
    pub(crate) buffer: vk::Buffer,
    pub(crate) allocation: vk_mem::Allocation,
    allocator: Arc<Allocator>,
    transfer_direction: TransferDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TransferDirection {
    GpuToMem,
    MemToGpu,
}

impl Buffer {
    pub(crate) fn new_decode(
        allocator: Arc<Allocator>,
        size: u64,
        profile: &H264ProfileInfo,
    ) -> Result<Self, VulkanCtxError> {
        let mut profile_list_info = vk::VideoProfileListInfoKHR::default()
            .profiles(std::slice::from_ref(&profile.profile_info));

        let buffer_create_info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(vk::BufferUsageFlags::VIDEO_DECODE_SRC_KHR)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .push_next(&mut profile_list_info);

        Self::new(allocator, buffer_create_info, TransferDirection::MemToGpu)
    }

    pub(crate) fn new_transfer(
        allocator: Arc<Allocator>,
        size: u64,
        direction: TransferDirection,
    ) -> Result<Self, VulkanCtxError> {
        let usage = match direction {
            TransferDirection::GpuToMem => vk::BufferUsageFlags::TRANSFER_DST,
            TransferDirection::MemToGpu => vk::BufferUsageFlags::TRANSFER_SRC,
        };

        let buffer_create_info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        Self::new(allocator, buffer_create_info, direction)
    }

    pub(crate) fn new_transfer_with_data(
        allocator: Arc<Allocator>,
        data: &[u8],
    ) -> Result<Self, VulkanCtxError> {
        let mut result =
            Self::new_transfer(allocator, data.len() as u64, TransferDirection::MemToGpu)?;
        result.copy_data_into(data)?;

        Ok(result)
    }

    fn new(
        allocator: Arc<Allocator>,
        create_info: vk::BufferCreateInfo,
        transfer_direction: TransferDirection,
    ) -> Result<Self, VulkanCtxError> {
        let allocation_flags = match transfer_direction {
            TransferDirection::GpuToMem => vk_mem::AllocationCreateFlags::HOST_ACCESS_RANDOM,
            TransferDirection::MemToGpu => {
                vk_mem::AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE
            }
        };

        let allocation_create_info = vk_mem::AllocationCreateInfo {
            usage: vk_mem::MemoryUsage::Auto,
            required_flags: vk::MemoryPropertyFlags::HOST_COHERENT,
            flags: allocation_flags,
            ..Default::default()
        };

        let (buffer, allocation) =
            unsafe { allocator.create_buffer(&create_info, &allocation_create_info)? };

        Ok(Self {
            buffer,
            allocation,
            allocator,
            transfer_direction,
        })
    }

    /// ## Safety
    /// the buffer has to be mappable and readable
    pub(crate) unsafe fn download_data_from_buffer(
        &mut self,
        size: usize,
    ) -> Result<Vec<u8>, VulkanCtxError> {
        let mut output = Vec::new();
        unsafe {
            let memory = self.allocator.map_memory(&mut self.allocation)?;
            let memory_slice = std::slice::from_raw_parts_mut(memory, size);
            output.extend_from_slice(memory_slice);
            self.allocator.unmap_memory(&mut self.allocation);
        }

        Ok(output)
    }

    pub(crate) fn new_with_decode_data(
        allocator: Arc<Allocator>,
        data: &[u8],
        buffer_size: u64,
        profile_info: &H264ProfileInfo,
    ) -> Result<Buffer, VulkanCtxError> {
        let mut decode_buffer = Buffer::new_decode(allocator.clone(), buffer_size, profile_info)?;
        decode_buffer.copy_data_into(data)?;

        Ok(decode_buffer)
    }

    fn copy_data_into(&mut self, data: &[u8]) -> Result<(), VulkanCtxError> {
        if self.transfer_direction != TransferDirection::MemToGpu {
            return Err(VulkanCtxError::UploadToImproperBuffer);
        }

        unsafe {
            let mem = self.allocator.map_memory(&mut self.allocation)?;
            let slice = std::slice::from_raw_parts_mut(mem.cast(), data.len());
            slice.copy_from_slice(data);
            self.allocator.unmap_memory(&mut self.allocation);
        }

        Ok(())
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        unsafe {
            self.allocator
                .destroy_buffer(self.buffer, &mut self.allocation)
        }
    }
}

impl std::ops::Deref for Buffer {
    type Target = vk::Buffer;

    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}

pub(crate) struct Image {
    pub(crate) image: vk::Image,
    allocation: vk_mem::Allocation,
    allocator: Arc<Allocator>,
    pub(crate) extent: vk::Extent3D,
}

impl Image {
    pub(crate) fn new(
        allocator: Arc<Allocator>,
        image_create_info: &vk::ImageCreateInfo,
    ) -> Result<Self, VulkanCtxError> {
        let extent = image_create_info.extent;
        let alloc_info = vk_mem::AllocationCreateInfo {
            usage: vk_mem::MemoryUsage::Auto,
            ..Default::default()
        };

        let (image, allocation) =
            unsafe { allocator.create_image(image_create_info, &alloc_info)? };

        Ok(Image {
            image,
            allocation,
            allocator,
            extent,
        })
    }
}

impl std::ops::Deref for Image {
    type Target = vk::Image;

    fn deref(&self) -> &Self::Target {
        &self.image
    }
}

impl Drop for Image {
    fn drop(&mut self) {
        unsafe {
            self.allocator
                .destroy_image(self.image, &mut self.allocation)
        };
    }
}

pub(crate) struct ImageView {
    pub(crate) view: vk::ImageView,
    pub(crate) _image: Arc<Image>,
    pub(crate) device: Arc<Device>,
}

impl ImageView {
    pub(crate) fn new(
        device: Arc<Device>,
        image: Arc<Image>,
        create_info: &vk::ImageViewCreateInfo,
    ) -> Result<Self, VulkanCtxError> {
        let view = unsafe { device.create_image_view(create_info, None)? };

        Ok(ImageView {
            view,
            _image: image,
            device: device.clone(),
        })
    }
}

impl Drop for ImageView {
    fn drop(&mut self) {
        unsafe { self.device.destroy_image_view(self.view, None) };
    }
}
