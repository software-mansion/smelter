use std::collections::HashMap;

use tracing::debug;

/// Capture buffers imported for the GPU to read in place, by address. A
/// buffer that can't be imported stays `None`, and its frames get copied.
pub(crate) struct HostImports {
    device: wgpu::Device,
    queue: wgpu::Queue,
    /// Dropped first: waits for the copies reading `buffers`.
    importer: Option<Importer>,
    buffers: HashMap<usize, Option<wgpu::Buffer>>,
}

impl HostImports {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        Self {
            importer: Importer::new(&device),
            device,
            queue,
            buffers: HashMap::new(),
        }
    }

    /// # Safety
    ///
    /// `memory` must stay allocated at its address until `clear` or drop.
    pub unsafe fn get(&mut self, memory: &[u8]) -> Option<&wgpu::Buffer> {
        let importer = self.importer.as_ref()?;
        self.buffers
            .entry(memory.as_ptr() as usize)
            .or_insert_with(|| {
                unsafe { importer.import(&self.device, memory) }
                    .inspect_err(
                        |err| debug!(%err, "Capture buffer not imported, copying its frames."),
                    )
                    .ok()
            })
            .as_ref()
    }

    /// Tracks the copy just submitted from an imported buffer.
    pub fn copy_submitted(&mut self) {
        if let Some(importer) = &mut self.importer {
            importer.signal(&self.queue);
        }
    }

    /// Waits for the copies submitted so far, before the driver gets their
    /// buffers back: a frame after they were submitted, so it rarely waits.
    pub fn wait_for_copies(&self) {
        if let Some(importer) = &self.importer {
            importer.wait();
        }
    }

    #[cfg(feature = "decklink")]
    pub fn clear(&mut self) {
        self.wait_for_copies();
        self.buffers.clear();
    }
}

/// Imports host memory through `VK_EXT_external_memory_host`, which devices
/// from `gpu-video` enable when supported, and counts the copies from it on a
/// timeline semaphore: submitted, and completed.
#[cfg(feature = "gpu-video")]
struct Importer {
    device: ash::Device,
    external_memory_host: ash::ext::external_memory_host::Device,
    semaphore: ash::vk::Semaphore,
    submitted: u64,
}

#[cfg(feature = "gpu-video")]
#[derive(Debug, thiserror::Error)]
enum ImportError {
    #[error("Host memory must start on a page boundary.")]
    Unaligned,
    #[error("No memory type can import this host memory.")]
    NoMemoryType,
    #[error(transparent)]
    Vulkan(#[from] ash::vk::Result),
}

#[cfg(feature = "gpu-video")]
impl Importer {
    fn new(device: &wgpu::Device) -> Option<Self> {
        use ash::vk;
        let hal = unsafe { device.as_hal::<wgpu::hal::api::Vulkan>() }?;
        if !hal
            .enabled_device_extensions()
            .contains(&ash::ext::external_memory_host::NAME)
        {
            return None;
        }
        let device = hal.raw_device().clone();
        let external_memory_host = ash::ext::external_memory_host::Device::new(
            hal.shared_instance().raw_instance(),
            &device,
        );
        let semaphore = unsafe {
            device.create_semaphore(
                &vk::SemaphoreCreateInfo::default().push_next(
                    &mut vk::SemaphoreTypeCreateInfo::default()
                        .semaphore_type(vk::SemaphoreType::TIMELINE),
                ),
                None,
            )
        }
        .inspect_err(|err| debug!(%err, "No timeline semaphore, copying capture frames."))
        .ok()?;
        Some(Self {
            device,
            external_memory_host,
            semaphore,
            submitted: 0,
        })
    }

    /// `memory` as a buffer the GPU copies from in place.
    ///
    /// # Safety
    ///
    /// `memory` must stay allocated at its address until the buffer is
    /// dropped and the GPU finished reading it.
    unsafe fn import(
        &self,
        device: &wgpu::Device,
        memory: &[u8],
    ) -> Result<wgpu::Buffer, ImportError> {
        use ash::vk;
        use wgpu::hal::{api::Vulkan, vulkan};

        const PAGE: usize = 4096;

        let address = memory.as_ptr() as *mut std::ffi::c_void;
        if !(address as usize).is_multiple_of(PAGE) {
            return Err(ImportError::Unaligned);
        }
        let size = memory.len().next_multiple_of(PAGE) as u64;
        let handle_type = vk::ExternalMemoryHandleTypeFlags::HOST_ALLOCATION_EXT;
        let mut properties = vk::MemoryHostPointerPropertiesEXT::default();
        unsafe {
            (self
                .external_memory_host
                .fp()
                .get_memory_host_pointer_properties_ext)(
                self.device.handle(),
                handle_type,
                address,
                &mut properties,
            )
        }
        .result()?;
        let memory_type = properties.memory_type_bits.trailing_zeros();
        if memory_type == u32::BITS {
            return Err(ImportError::NoMemoryType);
        }
        let raw = self.device.clone();
        let device_memory = unsafe {
            raw.allocate_memory(
                &vk::MemoryAllocateInfo::default()
                    .allocation_size(size)
                    .memory_type_index(memory_type)
                    .push_next(
                        &mut vk::ImportMemoryHostPointerInfoEXT::default()
                            .handle_type(handle_type)
                            .host_pointer(address),
                    ),
                None,
            )?
        };
        let buffer = unsafe {
            raw.create_buffer(
                &vk::BufferCreateInfo::default()
                    .size(size)
                    .usage(vk::BufferUsageFlags::TRANSFER_SRC)
                    .push_next(
                        &mut vk::ExternalMemoryBufferCreateInfo::default()
                            .handle_types(handle_type),
                    ),
                None,
            )
        }
        .and_then(|buffer| {
            unsafe { raw.bind_buffer_memory(buffer, device_memory, 0) }.map(|()| buffer)
        })
        .inspect_err(|_| unsafe { raw.free_memory(device_memory, None) })?;
        let hal_buffer = unsafe {
            vulkan::Buffer::from_raw_externally_owned(
                buffer,
                Box::new(move || {
                    raw.destroy_buffer(buffer, None);
                    raw.free_memory(device_memory, None);
                }),
            )
        };
        Ok(unsafe {
            device.create_buffer_from_hal::<Vulkan>(
                hal_buffer,
                &wgpu::BufferDescriptor {
                    label: Some("host memory"),
                    size,
                    usage: wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: false,
                },
            )
        })
    }

    fn signal(&mut self, queue: &wgpu::Queue) {
        self.submitted += 1;
        // Rides on the next submission, whichever thread makes it: it runs
        // after the copy, so the semaphore never signals early.
        unsafe { queue.as_hal::<wgpu::hal::api::Vulkan>() }
            .unwrap()
            .add_signal_semaphore(self.semaphore, Some(self.submitted));
        queue.submit([]);
    }

    /// Unlike a blocking `wgpu::Device::poll`, which holds wgpu's device lock
    /// and stalls every other thread's submissions, waits on the semaphore
    /// alone.
    fn wait(&self) {
        let wait = ash::vk::SemaphoreWaitInfo::default()
            .semaphores(std::slice::from_ref(&self.semaphore))
            .values(std::slice::from_ref(&self.submitted));
        if let Err(err) = unsafe { self.device.wait_semaphores(&wait, u64::MAX) } {
            tracing::error!(%err, "Failed to wait for capture buffer copies");
        }
    }
}

#[cfg(feature = "gpu-video")]
impl Drop for Importer {
    fn drop(&mut self) {
        self.wait();
        unsafe { self.device.destroy_semaphore(self.semaphore, None) };
    }
}

/// Without `gpu-video` nothing is imported, so there's nothing to wait for.
#[cfg(not(feature = "gpu-video"))]
enum Importer {}

#[cfg(not(feature = "gpu-video"))]
impl Importer {
    fn new(_device: &wgpu::Device) -> Option<Self> {
        None
    }

    unsafe fn import(
        &self,
        _device: &wgpu::Device,
        _memory: &[u8],
    ) -> Result<wgpu::Buffer, std::convert::Infallible> {
        match *self {}
    }

    fn signal(&mut self, _queue: &wgpu::Queue) {
        match *self {}
    }

    fn wait(&self) {
        match *self {}
    }
}
