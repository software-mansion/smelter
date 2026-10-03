use std::{alloc::Layout, ffi::c_void, ptr::NonNull};

use ash::vk;
use wgpu::hal::{api::Vulkan, vulkan};

/// Allocations are aligned to and sized in transparent huge pages: devices
/// that DMA into host memory pin and IOMMU-map it page by page.
const HUGE_PAGE: usize = 2 << 20;

#[derive(Debug, thiserror::Error)]
pub enum HostMemoryError {
    #[error("The device can't read host memory (VK_EXT_external_memory_host).")]
    Unsupported,
    #[error("No memory type can import host memory into a transfer source buffer.")]
    NoCompatibleMemoryType,
    #[error(transparent)]
    Vulkan(#[from] vk::Result),
}

/// Host memory the GPU copies from in place: a device can capture into it,
/// and the GPU copies out of it without the CPU touching the data.
pub struct HostMemoryBuffer {
    buffer: wgpu::Buffer,
    address: usize,
}

impl HostMemoryBuffer {
    /// Devices from `request_device_with_video_support` can read host memory
    /// when their adapter supports `VK_EXT_external_memory_host`.
    pub fn is_supported(device: &wgpu::Device) -> bool {
        unsafe { device.as_hal::<Vulkan>() }.is_some_and(|hal| imports_host_memory(&hal))
    }

    pub fn new(device: &wgpu::Device, size: usize) -> Result<Self, HostMemoryError> {
        let len = size.next_multiple_of(HUGE_PAGE);
        let memory = {
            let hal = unsafe { device.as_hal::<Vulkan>() }
                .filter(|hal| imports_host_memory(hal))
                .ok_or(HostMemoryError::Unsupported)?;
            ImportedMemory::new(&hal, len)?
        };
        let address = memory.address;
        let hal_buffer = unsafe {
            vulkan::Buffer::from_raw_externally_owned(memory.buffer, Box::new(move || drop(memory)))
        };
        let buffer = unsafe {
            device.create_buffer_from_hal::<Vulkan>(
                hal_buffer,
                &wgpu::BufferDescriptor {
                    label: Some("host memory buffer"),
                    size: len as u64,
                    usage: wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: false,
                },
            )
        };
        Ok(Self { buffer, address })
    }

    pub fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }

    /// Valid for writes while this buffer lives and the GPU isn't reading it.
    pub fn as_ptr(&self) -> NonNull<u8> {
        NonNull::new(self.address as *mut u8).unwrap()
    }
}

fn imports_host_memory(hal: &vulkan::Device) -> bool {
    hal.enabled_device_extensions()
        .contains(&ash::ext::external_memory_host::NAME)
}

/// Dropping frees whatever was created, the Vulkan objects before the memory
/// under them.
struct ImportedMemory {
    device: ash::Device,
    address: usize,
    layout: Layout,
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
}

impl ImportedMemory {
    fn new(hal: &vulkan::Device, len: usize) -> Result<Self, HostMemoryError> {
        let layout = Layout::from_size_align(len, HUGE_PAGE).unwrap();
        let address = unsafe { std::alloc::alloc(layout) } as *mut c_void;
        if address.is_null() {
            return Err(vk::Result::ERROR_OUT_OF_HOST_MEMORY.into());
        }
        let device = hal.raw_device().clone();
        let mut memory = Self {
            device: device.clone(),
            address: address as usize,
            layout,
            buffer: vk::Buffer::null(),
            memory: vk::DeviceMemory::null(),
        };
        #[cfg(target_os = "linux")]
        unsafe {
            libc::madvise(address, len, libc::MADV_HUGEPAGE)
        };

        let handle_type = vk::ExternalMemoryHandleTypeFlags::HOST_ALLOCATION_EXT;
        let host_memory = ash::ext::external_memory_host::Device::new(
            hal.shared_instance().raw_instance(),
            &device,
        );
        let mut host_properties = vk::MemoryHostPointerPropertiesEXT::default();
        unsafe {
            (host_memory.fp().get_memory_host_pointer_properties_ext)(
                device.handle(),
                handle_type,
                address,
                &mut host_properties,
            )
        }
        .result()?;
        memory.buffer = unsafe {
            device.create_buffer(
                &vk::BufferCreateInfo::default()
                    .size(len as u64)
                    .usage(vk::BufferUsageFlags::TRANSFER_SRC)
                    .push_next(
                        &mut vk::ExternalMemoryBufferCreateInfo::default()
                            .handle_types(handle_type),
                    ),
                None,
            )?
        };
        let requirements = unsafe { device.get_buffer_memory_requirements(memory.buffer) };
        let memory_type_bits = host_properties.memory_type_bits & requirements.memory_type_bits;
        if memory_type_bits == 0 {
            return Err(HostMemoryError::NoCompatibleMemoryType);
        }
        memory.memory = unsafe {
            device.allocate_memory(
                &vk::MemoryAllocateInfo::default()
                    .allocation_size(len as u64)
                    .memory_type_index(memory_type_bits.trailing_zeros())
                    .push_next(
                        &mut vk::ImportMemoryHostPointerInfoEXT::default()
                            .handle_type(handle_type)
                            .host_pointer(address),
                    ),
                None,
            )?
        };
        unsafe { device.bind_buffer_memory(memory.buffer, memory.memory, 0)? };
        Ok(memory)
    }
}

impl Drop for ImportedMemory {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_buffer(self.buffer, None);
            self.device.free_memory(self.memory, None);
            std::alloc::dealloc(self.address as *mut u8, self.layout);
        }
    }
}
