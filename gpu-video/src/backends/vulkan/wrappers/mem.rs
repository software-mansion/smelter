use std::sync::{Arc, Mutex, Weak};

use ash::vk::{self, Handle};
use gpu_allocator::{
    MemoryLocation,
    vulkan::{Allocation, AllocationCreateDesc, AllocationScheme, AllocatorCreateDesc},
};

use crate::backends::vulkan::{
    VulkanCommonError, VulkanDevice, VulkanDeviceInitError,
    codec::h264::parameters::H264DecodeProfileInfo,
    vulkan_decoder::VulkanDecoderError,
    vulkan_device::EncodingDevice,
    vulkan_encoder::VulkanEncoderError,
    wrappers::{ImageLayoutTracker, OpenCommandBuffer, ProfileInfo},
};

use super::{Device, Instance};

pub(crate) struct Allocator {
    allocator: Mutex<gpu_allocator::vulkan::Allocator>,
    device: Arc<Device>,
}

impl Allocator {
    pub(crate) fn new(
        instance: &Instance,
        physical_device: vk::PhysicalDevice,
        device: Arc<Device>,
    ) -> Result<Self, VulkanDeviceInitError> {
        let allocator = gpu_allocator::vulkan::Allocator::new(&AllocatorCreateDesc {
            instance: instance.instance.clone(),
            device: device.device.clone(),
            physical_device,
            debug_settings: Default::default(),
            buffer_device_address: false,
            allocation_sizes: Default::default(),
        })
        .map_err(VulkanCommonError::from)?;

        Ok(Self {
            allocator: Mutex::new(allocator),
            device,
        })
    }

    fn allocate(&self, desc: &AllocationCreateDesc) -> Result<Allocation, VulkanCommonError> {
        self.allocator
            .lock()
            .unwrap()
            .allocate(desc)
            .map_err(Into::into)
    }

    fn free(&self, allocation: Allocation) -> Result<(), VulkanCommonError> {
        self.allocator
            .lock()
            .unwrap()
            .free(allocation)
            .map_err(Into::into)
    }
}

fn allocation_scheme(
    requirements: vk::MemoryDedicatedRequirements,
    dedicated_sheme: AllocationScheme,
) -> AllocationScheme {
    if requirements.prefers_dedicated_allocation == vk::TRUE
        || requirements.requires_dedicated_allocation == vk::TRUE
    {
        dedicated_sheme
    } else {
        AllocationScheme::GpuAllocatorManaged
    }
}

/// Device memory allocation that owns whole memory block
pub(crate) struct DedicatedMemoryAllocation {
    pub(crate) memory: vk::DeviceMemory,
    pub(crate) size: u64,
    device: Arc<Device>,
}

impl DedicatedMemoryAllocation {
    pub(crate) fn new(
        device: &VulkanDevice,
        requirements: &vk::MemoryRequirements,
    ) -> Result<Self, VulkanCommonError> {
        let alloc_info = vk::MemoryAllocateInfo::default()
            .allocation_size(requirements.size)
            .memory_type_index(requirements.memory_type_bits.trailing_zeros());

        let memory = unsafe { device.device.allocate_memory(&alloc_info, None)? };

        Ok(Self {
            memory,
            size: requirements.size,
            device: device.device.clone(),
        })
    }
}

impl Drop for DedicatedMemoryAllocation {
    fn drop(&mut self) {
        unsafe { self.device.free_memory(self.memory, None) };
    }
}

pub(crate) struct DecodeInputBufferPool<'a> {
    freelist: Arc<Mutex<Vec<DecodeInputBuffer>>>,
    allocator: Arc<Allocator>,
    profile: Arc<H264DecodeProfileInfo<'a>>,
}

impl<'a> DecodeInputBufferPool<'a> {
    pub(crate) fn new(allocator: Arc<Allocator>, profile: Arc<H264DecodeProfileInfo<'a>>) -> Self {
        Self {
            allocator,
            freelist: Arc::new(Mutex::new(Vec::new())),
            profile,
        }
    }

    pub(crate) fn buffer(&mut self) -> Result<DecodeInputBuffer, VulkanDecoderError> {
        if let Some(buffer) = self.freelist.lock().unwrap().pop() {
            return Ok(buffer);
        }

        DecodeInputBuffer::new(
            self.allocator.clone(),
            &self.profile,
            Arc::downgrade(&self.freelist),
        )
    }
}

pub(crate) struct DecodeInputBuffer {
    pub(crate) buffer: Buffer,
    capacity: u64,
    allocator: Arc<Allocator>,
    pool_freelist: Weak<Mutex<Vec<DecodeInputBuffer>>>,
}

impl DecodeInputBuffer {
    pub(crate) fn new(
        allocator: Arc<Allocator>,
        profile: &H264DecodeProfileInfo,
        pool_freelist: Weak<Mutex<Vec<DecodeInputBuffer>>>,
    ) -> Result<Self, VulkanDecoderError> {
        const INITIAL_SIZE: u64 = 1024 * 1024; // 1MiB
        let buffer = Buffer::new_decode(allocator.clone(), INITIAL_SIZE, profile)?;

        Ok(Self {
            buffer,
            capacity: INITIAL_SIZE,
            allocator,
            pool_freelist,
        })
    }

    /// size must be passed in here for alignment reasons
    pub(crate) fn upload_data(
        &mut self,
        data: &[u8],
        size: u64,
        profile: &H264DecodeProfileInfo,
    ) -> Result<(), VulkanDecoderError> {
        debug_assert!(data.len() as u64 <= size);

        if self.capacity < size {
            let new_capacity = size.max(2 * self.capacity);
            self.buffer = Buffer::new_decode(self.allocator.clone(), new_capacity, profile)?;
            self.capacity = new_capacity;
        }

        self.buffer.copy_data_into(data)?;

        Ok(())
    }

    pub(crate) fn release_to_pool(self) {
        if let Some(pool_freelist) = self.pool_freelist.upgrade() {
            pool_freelist.lock().unwrap().push(self);
        }
    }
}

pub(crate) struct EncodeOutputBufferPool<'a> {
    freelist: Arc<Mutex<Vec<EncodeOutputBuffer>>>,
    allocator: Arc<Allocator>,
    profile: Arc<ProfileInfo<'a>>,
    buffer_len: u64,
}

impl<'a> EncodeOutputBufferPool<'a> {
    pub(crate) fn new(
        allocator: Arc<Allocator>,
        profile: Arc<ProfileInfo<'a>>,
        buffer_len: u64,
    ) -> Self {
        Self {
            allocator,
            freelist: Arc::new(Mutex::new(Vec::new())),
            profile,
            buffer_len,
        }
    }

    pub(crate) fn buffer(&mut self) -> Result<EncodeOutputBuffer, VulkanEncoderError> {
        if let Some(buffer) = self.freelist.lock().unwrap().pop() {
            return Ok(buffer);
        }

        let buffer = Buffer::new_encode(self.allocator.clone(), self.buffer_len, &self.profile)?;

        Ok(EncodeOutputBuffer {
            buffer,
            pool_freelist: Arc::downgrade(&self.freelist),
        })
    }
}

pub(crate) struct EncodeOutputBuffer {
    // TODO: this buffer should grow when necessary
    pub(crate) buffer: Buffer,
    pool_freelist: Weak<Mutex<Vec<EncodeOutputBuffer>>>,
}

impl EncodeOutputBuffer {
    pub(crate) fn release_to_pool(self) {
        if let Some(pool_freelist) = self.pool_freelist.upgrade() {
            pool_freelist.lock().unwrap().push(self);
        }
    }
}

pub(crate) struct EncodeInputImagePool<'a> {
    freelist: Arc<Mutex<Vec<EncodeInputImage>>>,
    encoding_device: Arc<EncodingDevice>,
    profile: Arc<ProfileInfo<'a>>,
    extent: vk::Extent3D,
    image_usages: vk::ImageUsageFlags,
    queue_family_indices: Vec<u32>,
    layout_tracker: Arc<Mutex<ImageLayoutTracker>>,
}

impl<'a> EncodeInputImagePool<'a> {
    pub(crate) fn new(
        encoding_device: Arc<EncodingDevice>,
        profile: Arc<ProfileInfo<'a>>,
        extent: vk::Extent3D,
        image_usages: vk::ImageUsageFlags,
        queue_family_indices: Vec<u32>,
        layout_tracker: Arc<Mutex<ImageLayoutTracker>>,
    ) -> Self {
        Self {
            freelist: Arc::new(Mutex::new(Vec::new())),
            encoding_device,
            profile,
            extent,
            image_usages,
            queue_family_indices,
            layout_tracker,
        }
    }

    pub(crate) fn image(&mut self) -> Result<EncodeInputImage, VulkanEncoderError> {
        if let Some(image) = self.freelist.lock().unwrap().pop() {
            return Ok(image);
        }

        let image = Image::new_encode(
            &self.encoding_device,
            self.extent,
            &self.profile,
            self.image_usages,
            &self.queue_family_indices,
            self.layout_tracker.clone(),
        )?;

        Ok(EncodeInputImage {
            image: Arc::new(image),
            pool_freelist: Arc::downgrade(&self.freelist),
        })
    }

    #[cfg(feature = "wgpu")]
    pub(crate) fn image_with_wgpu_texture(
        &mut self,
        wgpu_device: &wgpu::Device,
    ) -> Result<(EncodeInputImage, wgpu::Texture), VulkanEncoderError> {
        use wgpu::hal::vulkan::Api as VkApi;

        let hal_device = unsafe { wgpu_device.as_hal::<VkApi>().unwrap() };

        let image = self.image()?;

        let vk_extent = image.image.extent;
        let size = wgpu::Extent3d {
            width: vk_extent.width,
            height: vk_extent.height,
            depth_or_array_layers: vk_extent.depth,
        };

        let image_clone = image.image.clone();
        let hal_texture = unsafe {
            hal_device.texture_from_raw(
                image.image.image,
                &wgpu::hal::TextureDescriptor {
                    label: Some("gpu-video encoder input texture"),
                    size,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::NV12,
                    usage: wgpu::hal::vulkan::conv::map_vk_image_usage(self.image_usages),
                    memory_flags: wgpu::hal::MemoryFlags::empty(),
                    view_formats: Vec::new(),
                },
                Some(Box::new(move || {
                    drop(image_clone);
                })),
                wgpu::hal::vulkan::TextureMemory::External,
            )
        };

        let wgpu_texture = unsafe {
            wgpu_device.create_texture_from_hal::<VkApi>(
                hal_texture,
                &wgpu::TextureDescriptor {
                    label: Some("gpu-video encoder input texture"),
                    size,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::NV12,
                    usage: image_usage_to_wgpu_texture_usages(self.image_usages),
                    view_formats: &[],
                },
                wgpu::TextureUses::UNINITIALIZED,
            )
        };

        Ok((image, wgpu_texture))
    }
}

pub(crate) struct EncodeInputImage {
    pub(crate) image: Arc<Image>,
    pool_freelist: Weak<Mutex<Vec<EncodeInputImage>>>,
}

impl EncodeInputImage {
    pub(crate) fn release_to_pool(self) {
        if let Some(pool_freelist) = self.pool_freelist.upgrade() {
            pool_freelist.lock().unwrap().push(self);
        }
    }
}

#[cfg(feature = "wgpu")]
fn image_usage_to_wgpu_texture_usages(usage: vk::ImageUsageFlags) -> wgpu::TextureUsages {
    let mut usages = wgpu::TextureUsages::empty();
    usages.set(
        wgpu::TextureUsages::COPY_SRC,
        usage.contains(vk::ImageUsageFlags::TRANSFER_SRC),
    );
    usages.set(
        wgpu::TextureUsages::COPY_DST,
        usage.contains(vk::ImageUsageFlags::TRANSFER_DST),
    );
    usages.set(
        wgpu::TextureUsages::TEXTURE_BINDING,
        usage.contains(vk::ImageUsageFlags::SAMPLED),
    );
    usages.set(
        wgpu::TextureUsages::STORAGE_BINDING,
        usage.contains(vk::ImageUsageFlags::STORAGE),
    );
    usages.set(
        wgpu::TextureUsages::RENDER_ATTACHMENT,
        usage.intersects(
            vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
        ),
    );
    usages.set(
        wgpu::TextureUsages::TRANSIENT_ATTACHMENT,
        usage.contains(vk::ImageUsageFlags::TRANSIENT_ATTACHMENT),
    );
    usages
}

pub(crate) struct Buffer {
    pub(crate) buffer: vk::Buffer,
    allocation: Allocation,
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
        profile: &H264DecodeProfileInfo,
    ) -> Result<Self, VulkanCommonError> {
        let mut profile_list_info = vk::VideoProfileListInfoKHR::default()
            .profiles(std::slice::from_ref(&profile.profile_info.profile_info));

        let buffer_create_info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(vk::BufferUsageFlags::VIDEO_DECODE_SRC_KHR)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .push_next(&mut profile_list_info);

        Self::new(allocator, buffer_create_info, TransferDirection::MemToGpu)
    }

    pub(crate) fn new_encode(
        allocator: Arc<Allocator>,
        size: u64,
        profile: &ProfileInfo,
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

    pub(crate) fn new_transfer(
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

    pub(crate) fn new_transfer_with_data(
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
        let location = match transfer_direction {
            TransferDirection::GpuToMem => MemoryLocation::GpuToCpu,
            TransferDirection::MemToGpu => MemoryLocation::CpuToGpu,
        };

        let device = &allocator.device;
        let buffer = unsafe { device.create_buffer(&create_info, None)? };
        let allocation = Self::allocate_and_bind(&allocator, buffer, location)?;

        Ok(Self {
            buffer,
            allocation,
            allocator,
            transfer_direction,
        })
    }

    fn allocate_and_bind(
        allocator: &Allocator,
        buffer: vk::Buffer,
        location: MemoryLocation,
    ) -> Result<Allocation, VulkanCommonError> {
        let device = &allocator.device;

        let mut dedicated_requirements = vk::MemoryDedicatedRequirements::default();
        let mut requirements =
            vk::MemoryRequirements2::default().push_next(&mut dedicated_requirements);
        unsafe {
            device.get_buffer_memory_requirements2(
                &vk::BufferMemoryRequirementsInfo2::default().buffer(buffer),
                &mut requirements,
            )
        };

        let requirements = requirements.memory_requirements;

        let allocation = allocator.allocate(&AllocationCreateDesc {
            name: "gpu-video buffer",
            requirements,
            location,
            linear: true,
            allocation_scheme: allocation_scheme(
                dedicated_requirements,
                AllocationScheme::DedicatedBuffer(buffer),
            ),
        })?;

        unsafe { device.bind_buffer_memory(buffer, allocation.memory(), allocation.offset())? };

        Ok(allocation)
    }

    fn mapped_slice_mut(&mut self) -> Result<&mut [u8], VulkanCommonError> {
        self.allocation
            .mapped_slice_mut()
            .ok_or(VulkanCommonError::BufferNotMapped)
    }

    /// ## Safety
    /// the buffer has to be mappable and readable
    pub(crate) unsafe fn download_data_from_buffer(
        &mut self,
        size: usize,
    ) -> Result<Vec<u8>, VulkanCommonError> {
        unsafe { self.download_data_from_buffer_at(0, size) }
    }

    /// ## Safety
    /// the buffer has to be mappable and readable.
    /// `offset + size` must not exceed the buffer allocation size.
    pub(crate) unsafe fn download_data_from_buffer_at(
        &mut self,
        offset: usize,
        size: usize,
    ) -> Result<Vec<u8>, VulkanCommonError> {
        let memory = self.mapped_slice_mut()?;
        Ok(memory[offset..offset + size].to_vec())
    }

    fn copy_data_into(&mut self, data: &[u8]) -> Result<(), VulkanCommonError> {
        if self.transfer_direction != TransferDirection::MemToGpu {
            return Err(VulkanCommonError::UploadToImproperBuffer);
        }

        let memory = self.mapped_slice_mut()?;
        memory[..data.len()].copy_from_slice(data);

        Ok(())
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        unsafe { self.allocator.device.destroy_buffer(self.buffer, None) };
        self.allocator
            .free(std::mem::take(&mut self.allocation))
            .unwrap();
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
    allocation: Allocation,
    allocator: Arc<Allocator>,
    tracker: Arc<Mutex<ImageLayoutTracker>>,
    pub(crate) device: Arc<Device>,
    pub(crate) extent: vk::Extent3D,
}

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub struct ImageKey(u64);

impl Image {
    pub(crate) fn new(
        allocator: Arc<Allocator>,
        image_create_info: &vk::ImageCreateInfo,
        tracker: Arc<Mutex<ImageLayoutTracker>>,
    ) -> Result<Self, VulkanCommonError> {
        let extent = image_create_info.extent;
        let image = unsafe { allocator.device.create_image(image_create_info, None)? };

        let linear = image_create_info.tiling == vk::ImageTiling::LINEAR;
        let allocation = Self::allocate_and_bind(&allocator, image, linear)?;

        tracker.lock().unwrap().register_image(
            ImageKey(image.as_raw()),
            image_create_info.initial_layout,
            image_create_info.array_layers as usize,
        )?;

        Ok(Image {
            image,
            allocation,
            device: allocator.device.clone(),
            allocator,
            tracker,
            extent,
        })
    }

    fn allocate_and_bind(
        allocator: &Allocator,
        image: vk::Image,
        linear: bool,
    ) -> Result<Allocation, VulkanCommonError> {
        let device = &allocator.device;
        let mut dedicated_requirements = vk::MemoryDedicatedRequirements::default();
        let mut requirements =
            vk::MemoryRequirements2::default().push_next(&mut dedicated_requirements);
        unsafe {
            device.get_image_memory_requirements2(
                &vk::ImageMemoryRequirementsInfo2::default().image(image),
                &mut requirements,
            )
        };

        let requirements = requirements.memory_requirements;
        let allocation = allocator.allocate(&AllocationCreateDesc {
            name: "gpu-video image",
            requirements,
            location: MemoryLocation::GpuOnly,
            linear,
            allocation_scheme: allocation_scheme(
                dedicated_requirements,
                AllocationScheme::DedicatedImage(image),
            ),
        })?;

        unsafe { device.bind_image_memory(image, allocation.memory(), allocation.offset())? };

        Ok(allocation)
    }

    pub(crate) fn new_encode(
        device: &EncodingDevice,
        extent: vk::Extent3D,
        profile: &ProfileInfo,
        additional_usages: vk::ImageUsageFlags,
        additional_queue_family_indices: &[u32],
        tracker: Arc<Mutex<ImageLayoutTracker>>,
    ) -> Result<Self, VulkanCommonError> {
        let mut profile_list_info = vk::VideoProfileListInfoKHR::default()
            .profiles(std::slice::from_ref(&profile.profile_info));
        let mut queue_indices = vec![device.encode_queues.family_index as u32];
        queue_indices.extend_from_slice(additional_queue_family_indices);

        let encode_image_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(vk::Format::G8_B8R8_2PLANE_420_UNORM)
            .extent(extent)
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(additional_usages | vk::ImageUsageFlags::VIDEO_ENCODE_SRC_KHR)
            .sharing_mode(vk::SharingMode::CONCURRENT)
            .queue_family_indices(&queue_indices)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .flags(vk::ImageCreateFlags::MUTABLE_FORMAT | vk::ImageCreateFlags::EXTENDED_USAGE)
            .push_next(&mut profile_list_info);

        Self::new(device.allocator.clone(), &encode_image_info, tracker)
    }

    pub(crate) fn transition_layout_raw(
        &self,
        command_buffer: vk::CommandBuffer,
        layout: &mut [vk::ImageLayout],
        stages: std::ops::Range<vk::PipelineStageFlags2>,
        accesses: std::ops::Range<vk::AccessFlags2>,
        new_layout: vk::ImageLayout,
        subresource_range: vk::ImageSubresourceRange,
    ) -> Result<(), VulkanCommonError> {
        let barrier = vk::ImageMemoryBarrier2::default()
            .src_stage_mask(stages.start)
            .dst_stage_mask(stages.end)
            .src_access_mask(accesses.start)
            .dst_access_mask(accesses.end)
            .new_layout(new_layout)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(self.image)
            .subresource_range(subresource_range);

        let end = if subresource_range.layer_count == vk::REMAINING_ARRAY_LAYERS {
            layout.len()
        } else {
            subresource_range.base_array_layer as usize + subresource_range.layer_count as usize
        };

        let mut current_old_layout = None;
        let mut current_start = None;

        for (i, layout) in layout[subresource_range.base_array_layer as usize..end]
            .iter()
            .enumerate()
        {
            let i = i + subresource_range.base_array_layer as usize;

            if current_old_layout.is_none() {
                if *layout == new_layout {
                    continue;
                }

                current_old_layout = Some(*layout);
                current_start = Some(i);
                continue;
            }

            if let Some(old) = current_old_layout {
                if old == *layout {
                    continue;
                }

                let start = current_start.unwrap();

                let barrier =
                    barrier
                        .old_layout(old)
                        .subresource_range(vk::ImageSubresourceRange {
                            base_array_layer: start as u32,
                            layer_count: (i - start) as u32,
                            ..subresource_range
                        });

                unsafe {
                    self.device.cmd_pipeline_barrier2(
                        command_buffer,
                        &vk::DependencyInfo::default().image_memory_barriers(&[barrier]),
                    );
                }

                if *layout != new_layout {
                    current_old_layout = Some(*layout);
                    current_start = Some(i);
                } else {
                    current_old_layout = None;
                    current_start = None;
                }
            }
        }

        if let Some(old) = current_old_layout {
            let start = current_start.unwrap();

            let barrier = barrier
                .old_layout(old)
                .subresource_range(vk::ImageSubresourceRange {
                    base_array_layer: start as u32,
                    layer_count: (end - start) as u32,
                    ..subresource_range
                });

            unsafe {
                self.device.cmd_pipeline_barrier2(
                    command_buffer,
                    &vk::DependencyInfo::default().image_memory_barriers(&[barrier]),
                );
            }
        }

        for layout in layout[subresource_range.base_array_layer as usize..end].iter_mut() {
            *layout = new_layout;
        }

        Ok(())
    }

    pub(crate) fn transition_layout(
        &self,
        command_buffer: &mut OpenCommandBuffer,
        stages: std::ops::Range<vk::PipelineStageFlags2>,
        accesses: std::ops::Range<vk::AccessFlags2>,
        new_layout: vk::ImageLayout,
        subresource_range: vk::ImageSubresourceRange,
    ) -> Result<(), VulkanCommonError> {
        let raw_buffer = command_buffer.buffer();
        let layout = command_buffer.image_layout(self.key(), &self.tracker)?;

        self.transition_layout_raw(
            raw_buffer,
            layout,
            stages,
            accesses,
            new_layout,
            subresource_range,
        )
    }

    pub(crate) fn transition_layout_single_layer(
        &self,
        command_buffer: &mut OpenCommandBuffer,
        stages: std::ops::Range<vk::PipelineStageFlags2>,
        accesses: std::ops::Range<vk::AccessFlags2>,
        new_layout: vk::ImageLayout,
        base_array_layer: u32,
    ) -> Result<(), VulkanCommonError> {
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

    pub(crate) fn key(&self) -> ImageKey {
        ImageKey(self.image.as_raw())
    }

    #[cfg_attr(not(feature = "transcoder"), allow(dead_code))]
    pub(crate) fn create_plane_view(
        self: &Arc<Self>,
        layer: u32,
        plane: vk::ImageAspectFlags,
        usage: vk::ImageUsageFlags,
    ) -> Result<ImageView, VulkanCommonError> {
        let mut view_usage_info = vk::ImageViewUsageCreateInfo::default().usage(usage);

        let format = match plane {
            vk::ImageAspectFlags::PLANE_0 => vk::Format::R8_UNORM,
            vk::ImageAspectFlags::PLANE_1 => vk::Format::R8G8_UNORM,
            aspect => return Err(VulkanCommonError::UnsupportedImageAspect(aspect)),
        };

        let view_create_info = vk::ImageViewCreateInfo::default()
            .flags(vk::ImageViewCreateFlags::empty())
            .image(self.image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .components(vk::ComponentMapping::default())
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: plane,
                base_array_layer: layer,
                level_count: 1,
                base_mip_level: 0,
                layer_count: 1,
            })
            .push_next(&mut view_usage_info);

        ImageView::new(self.device.clone(), self.clone(), &view_create_info)
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
        if let Err(e) = self.tracker.lock().unwrap().unregister_image(self.key()) {
            tracing::error!("Error while freeing image: {e}")
        }

        unsafe { self.device.destroy_image(self.image, None) };
        self.allocator
            .free(std::mem::take(&mut self.allocation))
            .unwrap();
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
