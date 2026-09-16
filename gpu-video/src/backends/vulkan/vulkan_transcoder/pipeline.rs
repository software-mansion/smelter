use std::{
    io::Cursor,
    sync::{Arc, Mutex, Weak},
};

use ash::vk;

use crate::{
    backends::vulkan::{
        VulkanDevice,
        vulkan_decoder::{DecodeSubmission, DecoderTrackerWaitState},
        vulkan_encoder::{EncoderTrackerWaitState, async_encoder::DynVulkanEncoder},
        vulkan_transcoder::VulkanTranscoderError,
        wrappers::{
            CommandBufferPool, ComputePipeline, DescriptorPool, DescriptorSet, DescriptorSetLayout,
            EncodeInputImage, Image, ImageView, PipelineLayout, SemaphoreWaitValue, ShaderModule,
            TimelineSemaphore,
        },
    },
    parameters::ScalingAlgorithm,
};

const MAX_OUTPUTS: u32 = 8;

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct PushConstants {
    output_count: u32,
    width: u32,
    height: u32,
    scaling_algorithm: [u32; MAX_OUTPUTS as usize],
}

impl PushConstants {
    fn new(output_configs: &[OutputConfig], cropped_size: vk::Extent2D) -> Self {
        let mut result = PushConstants {
            output_count: output_configs.len() as u32,
            width: cropped_size.width,
            height: cropped_size.height,
            scaling_algorithm: [0; _],
        };

        for (i, config) in output_configs.iter().enumerate() {
            result.scaling_algorithm[i] = config.scaling_algorithm as u32;
        }

        result
    }
}

pub(crate) struct PlaneViews {
    pub(crate) view_y: ImageView,
    pub(crate) view_uv: ImageView,
}

impl PlaneViews {
    fn new(image: Arc<Image>, layer: u32) -> Result<Self, VulkanTranscoderError> {
        let view_y = image.create_plane_view(
            layer,
            vk::ImageAspectFlags::PLANE_0,
            vk::ImageUsageFlags::STORAGE,
        )?;
        let view_uv = image.create_plane_view(
            layer,
            vk::ImageAspectFlags::PLANE_1,
            vk::ImageUsageFlags::STORAGE,
        )?;

        Ok(Self { view_y, view_uv })
    }
}

pub(crate) struct InFlightResizeResources {
    pub(crate) _input_views: PlaneViews,
    pub(crate) _output_views: Vec<PlaneViews>,
    pub(crate) descriptors: Option<Descriptors>,
    pub(crate) _pipeline: Arc<ComputePipeline>,
    pub(crate) _encoder_semaphores: Vec<Arc<TimelineSemaphore>>,
}

impl Drop for InFlightResizeResources {
    fn drop(&mut self) {
        let Some(descriptors) = self.descriptors.take() else {
            return;
        };

        descriptors.release_to_pool();
    }
}

pub(crate) struct ResizeSubmission {
    pub(crate) outputs: Box<[EncodeInputImage]>,
    pub(crate) wait_value: SemaphoreWaitValue,
    pub(crate) in_flight_resources: InFlightResizeResources,
}

pub(crate) struct OutputConfig {
    pub(crate) scaling_algorithm: ScalingAlgorithm,
}

pub(crate) struct Descriptors {
    input: DescriptorSet,
    output_y: DescriptorSet,
    output_uv: DescriptorSet,

    freelist: Weak<Mutex<Vec<Descriptors>>>,
}

impl Descriptors {
    pub(crate) fn release_to_pool(self) {
        if let Some(freelist) = self.freelist.upgrade() {
            freelist.lock().unwrap().push(self);
        }
    }
}

struct DescriptorHeap {
    pool: Arc<DescriptorPool>,
    freelist: Arc<Mutex<Vec<Descriptors>>>,
    layout_input: Arc<DescriptorSetLayout>,
    layout_output: Arc<DescriptorSetLayout>,
}

impl DescriptorHeap {
    fn new(
        pool: Arc<DescriptorPool>,
        layout_input: Arc<DescriptorSetLayout>,
        layout_output: Arc<DescriptorSetLayout>,
    ) -> Self {
        Self {
            pool,
            freelist: Arc::new(Mutex::new(Vec::new())),
            layout_input,
            layout_output,
        }
    }

    fn allocate(&self) -> Result<Descriptors, VulkanTranscoderError> {
        if let Some(descriptors) = self.freelist.lock().unwrap().pop() {
            return Ok(descriptors);
        }

        let input = DescriptorSet::new(
            self.pool.clone(),
            &vk::DescriptorSetAllocateInfo::default()
                .descriptor_pool(self.pool.pool)
                .set_layouts(&[self.layout_input.set_layout]),
        )?
        .pop()
        .unwrap();

        let mut descriptor_set_outputs = DescriptorSet::new(
            self.pool.clone(),
            &vk::DescriptorSetAllocateInfo::default()
                .set_layouts(&[self.layout_output.set_layout, self.layout_output.set_layout]),
        )?;

        let output_uv = descriptor_set_outputs.pop().unwrap();
        let output_y = descriptor_set_outputs.pop().unwrap();

        Ok(Descriptors {
            input,
            output_y,
            output_uv,
            freelist: Arc::downgrade(&self.freelist),
        })
    }
}

pub(crate) struct ResizingPipeline {
    descriptor_heap: DescriptorHeap,
    pipeline: Arc<ComputePipeline>,
    pub(crate) buffer_pool: CommandBufferPool,
    configs: Vec<OutputConfig>,
    device: Arc<VulkanDevice>,
}

impl ResizingPipeline {
    pub(crate) fn new(
        device: Arc<VulkanDevice>,
        configs: Vec<OutputConfig>,
        max_in_flight: u32,
    ) -> Result<Self, VulkanTranscoderError> {
        if configs.is_empty() || configs.len() > MAX_OUTPUTS as usize {
            return Err(VulkanTranscoderError::WrongOutputNumber {
                expected_max: MAX_OUTPUTS as usize,
                actual: configs.len(),
            });
        }
        let pool_sizes = [vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::STORAGE_IMAGE)
            .descriptor_count((2 * MAX_OUTPUTS + 2) * max_in_flight.max(1))];
        let descriptor_pool = Arc::new(DescriptorPool::new(
            device.device.clone(),
            &vk::DescriptorPoolCreateInfo::default()
                .max_sets(3 * max_in_flight.max(1))
                .pool_sizes(&pool_sizes),
        )?);

        let bindings_input = [
            vk::DescriptorSetLayoutBinding::default()
                .descriptor_count(1)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .stage_flags(vk::ShaderStageFlags::COMPUTE)
                .binding(0),
            vk::DescriptorSetLayoutBinding::default()
                .descriptor_count(1)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .stage_flags(vk::ShaderStageFlags::COMPUTE)
                .binding(1),
        ];

        let layout_input = Arc::new(DescriptorSetLayout::new(
            device.device.clone(),
            &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings_input),
        )?);

        let bindings_output = [vk::DescriptorSetLayoutBinding::default()
            .descriptor_count(MAX_OUTPUTS)
            .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
            .binding(0)
            .stage_flags(vk::ShaderStageFlags::COMPUTE)];

        let flags = [vk::DescriptorBindingFlags::PARTIALLY_BOUND];
        let mut binding_flags =
            vk::DescriptorSetLayoutBindingFlagsCreateInfo::default().binding_flags(&flags);

        let layout_output = Arc::new(DescriptorSetLayout::new(
            device.device.clone(),
            &vk::DescriptorSetLayoutCreateInfo::default()
                .bindings(&bindings_output)
                .push_next(&mut binding_flags),
        )?);

        let descriptor_heap = DescriptorHeap::new(
            descriptor_pool.clone(),
            layout_input.clone(),
            layout_output.clone(),
        );

        let layouts = [
            layout_input.set_layout,
            layout_output.set_layout,
            layout_output.set_layout,
        ];
        let push_constants = [vk::PushConstantRange::default()
            .size(std::mem::size_of::<PushConstants>() as u32)
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

        const SHADER_SPV: &[u8] =
            include_bytes!(concat!(env!("OUT_DIR"), "/transcoding_shader.spv"));
        let mut shader_bytes_cursor = Cursor::new(SHADER_SPV);
        let compiled_shader = ash::util::read_spv(&mut shader_bytes_cursor).unwrap();

        let shader_module = Arc::new(ShaderModule::new(
            device.device.clone(),
            &vk::ShaderModuleCreateInfo::default().code(&compiled_shader),
        )?);

        let shader = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::COMPUTE)
            .name(c"main")
            .module(shader_module.module);
        let create_info = vk::ComputePipelineCreateInfo::default()
            .stage(shader)
            .layout(pipeline_layout.layout);

        let pipeline = Arc::new(ComputePipeline::new(
            device.device.clone(),
            create_info,
            pipeline_layout,
            shader_module,
        )?);

        let buffer_pool =
            CommandBufferPool::new(device.clone(), device.queues.compute.family_index)?;

        Ok(Self {
            descriptor_heap,
            pipeline,
            buffer_pool,
            configs,
            device,
        })
    }

    fn write_descriptors(
        &mut self,
        input_views: &PlaneViews,
        output_views: &[PlaneViews],
    ) -> Result<Descriptors, VulkanTranscoderError> {
        let image_info_input_y = vk::DescriptorImageInfo::default()
            .image_view(input_views.view_y.view)
            .image_layout(vk::ImageLayout::GENERAL);
        let image_info_input_uv = vk::DescriptorImageInfo::default()
            .image_view(input_views.view_uv.view)
            .image_layout(vk::ImageLayout::GENERAL);

        let (image_infos_output_y, image_infos_output_uv) = output_views
            .iter()
            .map(|views| {
                (
                    vk::DescriptorImageInfo::default()
                        .image_layout(vk::ImageLayout::GENERAL)
                        .image_view(views.view_y.view),
                    vk::DescriptorImageInfo::default()
                        .image_layout(vk::ImageLayout::GENERAL)
                        .image_view(views.view_uv.view),
                )
            })
            .unzip::<_, _, Vec<_>, Vec<_>>();

        let descriptors = self.descriptor_heap.allocate()?;

        let writes = [
            (
                descriptors.input.descriptor_set,
                std::slice::from_ref(&image_info_input_y),
                0,
            ),
            (
                descriptors.input.descriptor_set,
                std::slice::from_ref(&image_info_input_uv),
                1,
            ),
            (
                descriptors.output_y.descriptor_set,
                &image_infos_output_y,
                0,
            ),
            (
                descriptors.output_uv.descriptor_set,
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
                .image_info(image_infos)
        })
        .collect::<Vec<_>>();
        unsafe { self.device.device.update_descriptor_sets(&writes, &[]) };

        Ok(descriptors)
    }

    pub(crate) fn run(
        &mut self,
        input_submission: &mut DecodeSubmission,
        input_cropped_extent: vk::Extent2D,
        encoders: &mut [Box<dyn DynVulkanEncoder>],
    ) -> Result<ResizeSubmission, VulkanTranscoderError> {
        let input_image = &input_submission.decode_result.frame.image;
        let input_views = PlaneViews::new(
            input_image.clone(),
            input_submission.decode_result.frame.layer,
        )?;
        let outputs = encoders
            .iter_mut()
            .map(|e| e.next_input_image())
            .collect::<Result<Box<_>, _>>()?;
        let output_views = outputs
            .iter()
            .map(|output| PlaneViews::new(output.image.clone(), 0))
            .collect::<Result<Vec<_>, _>>()?;
        let descriptors = self.write_descriptors(&input_views, &output_views)?;

        let mut buffer = self.buffer_pool.begin_buffer()?;
        self.device
            .device
            .set_label(buffer.buffer(), Some("resize pipeline buffer"))?;

        input_image.transition_layout_single_layer(
            &mut buffer,
            vk::PipelineStageFlags2::NONE..vk::PipelineStageFlags2::COMPUTE_SHADER,
            vk::AccessFlags2::NONE..vk::AccessFlags2::SHADER_STORAGE_READ,
            vk::ImageLayout::GENERAL,
            input_submission.decode_result.frame.layer,
        )?;
        for output in outputs.iter() {
            output.image.transition_layout_single_layer(
                &mut buffer,
                vk::PipelineStageFlags2::NONE..vk::PipelineStageFlags2::COMPUTE_SHADER,
                vk::AccessFlags2::NONE..vk::AccessFlags2::SHADER_STORAGE_WRITE,
                vk::ImageLayout::GENERAL,
                0,
            )?;
        }

        let dispatch_size = outputs
            .iter()
            .map(|output| {
                (output.image.extent.width.next_multiple_of(16)
                    * output.image.extent.height.next_multiple_of(16))
                .div_ceil(256)
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
                    descriptors.input.descriptor_set,
                    descriptors.output_y.descriptor_set,
                    descriptors.output_uv.descriptor_set,
                ],
                &[],
            );

            let push_constants = PushConstants::new(&self.configs, input_cropped_extent);
            self.device.device.cmd_push_constants(
                buffer.buffer(),
                self.pipeline.layout.layout,
                vk::ShaderStageFlags::COMPUTE,
                0,
                bytemuck::bytes_of(&push_constants),
            );
            self.device
                .device
                .cmd_dispatch(buffer.buffer(), dispatch_size, 1, 1);
        }

        let buffer = buffer.end()?;
        let buffer_info = vk::CommandBufferSubmitInfo::default().command_buffer(buffer.buffer());

        let (encoder_semaphores, encoder_semaphore_submit_infos): (Vec<_>, Vec<_>) = encoders
            .iter_mut()
            .map(|e| {
                let tracker = &mut e.tracker().semaphore_tracker;
                (
                    tracker.semaphore.clone(),
                    tracker.next_submit_info(EncoderTrackerWaitState::ResizeInput),
                )
            })
            .unzip();

        let mut signals = encoder_semaphore_submit_infos
            .iter()
            .map(|c| c.signal_info(vk::PipelineStageFlags2::ALL_COMMANDS))
            .collect::<Vec<_>>();
        let mut waits = encoder_semaphore_submit_infos
            .iter()
            .flat_map(|c| c.wait_info(vk::PipelineStageFlags2::ALL_COMMANDS))
            .collect::<Vec<_>>();

        let decoder_semaphore_submit_info = input_submission
            .decoder
            .tracker
            .semaphore_tracker
            .next_submit_info(DecoderTrackerWaitState::ExternalProcessing);

        if let Some(wait) =
            decoder_semaphore_submit_info.wait_info(vk::PipelineStageFlags2::ALL_COMMANDS)
        {
            waits.push(wait);
        }

        signals
            .push(decoder_semaphore_submit_info.signal_info(vk::PipelineStageFlags2::ALL_COMMANDS));

        let submission_wait_value = decoder_semaphore_submit_info.signal_value();
        let submit_info = vk::SubmitInfo2::default()
            .command_buffer_infos(std::slice::from_ref(&buffer_info))
            .wait_semaphore_infos(&waits)
            .signal_semaphore_infos(&signals);

        unsafe {
            self.device.device.queue_submit2(
                *self.device.queues.compute.queue.lock().unwrap(),
                &[submit_info],
                vk::Fence::null(),
            )?;
        }

        buffer.mark_submitted(submission_wait_value);
        for semaphore_submit_info in encoder_semaphore_submit_infos {
            semaphore_submit_info.mark_submitted();
        }

        decoder_semaphore_submit_info.mark_submitted();

        Ok(ResizeSubmission {
            outputs,
            wait_value: submission_wait_value,
            in_flight_resources: InFlightResizeResources {
                _input_views: input_views,
                _output_views: output_views,
                descriptors: Some(descriptors),
                _pipeline: self.pipeline.clone(),
                _encoder_semaphores: encoder_semaphores,
            },
        })
    }
}
