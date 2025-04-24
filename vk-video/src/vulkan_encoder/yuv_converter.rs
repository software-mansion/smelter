use std::sync::{Arc, Mutex};

use ash::vk;

use crate::{
    wrappers::{
        CommandBuffer, DescriptorSetLayout, Image, PipelineLayout, Semaphore, ShaderModule,
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
}

impl Converter {
    pub(crate) fn new(
        device: Arc<VulkanDevice>,
        width: u32,
        height: u32,
        profile: &H264EncodeProfileInfo,
        command_buffer: &CommandBuffer,
    ) -> Result<Self, YuvConverterError> {
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
            command_buffer,
            vk::PipelineStageFlags2::NONE..vk::PipelineStageFlags2::NONE,
            vk::AccessFlags2::NONE..vk::AccessFlags2::NONE,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            0,
        )?;

        let vk_image = image.image;
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

        let layout = PipelineLayout::new(device.device.clone(), &create_info)?;

        let dynamic_states = [vk::DynamicState::SCISSOR, vk::DynamicState::VIEWPORT];
        let dynamic_state =
            vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

        let vertex_input_state = vk::PipelineVertexInputStateCreateInfo::default()
            .vertex_binding_descriptions(&[])
            .vertex_attribute_descriptions(&[]);

        let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
            .primitive_restart_enable(false);

        let viewport = vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: width as f32,
            height: height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        };

        let scissor = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent: vk::Extent2D { width, height },
        };

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

        let pipeline_create_info = vk::GraphicsPipelineCreateInfo::default()
            .stages(&shader_stages)
            .layout(layout.pipeline_layout);
        // let pipeline = device.device.create_graphics_pipelines(pipeline_cache, create_infos, allocation_callbacks);

        Ok(Self { device, image })
    }

    /// The returned image is NV12 with encoding layout
    pub(crate) fn convert(
        &self,
        texture: wgpu::Texture,
        signal_semaphores: &[&Semaphore],
        profile: &H264EncodeProfileInfo,
    ) -> Result<Image, YuvConverterError> {
        todo!()
    }
}
