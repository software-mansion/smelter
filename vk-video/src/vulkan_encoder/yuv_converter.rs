use std::sync::{Arc, Mutex};

use ash::vk;
use wgpu::hal::{CommandEncoder, Device, Queue, vulkan::Api as VkApi};

use crate::{
    VulkanCommonError, VulkanDevice,
    device::EncodingDevice,
    wrappers::{
        DescriptorPool, DescriptorSetLayout, Framebuffer, Image, ImageView, Pipeline,
        PipelineLayout, RenderPass, Sampler, ShaderModule,
    },
};

use super::H264EncodeProfileInfo;

#[derive(Debug, thiserror::Error)]
pub enum YuvConverterError {
    #[error(transparent)]
    VulkanCommonError(#[from] VulkanCommonError),

    #[error(transparent)]
    WgpuDeviceError(#[from] wgpu::hal::DeviceError),
}

pub(crate) struct Converter {
    device: Arc<VulkanDevice>,
    image: Arc<Mutex<Image>>,
    pipeline_y: ConvertingPipeline,
    pipeline_uv: ConvertingPipeline,
}

impl Converter {
    pub(crate) fn new(
        device: Arc<EncodingDevice>,
        width: u32,
        height: u32,
        profile: &H264EncodeProfileInfo,
    ) -> Result<Self, YuvConverterError> {
        let mut fence = unsafe {
            device
                .wgpu_device()
                .as_hal::<VkApi, _, _>(|d| d.unwrap().create_fence())?
        };

        let mut command_encoder = unsafe {
            device.wgpu_device().as_hal::<VkApi, _, _>(|d| {
                device.wgpu_queue().as_hal::<VkApi, _, _>(|q| {
                    d.unwrap()
                        .create_command_encoder(&wgpu::hal::CommandEncoderDescriptor {
                            label: Some("YUV converter init command encoder"),
                            queue: q.unwrap(),
                        })
                })
            })?
        };

        unsafe { command_encoder.begin_encoding(Some("YUV converter init recording"))? };
        let command_buffer = unsafe { command_encoder.raw_handle() };

        let extent = vk::Extent3D {
            width,
            height,
            depth: 1,
        };

        let mut profile_list_info = vk::VideoProfileListInfoKHR::default()
            .profiles(std::slice::from_ref(&profile.profile_info));

        let queue_indices =
            [device.h264_encode_queue.idx, device.queues.wgpu.idx].map(|i| i as u32);

        let create_info = vk::ImageCreateInfo::default()
            .flags(vk::ImageCreateFlags::MUTABLE_FORMAT | vk::ImageCreateFlags::EXTENDED_USAGE)
            .image_type(vk::ImageType::TYPE_2D)
            .format(vk::Format::G8_B8R8_2PLANE_420_UNORM)
            .extent(extent)
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(
                vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::VIDEO_ENCODE_SRC_KHR,
            )
            .sharing_mode(vk::SharingMode::CONCURRENT)
            .queue_family_indices(&queue_indices)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .push_next(&mut profile_list_info);

        let mut image = Image::new(device.allocator.clone(), &create_info)?;

        image.transition_layout_single_layer(
            command_buffer,
            vk::PipelineStageFlags2::NONE..vk::PipelineStageFlags2::NONE,
            vk::AccessFlags2::NONE..vk::AccessFlags2::NONE,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            0,
        )?;

        let image = Arc::new(Mutex::new(image));

        let module =
            wgpu::naga::front::wgsl::parse_str(include_str!("../shaders/rgba_to_yuv.wgsl"))
                .unwrap();
        let mut validator = wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        );

        validator
            .subgroup_stages(wgpu::naga::valid::ShaderStages::all())
            .subgroup_operations(wgpu::naga::valid::SubgroupOperationSet::all());

        let module_info = validator.validate(&module).unwrap();

        let compiled_vertex = wgpu::naga::back::spv::write_vec(
            &module,
            &module_info,
            &wgpu::naga::back::spv::Options::default(),
            Some(&wgpu::naga::back::spv::PipelineOptions {
                entry_point: "vs_main".into(),
                shader_stage: wgpu::naga::ShaderStage::Vertex,
            }),
        )
        .unwrap();

        let compiled_fragment_y = wgpu::naga::back::spv::write_vec(
            &module,
            &module_info,
            &wgpu::naga::back::spv::Options::default(),
            Some(&wgpu::naga::back::spv::PipelineOptions {
                entry_point: "fs_main_y".into(),
                shader_stage: wgpu::naga::ShaderStage::Fragment,
            }),
        )
        .unwrap();

        let compiled_fragment_uv = wgpu::naga::back::spv::write_vec(
            &module,
            &module_info,
            &wgpu::naga::back::spv::Options::default(),
            Some(&wgpu::naga::back::spv::PipelineOptions {
                entry_point: "fs_main_uv".into(),
                shader_stage: wgpu::naga::ShaderStage::Fragment,
            }),
        )
        .unwrap();

        let common_state = Arc::new(CommonState::new(device.vulkan_device.clone())?);

        let pipeline_y = ConvertingPipeline::new(
            device.vulkan_device.clone(),
            ShaderInfo {
                entry_point: c"vs_main",
                compiled_shader: compiled_vertex.clone(),
            },
            ShaderInfo {
                entry_point: c"fs_main_y",
                compiled_shader: compiled_fragment_y,
            },
            common_state.clone(),
            image.clone(),
            vk::Format::R8_UNORM,
        )?;

        let pipeline_uv = ConvertingPipeline::new(
            device.vulkan_device.clone(),
            ShaderInfo {
                entry_point: c"vs_main",
                compiled_shader: compiled_vertex.clone(),
            },
            ShaderInfo {
                entry_point: c"fs_main_uv",
                compiled_shader: compiled_fragment_uv,
            },
            common_state.clone(),
            image.clone(),
            vk::Format::R8G8_UNORM,
        )?;

        let command_buffer = unsafe { command_encoder.end_encoding()? };

        unsafe {
            device.wgpu_queue().as_hal::<VkApi, _, _>(|q| {
                q.unwrap().submit(&[&command_buffer], &[], (&mut fence, 1))
            })?
        };

        let mut done = false;
        while !done {
            done = unsafe {
                device
                    .wgpu_device()
                    .as_hal::<VkApi, _, _>(|d| d.unwrap().wait(&fence, 1, u32::MAX))?
            }
        }

        unsafe {
            device
                .wgpu_device()
                .as_hal::<VkApi, _, _>(|d| d.unwrap().destroy_fence(fence));
        }

        Ok(Self {
            device: device.vulkan_device.clone(),
            image,
            pipeline_y,
            pipeline_uv,
        })
    }

    /// The returned image is NV12 with color attachment layout
    ///
    /// # Safety
    /// - The texture can not be a surface texture
    /// - The texture has to be transitioned to [`wgpu::TextureUses::RESOURCE`] usage
    pub(crate) unsafe fn convert(
        &self,
        texture: wgpu::Texture,
    ) -> Result<ConvertState, YuvConverterError> {
        let mut command_encoder = unsafe {
            self.device.wgpu_device().as_hal::<VkApi, _, _>(|d| {
                self.device.wgpu_queue().as_hal::<VkApi, _, _>(|q| {
                    d.unwrap()
                        .create_command_encoder(&wgpu::hal::CommandEncoderDescriptor {
                            label: None,
                            queue: q.unwrap(),
                        })
                })
            })?
        };

        let image = unsafe {
            texture.as_hal::<VkApi, _, _>(|t| {
                let t = t.unwrap();
                t.raw_handle()
            })
        };

        let view_create_info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(vk::Format::R8G8B8A8_UNORM)
            .components(vk::ComponentMapping::default())
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                level_count: 1,
                base_mip_level: 0,
                layer_count: 1,
                base_array_layer: 0,
            });

        let view = ImageView::new(
            self.device.device.clone(),
            self.image.clone(),
            &view_create_info,
        )?;

        unsafe { command_encoder.begin_encoding(None)? };
        let command_buffer = unsafe { command_encoder.raw_handle() };

        self.pipeline_y.convert(command_buffer, &view);
        self.pipeline_uv.convert(command_buffer, &view);

        let wgpu_command_buffer = unsafe { command_encoder.end_encoding()? };

        let mut fence = unsafe {
            self.device
                .wgpu_device()
                .as_hal::<VkApi, _, _>(|d| d.unwrap().create_fence())?
        };

        unsafe {
            self.device.wgpu_queue().as_hal::<VkApi, _, _>(|q| {
                q.unwrap()
                    .submit(&[&wgpu_command_buffer], &[], (&mut fence, 1))
            })?;
        }

        Ok(ConvertState {
            image: self.image.clone(),
            _view: view,
            fence,
            _encoder: command_encoder,
            _buffer: wgpu_command_buffer,
        })
    }
}

pub(crate) struct ConvertState {
    _encoder: wgpu::hal::vulkan::CommandEncoder,
    _buffer: wgpu::hal::vulkan::CommandBuffer,
    pub(crate) fence: wgpu::hal::vulkan::Fence,
    pub(crate) image: Arc<Mutex<Image>>,
    pub(crate) _view: ImageView,
}

struct ShaderInfo {
    compiled_shader: Vec<u32>,
    entry_point: &'static std::ffi::CStr,
}

struct ConvertingPipeline {
    pipeline: Pipeline,
    render_pass: Arc<RenderPass>,
    framebuffer: Framebuffer,
    texture_descriptor_set: vk::DescriptorSet,
    extent: vk::Extent3D,
    common_state: Arc<CommonState>,
    device: Arc<VulkanDevice>,
}

impl ConvertingPipeline {
    fn new(
        device: Arc<VulkanDevice>,
        vertex_info: ShaderInfo,
        fragment_info: ShaderInfo,
        common_state: Arc<CommonState>,
        image: Arc<Mutex<Image>>,
        format: vk::Format,
    ) -> Result<Self, YuvConverterError> {
        let vertex = ShaderModule::new(device.device.clone(), &vertex_info.compiled_shader)?;
        let fragment = ShaderModule::new(device.device.clone(), &fragment_info.compiled_shader)?;

        let vertex_stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vertex.module)
            .name(vertex_info.entry_point);

        let fragment_stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(fragment.module)
            .name(fragment_info.entry_point);

        let shader_stages = [vertex_stage, fragment_stage];

        let dynamic_states = [vk::DynamicState::SCISSOR, vk::DynamicState::VIEWPORT];
        let dynamic_state =
            vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

        let vertex_input_state = vk::PipelineVertexInputStateCreateInfo::default()
            .vertex_binding_descriptions(&[])
            .vertex_attribute_descriptions(&[]);

        let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
            .primitive_restart_enable(false);

        let viewport_state = vk::PipelineViewportStateCreateInfo::default()
            .viewport_count(1)
            .scissor_count(1);

        let rasterizer_state = vk::PipelineRasterizationStateCreateInfo::default()
            .depth_clamp_enable(false)
            .rasterizer_discard_enable(false)
            .polygon_mode(vk::PolygonMode::FILL)
            .line_width(1.0)
            .cull_mode(vk::CullModeFlags::BACK)
            .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
            .depth_bias_enable(false);

        let multisample_state = vk::PipelineMultisampleStateCreateInfo::default()
            .sample_shading_enable(false)
            .rasterization_samples(vk::SampleCountFlags::TYPE_1);

        let color_blend = [vk::PipelineColorBlendAttachmentState::default()
            .color_write_mask(vk::ColorComponentFlags::RGBA)
            .blend_enable(false)];

        let color_blend_state = vk::PipelineColorBlendStateCreateInfo::default()
            .logic_op_enable(false)
            .attachments(&color_blend);

        let attachment_desc = vk::AttachmentDescription {
            format,
            flags: vk::AttachmentDescriptionFlags::empty(),
            samples: vk::SampleCountFlags::TYPE_1,
            load_op: vk::AttachmentLoadOp::DONT_CARE,
            store_op: vk::AttachmentStoreOp::STORE,
            stencil_load_op: vk::AttachmentLoadOp::DONT_CARE,
            stencil_store_op: vk::AttachmentStoreOp::DONT_CARE,
            initial_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            final_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        };

        let out_attachment_ref = vk::AttachmentReference {
            attachment: 0,
            layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        };

        let subpass_description = vk::SubpassDescription::default()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(std::slice::from_ref(&out_attachment_ref));

        let render_pass_info = vk::RenderPassCreateInfo::default()
            .attachments(std::slice::from_ref(&attachment_desc))
            .subpasses(std::slice::from_ref(&subpass_description));

        let render_pass = Arc::new(RenderPass::new(device.device.clone(), &render_pass_info)?);

        let pipeline_create_info = vk::GraphicsPipelineCreateInfo::default()
            .stages(&shader_stages)
            .vertex_input_state(&vertex_input_state)
            .input_assembly_state(&input_assembly)
            .viewport_state(&viewport_state)
            .rasterization_state(&rasterizer_state)
            .multisample_state(&multisample_state)
            .color_blend_state(&color_blend_state)
            .dynamic_state(&dynamic_state)
            .layout(common_state.pipeline_layout.pipeline_layout)
            .render_pass(render_pass.render_pass)
            .base_pipeline_index(-1)
            .base_pipeline_handle(vk::Pipeline::null());

        let pipeline = Pipeline::new(
            device.device.clone(),
            render_pass.clone(),
            common_state.pipeline_layout.clone(),
            &pipeline_create_info,
        )?;

        let mut view_usage_info =
            vk::ImageViewUsageCreateInfo::default().usage(vk::ImageUsageFlags::COLOR_ATTACHMENT);

        let aspect = match format {
            vk::Format::R8_UNORM => vk::ImageAspectFlags::PLANE_0,
            vk::Format::R8G8_UNORM => vk::ImageAspectFlags::PLANE_1,
            _ => panic!("unexpected format when creating pipeline"),
        };

        let view_info = vk::ImageViewCreateInfo::default()
            .flags(vk::ImageViewCreateFlags::empty())
            .image(image.lock().unwrap().image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .components(vk::ComponentMapping::default())
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: aspect,
                base_array_layer: 0,
                level_count: 1,
                base_mip_level: 0,
                layer_count: 1,
            })
            .push_next(&mut view_usage_info);

        let view = Arc::new(ImageView::new(
            device.device.clone(),
            image.clone(),
            &view_info,
        )?);

        let extent = image.lock().unwrap().extent;

        let extent = match format {
            vk::Format::R8_UNORM => extent,
            vk::Format::R8G8_UNORM => vk::Extent3D {
                width: extent.width / 2,
                height: extent.height / 2,
                depth: 1,
            },
            _ => panic!("unexpected format when creating pipeline"),
        };

        let framebuffer_info = vk::FramebufferCreateInfo::default()
            .render_pass(render_pass.render_pass)
            .attachments(std::slice::from_ref(&view.view))
            .width(extent.width)
            .height(extent.height)
            .layers(1);

        let framebuffer = Framebuffer::new(
            device.device.clone(),
            render_pass.clone(),
            vec![view.clone()],
            &framebuffer_info,
        )?;

        let allocate_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(common_state.descriptor_pool.pool)
            .set_layouts(std::slice::from_ref(
                &common_state
                    .descriptor_set_layout_sampled_texture
                    .set_layout,
            ));

        let descriptor_sets = unsafe {
            device
                .device
                .allocate_descriptor_sets(&allocate_info)
                .map_err(VulkanCommonError::from)?
        };

        let texture_descriptor_set = descriptor_sets[0];

        Ok(Self {
            pipeline,
            render_pass,
            framebuffer,
            texture_descriptor_set,
            device,
            extent,
            common_state,
        })
    }

    fn convert(&self, command_buffer: vk::CommandBuffer, view: &ImageView) {
        let descriptor_image_info = vk::DescriptorImageInfo {
            image_view: view.view,
            image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            ..Default::default()
        };

        let write_descriptor_set = vk::WriteDescriptorSet::default()
            .dst_set(self.texture_descriptor_set)
            .dst_binding(0)
            .dst_array_element(0)
            .descriptor_count(1)
            .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
            .image_info(std::slice::from_ref(&descriptor_image_info));

        unsafe {
            self.device
                .device
                .update_descriptor_sets(&[write_descriptor_set], &[])
        };

        let render_pass_info = vk::RenderPassBeginInfo::default()
            .render_pass(self.render_pass.render_pass)
            .framebuffer(self.framebuffer.framebuffer)
            .render_area(vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: vk::Extent2D {
                    width: self.extent.width,
                    height: self.extent.height,
                },
            });

        unsafe {
            self.device.device.cmd_begin_render_pass(
                command_buffer,
                &render_pass_info,
                vk::SubpassContents::INLINE,
            );

            self.device.device.cmd_bind_pipeline(
                command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline.pipeline,
            );

            self.device.device.cmd_bind_descriptor_sets(
                command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.common_state.pipeline_layout.pipeline_layout,
                0,
                &[
                    self.texture_descriptor_set,
                    self.common_state.sampler_descriptor_set,
                ],
                &[],
            );

            let viewport = vk::Viewport {
                x: 0.0,
                y: 0.0,
                width: self.extent.width as f32,
                height: self.extent.height as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            };

            self.device
                .device
                .cmd_set_viewport(command_buffer, 0, &[viewport]);

            let scissor = vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: vk::Extent2D {
                    width: self.extent.width,
                    height: self.extent.height,
                },
            };

            self.device
                .device
                .cmd_set_scissor(command_buffer, 0, &[scissor]);

            self.device.device.cmd_draw(command_buffer, 3, 1, 0, 0);

            self.device.device.cmd_end_render_pass(command_buffer);
        }
    }
}

struct CommonState {
    pipeline_layout: Arc<PipelineLayout>,
    descriptor_set_layout_sampled_texture: DescriptorSetLayout,
    _descriptor_set_layout_sampler: DescriptorSetLayout,
    _sampler: Sampler,
    sampler_descriptor_set: vk::DescriptorSet,
    descriptor_pool: DescriptorPool,
}

impl CommonState {
    fn new(device: Arc<VulkanDevice>) -> Result<Self, YuvConverterError> {
        let bindings = [vk::DescriptorSetLayoutBinding {
            binding: 0,
            stage_flags: vk::ShaderStageFlags::FRAGMENT,
            descriptor_type: vk::DescriptorType::SAMPLED_IMAGE,
            descriptor_count: 1,
            ..Default::default()
        }];

        let create_info = vk::DescriptorSetLayoutCreateInfo::default()
            .flags(vk::DescriptorSetLayoutCreateFlags::empty())
            .bindings(&bindings);

        let descriptor_set_layout_sampled_texture =
            DescriptorSetLayout::new(device.device.clone(), &create_info)?;

        let bindings = [vk::DescriptorSetLayoutBinding {
            binding: 0,
            stage_flags: vk::ShaderStageFlags::FRAGMENT,
            descriptor_type: vk::DescriptorType::SAMPLER,
            descriptor_count: 1,
            ..Default::default()
        }];

        let create_info = vk::DescriptorSetLayoutCreateInfo::default()
            .flags(vk::DescriptorSetLayoutCreateFlags::empty())
            .bindings(&bindings);

        let descriptor_set_layout_sampler =
            DescriptorSetLayout::new(device.device.clone(), &create_info)?;

        let set_layouts = [
            descriptor_set_layout_sampled_texture.set_layout,
            descriptor_set_layout_sampler.set_layout,
        ];

        let create_info = vk::PipelineLayoutCreateInfo::default()
            .flags(vk::PipelineLayoutCreateFlags::empty())
            .set_layouts(&set_layouts);

        let pipeline_layout = Arc::new(PipelineLayout::new(device.device.clone(), &create_info)?);

        let sampler_info = vk::SamplerCreateInfo::default()
            .flags(vk::SamplerCreateFlags::empty())
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .address_mode_u(vk::SamplerAddressMode::REPEAT)
            .address_mode_v(vk::SamplerAddressMode::REPEAT)
            .address_mode_w(vk::SamplerAddressMode::REPEAT)
            .anisotropy_enable(false)
            .max_anisotropy(1.0)
            .border_color(vk::BorderColor::INT_OPAQUE_BLACK)
            .unnormalized_coordinates(false)
            .compare_enable(false)
            .compare_op(vk::CompareOp::ALWAYS)
            .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
            .mip_lod_bias(0.0)
            .min_lod(0.0)
            .max_lod(0.0);

        let sampler = Sampler::new(device.device.clone(), &sampler_info)?;

        let pool_sizes = [
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::SAMPLER,
                descriptor_count: 1,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::SAMPLED_IMAGE,
                descriptor_count: 2,
            },
        ];

        let descriptor_pool_create_info = vk::DescriptorPoolCreateInfo::default()
            .flags(vk::DescriptorPoolCreateFlags::empty())
            .pool_sizes(&pool_sizes)
            .max_sets(3);

        let descriptor_pool =
            DescriptorPool::new(device.device.clone(), &descriptor_pool_create_info)?;

        let allocate_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(descriptor_pool.pool)
            .set_layouts(std::slice::from_ref(
                &descriptor_set_layout_sampler.set_layout,
            ));

        let descriptor_sets = unsafe {
            device
                .device
                .allocate_descriptor_sets(&allocate_info)
                .map_err(VulkanCommonError::from)?
        };

        let sampler_descriptor_set = descriptor_sets[0];

        let descriptor_image_info = vk::DescriptorImageInfo {
            sampler: sampler.sampler,
            ..Default::default()
        };

        let write_descriptor_set = vk::WriteDescriptorSet::default()
            .dst_set(sampler_descriptor_set)
            .dst_binding(0)
            .dst_array_element(0)
            .descriptor_count(1)
            .descriptor_type(vk::DescriptorType::SAMPLER)
            .image_info(std::slice::from_ref(&descriptor_image_info));

        unsafe {
            device
                .device
                .update_descriptor_sets(&[write_descriptor_set], &[])
        };

        Ok(Self {
            pipeline_layout,
            _descriptor_set_layout_sampler: descriptor_set_layout_sampler,
            descriptor_set_layout_sampled_texture,
            _sampler: sampler,
            descriptor_pool,
            sampler_descriptor_set,
        })
    }
}
