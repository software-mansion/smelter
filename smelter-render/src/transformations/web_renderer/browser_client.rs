use std::{
    ffi::c_int,
    sync::{Arc, Mutex},
};

use crate::{Resolution, wgpu::WgpuCtx};
use ash::vk::{self, ExternalMemoryHandleTypeFlags};
use bytes::Bytes;
use libcef::PixelPlane;
use tracing::error;
use vk_mem::{Alloc, AllocatorCreateInfo};

use crate::transformations::web_renderer::{FrameData, SourceTransforms};

use super::{
    GET_FRAME_POSITIONS_MESSAGE,
    transformation_matrices::{Position, vertices_transformation_matrix},
};

#[derive(Clone)]
pub(super) struct BrowserClient {
    ctx: Arc<WgpuCtx>,
    frame_data: FrameData,
    source_transforms: SourceTransforms,
    resolution: Resolution,
}

impl libcef::Client for BrowserClient {
    type RenderHandlerType = RenderHandler;

    fn render_handler(&self) -> Option<Self::RenderHandlerType> {
        Some(RenderHandler::new(
            self.ctx.clone(),
            self.frame_data.clone(),
            self.resolution,
        ))
    }

    fn on_process_message_received(
        &mut self,
        _browser: &libcef::Browser,
        _frame: &libcef::Frame,
        _source_process: libcef::ProcessId,
        message: &libcef::ProcessMessage,
    ) -> bool {
        match message.name().as_str() {
            GET_FRAME_POSITIONS_MESSAGE => {
                let mut transforms_matrices = Vec::new();
                for i in (0..message.size()).step_by(4) {
                    let position = match Self::read_frame_position(message, i) {
                        Ok(position) => position,
                        Err(err) => {
                            error!(
                                "Error occurred while reading frame positions from IPC message: {err}"
                            );
                            return true;
                        }
                    };

                    let transformations_matrix =
                        vertices_transformation_matrix(&position, &self.resolution);

                    transforms_matrices.push(transformations_matrix);
                }

                let mut source_transforms = self.source_transforms.lock().unwrap();
                *source_transforms = transforms_matrices;
            }
            ty => error!("Unknown process message type \"{ty}\""),
        }
        true
    }
}

impl BrowserClient {
    pub fn new(
        ctx: Arc<WgpuCtx>,
        frame_data: FrameData,
        source_transforms: SourceTransforms,
        resolution: Resolution,
    ) -> Self {
        Self {
            ctx,
            frame_data,
            source_transforms,
            resolution,
        }
    }

    fn read_frame_position(
        msg: &libcef::ProcessMessage,
        index: usize,
    ) -> Result<Position, libcef::ProcessMessageError> {
        let x = msg.read_double(index)?;
        let y = msg.read_double(index + 1)?;
        let width = msg.read_double(index + 2)?;
        let height = msg.read_double(index + 3)?;

        Ok(Position {
            top: y as f32,
            left: x as f32,
            width: width as f32,
            height: height as f32,
            rotation_degrees: 0.0,
        })
    }
}

pub(super) struct RenderHandler {
    ctx: Arc<WgpuCtx>,
    frame_data: FrameData,
    resolution: Resolution,
}

impl libcef::RenderHandler for RenderHandler {
    fn resolution(&self, _browser: &libcef::Browser) -> libcef::Resolution {
        libcef::Resolution {
            width: self.resolution.width,
            height: self.resolution.height,
        }
    }

    fn on_paint(&self, _browser: &libcef::Browser, buffer: &[u8], _resolution: libcef::Resolution) {
        let mut frame_data = self.frame_data.lock().unwrap();
        *frame_data = Bytes::copy_from_slice(buffer);
    }

    fn on_accelerated_paint(
        &self,
        browser: &libcef::Browser,
        planes: &[libcef::PixelPlane],
        format: libcef::ColorFormat,
    ) {
        let frame = &planes[0];
    }
}

impl RenderHandler {
    pub fn new(ctx: Arc<WgpuCtx>, frame_data: Arc<Mutex<Bytes>>, resolution: Resolution) -> Self {
        Self {
            ctx,
            frame_data,
            resolution,
        }
    }
}

use wgpu::hal::vulkan::Api as VkApi;

pub struct SharedTexture {
    fd: c_int,
}

impl SharedTexture {
    fn new(ctx: &WgpuCtx, frame: &PixelPlane, format: libcef::ColorFormat) -> Self {
        let instance = unsafe {
            ctx.instance
                .as_hal::<VkApi>()
                .unwrap()
                .shared_instance()
                .raw_instance()
        };
        let physical_device = unsafe {
            ctx.adapter
                .as_hal::<VkApi, _, _>(|adapter| adapter.unwrap().raw_physical_device())
        };

        unsafe {
            ctx.device.as_hal::<VkApi, _, _>(|device| {
                let device = device.unwrap().raw_device();

                let allocator = Arc::new(
                    vk_mem::Allocator::new(AllocatorCreateInfo::new(
                        instance,
                        device,
                        physical_device,
                    ))
                    .unwrap(),
                );

                // TODO: Handle different rendering modes?
                let image_format = match format {
                    libcef::ColorFormat::Rgba8888 => vk::Format::R8G8B8A8_UNORM,
                    libcef::ColorFormat::Bgra8888 => vk::Format::B8G8R8A8_UNORM,
                    libcef::ColorFormat::NumValues => todo!(),
                };
                let plane_layouts = &[vk::SubresourceLayout {
                    offset: 0,
                    size: 0,
                    row_pitch: frame.stride as u64,
                    array_pitch: 0,
                    depth_pitch: 0,
                }];
                let mut drm_modifier_info =
                    vk::ImageDrmFormatModifierExplicitCreateInfoEXT::default()
                        .drm_format_modifier(frame.modifier)
                        .plane_layouts(plane_layouts);
                let mut external_image_info = vk::ExternalMemoryImageCreateInfo::default()
                    .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
                let image_create_info = vk::ImageCreateInfo::default()
                    .push_next(&mut external_image_info)
                    .push_next(&mut drm_modifier_info)
                    .image_type(vk::ImageType::TYPE_2D)
                    .format(image_format)
                    .extent(vk::Extent3D {
                        width: frame.stride / 4,
                        height: (frame.size / frame.stride as u64) as u32,
                        depth: 1,
                    })
                    .mip_levels(1)
                    .array_layers(1)
                    .samples(vk::SampleCountFlags::TYPE_1)
                    .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
                    .usage(vk::ImageUsageFlags::TRANSFER_SRC)
                    .initial_layout(vk::ImageLayout::UNDEFINED)
                    .sharing_mode(vk::SharingMode::EXCLUSIVE);

                let image = device.create_image(&image_create_info, None).unwrap();
                let ext_mem_fd_loader = ash::khr::external_memory_fd::Device::new(instance, device);
                let mut fd_props = vk::MemoryFdPropertiesKHR::default();
                ext_mem_fd_loader
                    .get_memory_fd_properties(
                        vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT,
                        frame.fd,
                        &mut fd_props,
                    )
                    .unwrap();

                let image_mem_reqs = device.get_image_memory_requirements(image);
                let alloc_info = vk_mem::AllocationCreateInfo {
                    usage: vk_mem::MemoryUsage::Auto,
                    required_flags: vk::MemoryPropertyFlags::DEVICE_LOCAL,
                    ..Default::default()
                };
                let mem_type_index = unsafe {
                    // DANGER: This might be wrong
                    allocator
                        .find_memory_type_index_for_image_info(image_create_info, &alloc_info)
                        .unwrap()
                };

                let mut import_mem_info = vk::ImportMemoryFdInfoKHR::default()
                    .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT)
                    .fd(frame.fd);
                let mem_alloc_info = vk::MemoryAllocateInfo::default()
                    .push_next(&mut import_mem_info)
                    .allocation_size(image_mem_reqs.size)
                    .memory_type_index(mem_type_index);
                let memory = device.allocate_memory(&mem_alloc_info, None).unwrap();
                device.bind_image_memory(image, memory, 0).unwrap();
            });
        }

        Self { fd: frame.fd }
    }
}
