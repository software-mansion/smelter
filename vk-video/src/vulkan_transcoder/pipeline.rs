use std::sync::Arc;

use ash::vk;

use crate::{
    VulkanDevice,
    vulkan_decoder::{DecodeSubmission, DecoderTracker},
    vulkan_encoder::{EncoderTracker, EncoderTrackerWaitState, H264EncodeProfileInfo},
    vulkan_transcoder::TranscoderError,
    wrappers::{
        CommandBufferPool, ComputePipeline, DescriptorPool, DescriptorSet, DescriptorSetLayout,
        Image, ImageView, PipelineLayout, ShaderModule, TrackerWait,
    },
};

const MAX_OUTPUTS: usize = 8;

pub(crate) struct ResizingImageBundle {
    pub(crate) image: Arc<Image>,
    view_y: ImageView,
    view_uv: ImageView,
}

impl ResizingImageBundle {
    fn new(image: Arc<Image>) -> Result<Self, TranscoderError> {
        let view_y =
            image.create_plane_view(vk::ImageAspectFlags::PLANE_0, vk::ImageUsageFlags::STORAGE)?;
        let view_uv =
            image.create_plane_view(vk::ImageAspectFlags::PLANE_1, vk::ImageUsageFlags::STORAGE)?;

        Ok(Self {
            image,
            view_y,
            view_uv,
        })
    }
}

pub(crate) struct OutputConfig<'a> {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) tracker: &'a mut EncoderTracker,
    pub(crate) profile: &'a H264EncodeProfileInfo<'a>,
}

pub(crate) struct Pipeline {
    descriptor_set_input: DescriptorSet,
    descriptor_set_output_y: DescriptorSet,
    descriptor_set_output_uv: DescriptorSet,
    pipeline: ComputePipeline,
    buffer_pool: CommandBufferPool,
    device: Arc<VulkanDevice>,
}

impl Pipeline {
    pub(crate) fn new(
        device: Arc<VulkanDevice>,
        configs: &[OutputConfig<'_>],
    ) -> Result<Self, TranscoderError> {
        let pool_sizes = [vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::STORAGE_IMAGE)
            .descriptor_count(2 * MAX_OUTPUTS as u32 + 2)];
        let descriptor_pool = Arc::new(DescriptorPool::new(
            device.device.clone(),
            &vk::DescriptorPoolCreateInfo::default()
                .max_sets(3)
                .pool_sizes(&pool_sizes),
        )?);

        let bindings_input = [
            vk::DescriptorSetLayoutBinding::default()
                .descriptor_count(1)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .stage_flags(vk::ShaderStageFlags::COMPUTE)
                .binding(0)
                .descriptor_count(1),
            vk::DescriptorSetLayoutBinding::default()
                .descriptor_count(1)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .stage_flags(vk::ShaderStageFlags::COMPUTE)
                .binding(1)
                .descriptor_count(1),
        ];

        let layout_input = Arc::new(DescriptorSetLayout::new(
            device.device.clone(),
            &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings_input),
        )?);

        let descriptor_set_input = DescriptorSet::new(
            descriptor_pool.clone(),
            &vk::DescriptorSetAllocateInfo::default()
                .descriptor_pool(descriptor_pool.pool)
                .set_layouts(&[layout_input.set_layout]),
        )?
        .pop()
        .unwrap();

        let bindings_output = [vk::DescriptorSetLayoutBinding::default()
            .descriptor_count(MAX_OUTPUTS as u32)
            .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
            .binding(0)
            .stage_flags(vk::ShaderStageFlags::COMPUTE)];

        let flags = [vk::DescriptorBindingFlags::VARIABLE_DESCRIPTOR_COUNT_EXT];
        let mut binding_flags =
            vk::DescriptorSetLayoutBindingFlagsCreateInfo::default().binding_flags(&flags);

        let layout_output = Arc::new(DescriptorSetLayout::new(
            device.device.clone(),
            &vk::DescriptorSetLayoutCreateInfo::default()
                .bindings(&bindings_output)
                .push_next(&mut binding_flags),
        )?);

        let counts = [configs.len() as u32, configs.len() as u32];
        let mut count = vk::DescriptorSetVariableDescriptorCountAllocateInfo::default()
            .descriptor_counts(&counts);
        let mut descriptor_set_outputs = DescriptorSet::new(
            descriptor_pool.clone(),
            &vk::DescriptorSetAllocateInfo::default()
                .set_layouts(&[layout_output.set_layout, layout_output.set_layout])
                .push_next(&mut count),
        )?;

        let descriptor_set_output_uv = descriptor_set_outputs.pop().unwrap();
        let descriptor_set_output_y = descriptor_set_outputs.pop().unwrap();

        let layouts = [
            layout_input.set_layout,
            layout_output.set_layout,
            layout_output.set_layout,
        ];
        let push_constants = [vk::PushConstantRange::default()
            .size(4)
            .offset(0)
            .stage_flags(vk::ShaderStageFlags::COMPUTE)];
        let create_info = vk::PipelineLayoutCreateInfo::default()
            .set_layouts(&layouts)
            .push_constant_ranges(&push_constants);
        let pipeline_layout = Arc::new(PipelineLayout::new(
            device.device.clone(),
            &create_info,
            vec![layout_input.clone(), layout_output.clone()],
        )?);

        let mut front = wgpu::naga::front::wgsl::Frontend::new();
        let parsed = front.parse(include_str!("shader.wgsl")).unwrap();
        let mut validator = wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        );
        validator
            .subgroup_stages(wgpu::naga::valid::ShaderStages::COMPUTE)
            .subgroup_operations(wgpu::naga::valid::SubgroupOperationSet::all());
        let module_info = validator.validate(&parsed).unwrap();
        let compiled = wgpu::naga::back::spv::write_vec(
            &parsed,
            &module_info,
            &wgpu::naga::back::spv::Options {
                lang_version: (1, 6),
                ..Default::default()
            },
            Some(&wgpu::naga::back::spv::PipelineOptions {
                shader_stage: wgpu::naga::ShaderStage::Compute,
                entry_point: "main".into(),
            }),
        )
        .unwrap();
        let shader_module = Arc::new(ShaderModule::new(
            device.device.clone(),
            &vk::ShaderModuleCreateInfo::default().code(&compiled),
        )?);

        let shader = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::COMPUTE)
            .name(c"main")
            .module(shader_module.module);
        let create_info = vk::ComputePipelineCreateInfo::default()
            .stage(shader)
            .layout(pipeline_layout.layout);

        let pipeline = ComputePipeline::new(
            device.device.clone(),
            create_info,
            pipeline_layout,
            shader_module,
        )?;

        let buffer_pool = CommandBufferPool::new(device.clone(), device.queues.wgpu.family_index)?;

        Ok(Self {
            descriptor_set_input,
            descriptor_set_output_y,
            descriptor_set_output_uv,
            pipeline,
            buffer_pool,
            device,
        })
    }

    fn allocate_images(
        &self,
        configs: &[OutputConfig<'_>],
    ) -> Result<Vec<ResizingImageBundle>, TranscoderError> {
        let mut result = Vec::new();
        for config in configs {
            let mut profile_list_info = vk::VideoProfileListInfoKHR::default()
                .profiles(std::slice::from_ref(&config.profile.profile_info));
            let queue_indices = [
                self.device
                    .queues
                    .h264_encode
                    .as_ref()
                    .unwrap()
                    .family_index as u32,
                self.device.queues.wgpu.family_index as u32,
            ];
            let create_info = vk::ImageCreateInfo::default()
                .flags(vk::ImageCreateFlags::EXTENDED_USAGE | vk::ImageCreateFlags::MUTABLE_FORMAT)
                .image_type(vk::ImageType::TYPE_2D)
                .format(vk::Format::G8_B8R8_2PLANE_420_UNORM)
                .extent(vk::Extent3D {
                    width: config.width,
                    height: config.height,
                    depth: 1,
                })
                .mip_levels(1)
                .array_layers(1)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::VIDEO_ENCODE_SRC_KHR)
                .sharing_mode(vk::SharingMode::CONCURRENT)
                .queue_family_indices(&queue_indices)
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .push_next(&mut profile_list_info);

            let image = Arc::new(Image::new(
                self.device.allocator.clone(),
                &create_info,
                config.tracker.image_layout_tracker.clone(),
            )?);

            result.push(ResizingImageBundle::new(image)?);
        }

        Ok(result)
    }

    fn write_descriptors(
        &self,
        input: &ResizingImageBundle,
        outputs: &[ResizingImageBundle],
    ) -> Result<(), TranscoderError> {
        let image_info_input_y = vk::DescriptorImageInfo::default()
            .image_view(input.view_y.view)
            .image_layout(vk::ImageLayout::GENERAL);
        let image_info_input_uv = vk::DescriptorImageInfo::default()
            .image_view(input.view_uv.view)
            .image_layout(vk::ImageLayout::GENERAL);

        let (image_infos_output_y, image_infos_output_uv) = outputs
            .iter()
            .map(|bundle| {
                (
                    vk::DescriptorImageInfo::default()
                        .image_layout(vk::ImageLayout::GENERAL)
                        .image_view(bundle.view_y.view),
                    vk::DescriptorImageInfo::default()
                        .image_layout(vk::ImageLayout::GENERAL)
                        .image_view(bundle.view_uv.view),
                )
            })
            .unzip::<_, _, Vec<_>, Vec<_>>();

        let writes = [
            (
                self.descriptor_set_input.descriptor_set,
                std::slice::from_ref(&image_info_input_y),
                0,
            ),
            (
                self.descriptor_set_input.descriptor_set,
                std::slice::from_ref(&image_info_input_uv),
                1,
            ),
            (
                self.descriptor_set_output_y.descriptor_set,
                &image_infos_output_y,
                0,
            ),
            (
                self.descriptor_set_output_uv.descriptor_set,
                &image_infos_output_uv,
                0,
            ),
        ]
        .into_iter()
        .map(|(descriptor_set, image_infos, binding)| {
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(binding)
                .dst_array_element(0)
                .descriptor_count(image_infos.len() as u32)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .image_info(&image_infos)
        })
        .collect::<Vec<_>>();
        unsafe { self.device.device.update_descriptor_sets(&writes, &[]) };

        Ok(())
    }

    pub(crate) fn run(
        &self,
        input_submission: &DecodeSubmission,
        decode_tracker: &mut DecoderTracker,
        configs: &mut [OutputConfig<'_>],
    ) -> Result<Vec<ResizingImageBundle>, TranscoderError> {
        let input = ResizingImageBundle::new(input_submission.image.clone())?;
        let outputs = self.allocate_images(configs)?;
        self.write_descriptors(&input, &outputs)?;

        let mut buffer = self.buffer_pool.begin_buffer()?;
        input.image.transition_layout_single_layer(
            &mut buffer,
            vk::PipelineStageFlags2::NONE..vk::PipelineStageFlags2::COMPUTE_SHADER,
            vk::AccessFlags2::NONE..vk::AccessFlags2::SHADER_STORAGE_READ,
            vk::ImageLayout::GENERAL,
            0,
        )?;
        for bundle in outputs.iter() {
            bundle.image.transition_layout_single_layer(
                &mut buffer,
                vk::PipelineStageFlags2::NONE..vk::PipelineStageFlags2::COMPUTE_SHADER,
                vk::AccessFlags2::NONE..vk::AccessFlags2::SHADER_STORAGE_WRITE,
                vk::ImageLayout::GENERAL,
                0,
            )?;
        }

        // TODO: move this and make this better
        let dispatch_size = outputs
            .iter()
            .map(|&ResizingImageBundle { ref image, .. }| {
                image.extent.width * image.extent.height * 3 / 2
            })
            .sum::<u32>();

        unsafe {
            self.device.device.cmd_bind_pipeline(
                buffer.buffer(),
                vk::PipelineBindPoint::COMPUTE,
                self.pipeline.pipeline,
            );
            self.device.device.cmd_bind_descriptor_sets(
                buffer.buffer(),
                vk::PipelineBindPoint::COMPUTE,
                self.pipeline.layout.layout,
                0,
                &[
                    self.descriptor_set_input.descriptor_set,
                    self.descriptor_set_output_y.descriptor_set,
                    self.descriptor_set_output_uv.descriptor_set,
                ],
                &[],
            );
            self.device.device.cmd_push_constants(
                buffer.buffer(),
                self.pipeline.layout.layout,
                vk::ShaderStageFlags::COMPUTE,
                0,
                &(outputs.len() as u32).to_ne_bytes(),
            );
            self.device
                .device
                .cmd_dispatch(buffer.buffer(), dispatch_size.div_ceil(256), 1, 1);
        }

        let buffer = buffer.end()?;
        let buffer_info = vk::CommandBufferSubmitInfo::default().command_buffer(buffer.buffer());

        let (waits, signals) = configs
            .iter_mut()
            .map(|OutputConfig { tracker, .. }| {
                let value = tracker.semaphore_tracker.next_sem_value();
                let submit_info = vk::SemaphoreSubmitInfo::default()
                    .stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
                    .semaphore(tracker.semaphore_tracker.semaphore.semaphore)
                    .value(value.0);
                let wait = tracker.semaphore_tracker.wait_for.take().map(|wait| {
                    vk::SemaphoreSubmitInfo::default()
                        .value(wait.value.0)
                        .semaphore(tracker.semaphore_tracker.semaphore.semaphore)
                        .stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
                });
                (wait, (submit_info, value))
            })
            .unzip::<_, _, Vec<_>, Vec<_>>();

        let (signals, values) = signals.into_iter().unzip::<_, _, Vec<_>, Vec<_>>();

        let mut waits = waits.into_iter().flatten().collect::<Vec<_>>();
        waits.push(
            vk::SemaphoreSubmitInfo::default()
                .stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
                .value(input_submission.semaphore_wait_value.0)
                .semaphore(decode_tracker.semaphore_tracker.semaphore.semaphore),
        );
        let submit_info = vk::SubmitInfo2::default()
            .command_buffer_infos(std::slice::from_ref(&buffer_info))
            .wait_semaphore_infos(&waits)
            .signal_semaphore_infos(&signals);

        unsafe {
            self.device.device.queue_submit2(
                *self.device.queues.wgpu.queue.lock().unwrap(),
                &[submit_info],
                vk::Fence::null(),
            )?;
        }

        buffer.mark_submitted_no_wait_value();
        configs.iter_mut().zip(values).for_each(|(config, value)| {
            config.tracker.semaphore_tracker.wait_for = Some(TrackerWait {
                value,
                _state: EncoderTrackerWaitState::ResizeInput,
            });
        });

        Ok(outputs)
    }

    pub(crate) fn mark_command_buffers_completed(&self) {
        self.buffer_pool.mark_all_submitted_as_free();
    }
}
