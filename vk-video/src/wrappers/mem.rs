use std::sync::{Arc, Mutex};

use ash::vk;
use vk_mem::Alloc;

use crate::{
    VulkanCommonError, VulkanDecoderError, VulkanInitError, vulkan_encoder::H264EncodeProfileInfo,
};

use super::{Device, H264DecodeProfileInfo, Instance};

pub struct Allocator {
    allocator: Arc<vk_mem::Allocator>,
    _instance: Arc<Instance>,
    pub device: Arc<Device>,
}

impl Allocator {
    pub fn new(
        instance: Arc<Instance>,
        physical_device: vk::PhysicalDevice,
        device: Arc<Device>,
    ) -> Result<Self, VulkanInitError> {
        let mut allocator_create_info =
            vk_mem::AllocatorCreateInfo::new(&instance, &device, physical_device);
        allocator_create_info.vulkan_api_version = vk::API_VERSION_1_3;

        let allocator = unsafe { vk_mem::Allocator::new(allocator_create_info)? };

        Ok(Self {
            allocator: Arc::new(allocator),
            device,
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

pub struct MemoryAllocation {
    pub allocation: vk_mem::Allocation,
    allocator: Arc<Allocator>,
}

impl MemoryAllocation {
    pub fn new(
        allocator: Arc<Allocator>,
        memory_requirements: &vk::MemoryRequirements,
        alloc_info: &vk_mem::AllocationCreateInfo,
    ) -> Result<Self, VulkanCommonError> {
        let allocation = unsafe { allocator.allocate_memory(memory_requirements, alloc_info)? };

        Ok(Self {
            allocation,
            allocator,
        })
    }

    pub fn allocation_info(&self) -> vk_mem::AllocationInfo {
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

pub struct DecodeInputBuffer {
    pub buffer: Buffer,
    capacity: u64,
    allocator: Arc<Allocator>,
}

impl DecodeInputBuffer {
    pub fn new(
        allocator: Arc<Allocator>,
        profile: &H264DecodeProfileInfo,
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
    pub fn upload_data(
        &mut self,
        data: &[u8],
        size: u64,
        profile: &H264DecodeProfileInfo,
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

pub struct Buffer {
    pub buffer: vk::Buffer,
    pub allocation: vk_mem::Allocation,
    allocator: Arc<Allocator>,
    transfer_direction: TransferDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferDirection {
    GpuToMem,
    MemToGpu,
}

impl Buffer {
    pub fn new_decode(
        allocator: Arc<Allocator>,
        size: u64,
        profile: &H264DecodeProfileInfo,
    ) -> Result<Self, VulkanCommonError> {
        let mut profile_list_info = vk::VideoProfileListInfoKHR::default()
            .profiles(std::slice::from_ref(&profile.profile_info));

        let buffer_create_info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(vk::BufferUsageFlags::VIDEO_DECODE_SRC_KHR)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .push_next(&mut profile_list_info);

        Self::new(allocator, buffer_create_info, TransferDirection::MemToGpu)
    }

    pub fn new_encode(
        allocator: Arc<Allocator>,
        size: u64,
        profile: &H264EncodeProfileInfo,
    ) -> Result<Self, VulkanCommonError> {
        let mut profile_list_info = vk::VideoProfileListInfoKHR::default()
            .profiles(std::slice::from_ref(&profile.profile_info));

        let buffer_create_info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(vk::BufferUsageFlags::VIDEO_ENCODE_DST_KHR)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .push_next(&mut profile_list_info);

        Self::new(allocator, buffer_create_info, TransferDirection::GpuToMem)
    }

    pub fn new_transfer(
        allocator: Arc<Allocator>,
        size: u64,
        direction: TransferDirection,
    ) -> Result<Self, VulkanCommonError> {
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

    pub fn new_transfer_with_data(
        allocator: Arc<Allocator>,
        data: &[u8],
    ) -> Result<Self, VulkanCommonError> {
        let mut result =
            Self::new_transfer(allocator, data.len() as u64, TransferDirection::MemToGpu)?;
        result.copy_data_into(data)?;

        Ok(result)
    }

    fn new(
        allocator: Arc<Allocator>,
        create_info: vk::BufferCreateInfo,
        transfer_direction: TransferDirection,
    ) -> Result<Self, VulkanCommonError> {
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
    pub unsafe fn download_data_from_buffer(
        &mut self,
        size: usize,
    ) -> Result<Vec<u8>, VulkanCommonError> {
        let output;
        unsafe {
            let memory = self.allocator.map_memory(&mut self.allocation)?;
            let memory_slice = std::slice::from_raw_parts_mut(memory, size);
            output = memory_slice.to_vec();
            self.allocator.unmap_memory(&mut self.allocation);
        }

        Ok(output)
    }

    fn copy_data_into(&mut self, data: &[u8]) -> Result<(), VulkanCommonError> {
        if self.transfer_direction != TransferDirection::MemToGpu {
            return Err(VulkanCommonError::UploadToImproperBuffer);
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

pub struct Image {
    pub image: vk::Image,
    allocation: vk_mem::Allocation,
    allocator: Arc<Allocator>,
    pool: Option<vk_mem::AllocatorPool>,
    pub device: Arc<Device>,
    pub layout: Box<[vk::ImageLayout]>,
    pub extent: vk::Extent3D,
}

impl Image {
    pub fn new(
        allocator: Arc<Allocator>,
        image_create_info: &vk::ImageCreateInfo,
    ) -> Result<Self, VulkanCommonError> {
        let extent = image_create_info.extent;
        let layout =
            vec![image_create_info.initial_layout; image_create_info.array_layers as usize]
                .into_boxed_slice();
        let alloc_info = vk_mem::AllocationCreateInfo {
            usage: vk_mem::MemoryUsage::Auto,
            ..Default::default()
        };

        let (image, allocation) =
            unsafe { allocator.create_image(image_create_info, &alloc_info)? };

        Ok(Image {
            image,
            allocation,
            pool: None,
            device: allocator.device.clone(),
            allocator,
            layout,
            extent,
        })
    }

    pub fn new_export(
        device: &Device,
        allocator: Arc<Allocator>,
    ) -> Result<Self, VulkanCommonError> {
        // TODO: Requires 1.3
        let mut export_image_info = vk::ExternalMemoryImageCreateInfo::default()
            .handle_types(vk::ExternalMemoryHandleTypeFlags::OPAQUE_FD);
        let image_create_info = vk::ImageCreateInfo::default()
            .push_next(&mut export_image_info)
            .image_type(vk::ImageType::TYPE_2D)
            .format(vk::Format::R8G8B8A8_SRGB)
            .extent(vk::Extent3D {
                width: 1280,
                height: 720,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::LINEAR)
            .usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let alloc_info = vk_mem::AllocationCreateInfo {
            usage: vk_mem::MemoryUsage::Auto,
            required_flags: vk::MemoryPropertyFlags::DEVICE_LOCAL,
            ..Default::default()
        };
        let mem_type = unsafe {
            allocator.find_memory_type_index_for_image_info(image_create_info, &alloc_info)?
        };
        // TODO: Requires 1.3
        let export_alloc_info = vk::ExportMemoryAllocateInfo::default()
            .handle_types(vk::ExternalMemoryHandleTypeFlags::OPAQUE_FD);
        let pool_info = vk_mem::PoolCreateInfo {
            memory_allocate_next: &raw const export_alloc_info as *const _,
            memory_type_index: mem_type,
            ..Default::default()
        };
        let pool = allocator.allocator.create_pool(&pool_info)?;
        let extent = image_create_info.extent;
        let layout =
            vec![image_create_info.initial_layout; image_create_info.array_layers as usize]
                .into_boxed_slice();
        let alloc_info = vk_mem::AllocationCreateInfo {
            flags: vk_mem::AllocationCreateFlags::DEDICATED_MEMORY,
            ..Default::default()
        };

        let (image, allocation) = unsafe { pool.create_image(&image_create_info, &alloc_info)? };

        Ok(Image {
            image,
            allocation,
            device: allocator.device.clone(),
            pool: Some(pool),
            allocator,
            layout,
            extent,
        })
    }

    pub fn transition_layout(
        &mut self,
        command_buffer: vk::CommandBuffer,
        stages: std::ops::Range<vk::PipelineStageFlags2>,
        accesses: std::ops::Range<vk::AccessFlags2>,
        new_layout: vk::ImageLayout,
        subresource_range: vk::ImageSubresourceRange,
    ) -> Result<vk::ImageLayout, VulkanCommonError> {
        let barrier = vk::ImageMemoryBarrier2::default()
            .src_stage_mask(stages.start)
            .dst_stage_mask(stages.end)
            .src_access_mask(accesses.start)
            .dst_access_mask(accesses.end)
            .old_layout(self.layout[subresource_range.base_array_layer as usize])
            .new_layout(new_layout)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(self.image)
            .subresource_range(subresource_range);

        unsafe {
            self.device.cmd_pipeline_barrier2(
                command_buffer,
                &vk::DependencyInfo::default().image_memory_barriers(&[barrier]),
            );
        }

        let old_layout = self.layout[subresource_range.base_array_layer as usize];
        let end = if subresource_range.layer_count == vk::REMAINING_ARRAY_LAYERS {
            self.layout.len()
        } else {
            subresource_range.base_array_layer as usize + subresource_range.layer_count as usize
        };

        for layout in self.layout[subresource_range.base_array_layer as usize..end].iter_mut() {
            *layout = new_layout;
        }

        Ok(old_layout)
    }

    pub fn transition_layout_single_layer(
        &mut self,
        command_buffer: vk::CommandBuffer,
        stages: std::ops::Range<vk::PipelineStageFlags2>,
        accesses: std::ops::Range<vk::AccessFlags2>,
        new_layout: vk::ImageLayout,
        base_array_layer: u32,
    ) -> Result<vk::ImageLayout, VulkanCommonError> {
        self.transition_layout(
            command_buffer,
            stages,
            accesses,
            new_layout,
            vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_array_layer,
                layer_count: 1,
                base_mip_level: 0,
                level_count: 1,
            },
        )
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

pub struct ImageView {
    pub view: vk::ImageView,
    pub _image: Arc<Mutex<Image>>,
    pub device: Arc<Device>,
}

impl ImageView {
    pub fn new(
        device: Arc<Device>,
        image: Arc<Mutex<Image>>,
        create_info: &vk::ImageViewCreateInfo,
    ) -> Result<Self, VulkanCommonError> {
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
