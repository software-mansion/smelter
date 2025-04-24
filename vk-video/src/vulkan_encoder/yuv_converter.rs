use std::sync::{Arc, Mutex};

use ash::vk;

use crate::{
    wrappers::{
        CommandBuffer, CommandPool, DescriptorPool, DescriptorSetLayout, Fence, Framebuffer, Image,
        ImageView, Pipeline, PipelineLayout, RenderPass, Sampler, Semaphore, ShaderModule,
    },
    VulkanCtxError, VulkanDevice,
};

use super::H264EncodeProfileInfo;

#[derive(Debug, thiserror::Error)]
pub enum YuvConverterError {
    #[error(transparent)]
    VulkanCtxError(#[from] VulkanCtxError),
}

pub(crate) struct Converter {
    device: Arc<VulkanDevice>,
    image: Arc<Mutex<Image>>,
    pipeline: Pipeline,
    pipeline_layout: Arc<PipelineLayout>,
    _command_pool: Arc<CommandPool>,
    command_buffer: CommandBuffer,
    view_y: Arc<ImageView>,
    view_uv: Arc<ImageView>,
    render_pass: Arc<RenderPass>,
    framebuffer_y: Framebuffer,
    width: u32,
    height: u32,
    sampler: Sampler,
    texture_descriptor_set: vk::DescriptorSet,
    sampler_descriptor_set: vk::DescriptorSet,
}

impl Converter {
    pub(crate) fn new(
        device: Arc<VulkanDevice>,
        width: u32,
        height: u32,
        profile: &H264EncodeProfileInfo,
    ) -> Result<Self, YuvConverterError> {
        // TODO: this means we use the same queue wgpu does. queue submits cannot be made at the
        // same time.
        let command_pool = Arc::new(CommandPool::new(device.clone(), device.queues.wgpu.idx)?);

        let command_buffer = CommandBuffer::new_primary(command_pool.clone())?;

        command_buffer.begin()?;

        let extent = vk::Extent3D {
            width,
            height,
            depth: 1,
        };

        let mut profile_list_info = vk::VideoProfileListInfoKHR::default()
            .profiles(std::slice::from_ref(&profile.profile_info));

        let queue_indices =
            [device.queues.h264_encode.idx, device.queues.wgpu.idx].map(|i| i as u32);

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
            &command_buffer,
            vk::PipelineStageFlags2::NONE..vk::PipelineStageFlags2::NONE,
            vk::AccessFlags2::NONE..vk::AccessFlags2::NONE,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            0,
        )?;

        let vk_image = image.image;
        let image = Arc::new(Mutex::new(image));

        let view_y_info = vk::ImageViewCreateInfo::default()
            .flags(vk::ImageViewCreateFlags::empty())
            .image(vk_image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(vk::Format::R8_UNORM)
            .components(vk::ComponentMapping::default())
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::PLANE_0,
                base_array_layer: 0,
                level_count: 1,
                base_mip_level: 0,
                layer_count: 1,
            });

        let view_y = Arc::new(ImageView::new(
            device.device.clone(),
            image.clone(),
            &view_y_info,
        )?);

        let view_uv_info = view_y_info
            .format(vk::Format::R8G8_UNORM)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::PLANE_1,
                ..view_y_info.subresource_range
            });

        let view_uv = Arc::new(ImageView::new(
            device.device.clone(),
            image.clone(),
            &view_uv_info,
        )?);

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

        let compiled_fragment = wgpu::naga::back::spv::write_vec(
            &module,
            &module_info,
            &wgpu::naga::back::spv::Options::default(),
            Some(&wgpu::naga::back::spv::PipelineOptions {
                entry_point: "fs_main_y".into(),
                shader_stage: wgpu::naga::ShaderStage::Fragment,
            }),
        )
        .unwrap();

        let vertex = ShaderModule::new(device.device.clone(), &compiled_vertex)?;
        let fragment = ShaderModule::new(device.device.clone(), &compiled_fragment)?;

        let vertex_stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vertex.module)
            .name(c"vs_main");

        let fragment_stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(fragment.module)
            .name(c"fs_main_y");

        let shader_stages = [vertex_stage, fragment_stage];

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

        let layout = Arc::new(PipelineLayout::new(device.device.clone(), &create_info)?);

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
            format: vk::Format::R8_UNORM,
            flags: vk::AttachmentDescriptionFlags::empty(),
            samples: vk::SampleCountFlags::TYPE_1,
            load_op: vk::AttachmentLoadOp::DONT_CARE,
            store_op: vk::AttachmentStoreOp::STORE,
            stencil_load_op: vk::AttachmentLoadOp::DONT_CARE,
            stencil_store_op: vk::AttachmentStoreOp::DONT_CARE,
            initial_layout: vk::ImageLayout::ATTACHMENT_OPTIMAL,
            final_layout: vk::ImageLayout::ATTACHMENT_OPTIMAL,
        };

        let out_attachment_ref = vk::AttachmentReference {
            attachment: 0,
            layout: vk::ImageLayout::ATTACHMENT_OPTIMAL,
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
            .layout(layout.pipeline_layout)
            .render_pass(render_pass.render_pass)
            .base_pipeline_index(-1)
            .base_pipeline_handle(vk::Pipeline::null());

        let pipeline = Pipeline::new(
            device.device.clone(),
            render_pass.clone(),
            layout.clone(),
            &pipeline_create_info,
        )?;

        let framebuffer_y_info = vk::FramebufferCreateInfo::default()
            .render_pass(render_pass.render_pass)
            .attachments(std::slice::from_ref(&view_y.view))
            .width(width)
            .height(height)
            .layers(1);

        let framebuffer_y = Framebuffer::new(
            device.device.clone(),
            render_pass.clone(),
            vec![view_y.clone()],
            &framebuffer_y_info,
        )?;

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
                descriptor_count: 1,
            },
        ];

        let descriptor_pool_create_info = vk::DescriptorPoolCreateInfo::default()
            .flags(vk::DescriptorPoolCreateFlags::FREE_DESCRIPTOR_SET)
            .pool_sizes(&pool_sizes)
            .max_sets(2);

        let descriptor_pool =
            DescriptorPool::new(device.device.clone(), &descriptor_pool_create_info)?;

        // add descriptors

        let allocate_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(descriptor_pool.pool)
            .set_layouts(&set_layouts);

        let descriptor_sets = unsafe {
            device
                .device
                .allocate_descriptor_sets(&allocate_info)
                .map_err(VulkanCtxError::from)?
        };

        let texture_descriptor_set = descriptor_sets[0];
        let sampler_descriptor_set = descriptor_sets[1];

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

        let fence = Fence::new(device.device.clone(), false)?;

        command_buffer.end()?;

        device
            .queues
            .wgpu
            .submit(&command_buffer, &[], &[], Some(fence.fence))?;

        fence.wait_and_reset(u64::MAX)?;

        Ok(Self {
            command_buffer,
            _command_pool: command_pool,
            device,
            image,
            pipeline,
            view_y,
            view_uv,
            render_pass,
            framebuffer_y,
            width,
            height,
            sampler,
            texture_descriptor_set,
            sampler_descriptor_set,
            pipeline_layout: layout,
        })
    }

    /// The returned image is NV12 with encoding layout
    pub(crate) fn convert(
        &self,
        texture: wgpu::Texture,
        wait_semaphores: &[(vk::Semaphore, vk::PipelineStageFlags2)],
        signal_semaphores: &[(vk::Semaphore, vk::PipelineStageFlags2)],
    ) -> Result<Arc<Mutex<Image>>, YuvConverterError> {
        let image = unsafe {
            texture.as_hal::<wgpu::hal::vulkan::Api, _, _>(|t| {
                let t = t.unwrap();
                t.raw_handle()
            })
        };

        let view_create_info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_1D)
            .format(vk::Format::R8G8B8A8_UNORM)
            .components(vk::ComponentMapping::default())
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                level_count: 1,
                base_mip_level: 0,
                layer_count: 1,
                base_array_layer: 0,
            });

        let view = unsafe {
            self.device
                .device
                .create_image_view(&view_create_info, None)
                .map_err(VulkanCtxError::from)?
        };

        let descriptor_image_info = vk::DescriptorImageInfo {
            image_view: view,
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

        self.command_buffer.begin()?;

        let render_pass_info = vk::RenderPassBeginInfo::default()
            .render_pass(self.render_pass.render_pass)
            .framebuffer(self.framebuffer_y.framebuffer)
            .render_area(vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: vk::Extent2D {
                    width: self.width,
                    height: self.height,
                },
            });

        unsafe {
            self.device.device.cmd_begin_render_pass(
                *self.command_buffer,
                &render_pass_info,
                vk::SubpassContents::INLINE,
            );

            self.device.device.cmd_bind_pipeline(
                *self.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline.pipeline,
            );

            self.device.device.cmd_bind_descriptor_sets(
                *self.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline_layout.pipeline_layout,
                0,
                &[self.texture_descriptor_set, self.sampler_descriptor_set],
                &[],
            );

            let viewport = vk::Viewport {
                x: 0.0,
                y: 0.0,
                width: self.width as f32,
                height: self.height as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            };

            self.device
                .device
                .cmd_set_viewport(*self.command_buffer, 0, &[viewport]);

            let scissor = vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: vk::Extent2D {
                    width: self.width,
                    height: self.height,
                },
            };

            self.device
                .device
                .cmd_set_scissor(*self.command_buffer, 0, &[scissor]);

            self.device
                .device
                .cmd_draw(*self.command_buffer, 3, 1, 0, 0);

            self.device.device.cmd_end_render_pass(*self.command_buffer);

            self.image.lock().unwrap().transition_layout_single_layer(
                &self.command_buffer,
                vk::PipelineStageFlags2::FRAGMENT_SHADER..vk::PipelineStageFlags2::VIDEO_ENCODE_KHR,
                vk::AccessFlags2::SHADER_WRITE..vk::AccessFlags2::VIDEO_ENCODE_READ_KHR,
                vk::ImageLayout::VIDEO_ENCODE_SRC_KHR,
                0,
            )?;
        }

        self.command_buffer.end()?;

        self.device.queues.wgpu.submit(
            &self.command_buffer,
            wait_semaphores,
            signal_semaphores,
            None,
        )?;

        unsafe { self.device.device.destroy_image_view(view, None) };

        Ok(self.image.clone())
    }
}
