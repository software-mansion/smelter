use std::sync::Arc;

use ash::vk;
use vk_mem::Alloc;

use crate::vulkan_decoder::{H264ProfileInfo, VulkanCtxError, VulkanDecoderError};

use super::{Device, Instance};

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
    ) -> Result<Self, VulkanDecoderError> {
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

pub(crate) struct DecodeInputBuffer {
    pub(crate) buffer: Buffer,
    capacity: u64,
    allocator: Arc<Allocator>,
}

impl DecodeInputBuffer {
    pub(crate) fn new(
        allocator: Arc<Allocator>,
        profile: &H264ProfileInfo,
    ) -> Result<Self, VulkanDecoderError> {
        const INITIAL_SIZE: u64 = 1024 * 1024; // 1MiB
        let buffer = Buffer::new_decode(allocator.clone(), INITIAL_SIZE, profile)?;

        Ok(Self {
            buffer,
            capacity: INITIAL_SIZE,
            allocator,
        })
    }

    /// size must be passed in here for alignment reasons
    pub(crate) fn upload_data(
        &mut self,
        data: &[u8],
        size: u64,
        profile: &H264ProfileInfo,
    ) -> Result<(), VulkanDecoderError> {
        debug_assert!(data.len() as u64 <= size);

        if self.capacity < size {
            let new_capacity = size.max(2 * self.capacity);
            self.buffer = Buffer::new_decode(self.allocator.clone(), new_capacity, profile)?;
        }

        unsafe {
            let mem = self.allocator.map_memory(&mut self.buffer.allocation)?;
            let slice = std::slice::from_raw_parts_mut(mem.cast(), data.len());
            slice.copy_from_slice(data);
            self.allocator.unmap_memory(&mut self.buffer.allocation);
        }

        Ok(())
    }
}

pub(crate) struct Buffer {
    pub(crate) buffer: vk::Buffer,
    pub(crate) allocation: vk_mem::Allocation,
    allocator: Arc<Allocator>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum TransferDirection {
    GpuToMem,
}

impl Buffer {
    pub(crate) fn new_decode(
        allocator: Arc<Allocator>,
        size: u64,
        profile: &H264ProfileInfo,
    ) -> Result<Self, VulkanDecoderError> {
        let mut profile_list_info = vk::VideoProfileListInfoKHR::default()
            .profiles(std::slice::from_ref(&profile.profile_info));

        let buffer_create_info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(vk::BufferUsageFlags::VIDEO_DECODE_SRC_KHR)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .push_next(&mut profile_list_info);

        let allocation_create_info = vk_mem::AllocationCreateInfo {
            usage: vk_mem::MemoryUsage::Auto,
            required_flags: vk::MemoryPropertyFlags::HOST_COHERENT,
            flags: vk_mem::AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE,
            ..Default::default()
        };

        Self::new(allocator, buffer_create_info, allocation_create_info)
    }

    pub(crate) fn new_transfer(
        allocator: Arc<Allocator>,
        size: u64,
        direction: TransferDirection,
    ) -> Result<Self, VulkanDecoderError> {
        let usage = match direction {
            TransferDirection::GpuToMem => vk::BufferUsageFlags::TRANSFER_DST,
        };

        let allocation_flags = match direction {
            TransferDirection::GpuToMem => vk_mem::AllocationCreateFlags::HOST_ACCESS_RANDOM,
        };

        let buffer_create_info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let allocation_create_info = vk_mem::AllocationCreateInfo {
            usage: vk_mem::MemoryUsage::Auto,
            required_flags: vk::MemoryPropertyFlags::HOST_COHERENT,
            flags: allocation_flags,
            ..Default::default()
        };

        Self::new(allocator, buffer_create_info, allocation_create_info)
    }

    fn new(
        allocator: Arc<Allocator>,
        create_info: vk::BufferCreateInfo,
        allocation_create_info: vk_mem::AllocationCreateInfo,
    ) -> Result<Self, VulkanDecoderError> {
        let (buffer, allocation) =
            unsafe { allocator.create_buffer(&create_info, &allocation_create_info)? };

        Ok(Self {
            buffer,
            allocation,
            allocator,
        })
    }

    /// ## Safety
    /// the buffer has to be mappable and readable
    pub(crate) unsafe fn download_data_from_buffer(
        &mut self,
        size: usize,
    ) -> Result<Vec<u8>, VulkanDecoderError> {
        let output;
        unsafe {
            let memory = self.allocator.map_memory(&mut self.allocation)?;
            let memory_slice = std::slice::from_raw_parts_mut(memory, size);
            output = memory_slice.to_vec();
            self.allocator.unmap_memory(&mut self.allocation);
        }

        Ok(output)
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
    ) -> Result<Self, VulkanDecoderError> {
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
    ) -> Result<Self, VulkanDecoderError> {
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
