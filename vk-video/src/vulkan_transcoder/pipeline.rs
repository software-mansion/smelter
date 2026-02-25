use std::{collections::VecDeque, sync::Arc};

use ash::vk;

use crate::{
    VulkanDevice,
    vulkan_decoder::{DecodeSubmission, DecoderTracker, DecoderTrackerWaitState},
    vulkan_encoder::{EncoderTracker, EncoderTrackerWaitState, H264EncodeProfileInfo},
    vulkan_transcoder::TranscoderError,
    wrappers::{
        Buffer, CommandBufferPool, ComputePipeline, DescriptorPool, DescriptorSet,
        DescriptorSetLayout, Image, ImageView, PipelineLayout, ShaderModule, TrackerWait,
        TransferDirection,
    },
};

const MAX_OUTPUTS: u32 = 8;
const MAX_FRAMES_IN_FLIGHT: u32 = 16; // The max reorder in h264
const DEBUG_BUFFER_SIZE: u64 = 4 * 1024 * 1024; // 4MB for debug storage buffer

pub(crate) struct ResizingImageBundle {
    pub(crate) image: Arc<Image>,
    view_y: ImageView,
    view_uv: ImageView,
}

pub(crate) struct ResizeSubmission {
    pub(crate) outputs: Vec<ResizingImageBundle>,
    pub(crate) input: ResizingImageBundle,
    pub(crate) descriptors: Descriptors,
}

impl ResizingImageBundle {
    fn new(image: Arc<Image>, layer: u32) -> Result<Self, TranscoderError> {
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

pub(crate) struct Descriptors {
    input: DescriptorSet,
    output_y: DescriptorSet,
    output_uv: DescriptorSet,
}

struct DescriptorHeap {
    pool: Arc<DescriptorPool>,
    freelist: Vec<Descriptors>,
    output_counts: [u32; 2],
    layout_input: Arc<DescriptorSetLayout>, // TODO: maybe no need to be in arcs?
    layout_output: Arc<DescriptorSetLayout>,
}

impl DescriptorHeap {
    fn new(
        pool: Arc<DescriptorPool>,
        output_counts: [u32; 2],
        layout_input: Arc<DescriptorSetLayout>,
        layout_output: Arc<DescriptorSetLayout>,
    ) -> Self {
        Self {
            pool,
            freelist: Vec::new(),
            output_counts,
            layout_input,
            layout_output,
        }
    }

    fn free(&mut self, descriptors: Descriptors) {
        self.freelist.push(descriptors);
    }

    fn allocate(&mut self) -> Result<Descriptors, TranscoderError> {
        if let Some(descriptors) = self.freelist.pop() {
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

        // let mut count = vk::DescriptorSetVariableDescriptorCountAllocateInfo::default()
        //     .descriptor_counts(&self.output_counts);
        let mut descriptor_set_outputs = DescriptorSet::new(
            self.pool.clone(),
            &vk::DescriptorSetAllocateInfo::default()
                .set_layouts(&[self.layout_output.set_layout, self.layout_output.set_layout])
                // .push_next(&mut count),
        )?;

        let output_uv = descriptor_set_outputs.pop().unwrap();
        let output_y = descriptor_set_outputs.pop().unwrap();

        Ok(Descriptors {
            input,
            output_y,
            output_uv,
        })
    }
}

pub(crate) struct Pipeline {
    descriptor_heap: DescriptorHeap,
    pipeline: ComputePipeline,
    buffer_pool: CommandBufferPool,
    device: Arc<VulkanDevice>,
    debug_buffer: Buffer,
    debug_descriptor: DescriptorSet,
}

impl Pipeline {
    pub(crate) fn new(
        device: Arc<VulkanDevice>,
        configs: &[OutputConfig<'_>],
    ) -> Result<Self, TranscoderError> {
        let pool_sizes = [
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::STORAGE_IMAGE)
                .descriptor_count((2 * MAX_OUTPUTS as u32 + 2) * MAX_FRAMES_IN_FLIGHT),
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1),
        ];
        let descriptor_pool = Arc::new(DescriptorPool::new(
            device.device.clone(),
            &vk::DescriptorPoolCreateInfo::default()
                .max_sets(3 * MAX_FRAMES_IN_FLIGHT + 1)
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

        let bindings_output = [vk::DescriptorSetLayoutBinding::default()
            .descriptor_count(MAX_OUTPUTS as u32)
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
            [configs.len() as u32; 2],
            layout_input.clone(),
            layout_output.clone(),
        );

        let bindings_debug = [vk::DescriptorSetLayoutBinding::default()
            .descriptor_count(1)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .binding(0)
            .stage_flags(vk::ShaderStageFlags::COMPUTE)];
        let layout_debug = Arc::new(DescriptorSetLayout::new(
            device.device.clone(),
            &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings_debug),
        )?);

        let layouts = [
            layout_input.set_layout,
            layout_output.set_layout,
            layout_output.set_layout,
            layout_debug.set_layout,
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
            vec![layout_input.clone(), layout_output.clone(), layout_debug.clone()],
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
        // let spv = ash::util::read_spv(&mut std::io::Cursor::new(include_bytes!("../../out.spv"))).unwrap();
        let shader_module = Arc::new(ShaderModule::new(
            device.device.clone(),
            &vk::ShaderModuleCreateInfo::default().code(&compiled),
        )?);

        // let shader_module = Arc::new(ShaderModule::new(
        //     device.device.clone(),
        //     &vk::ShaderModuleCreateInfo::default().code(&spv),
        // )?);

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

        // Debug storage buffer (host-visible for readback)
        let debug_buffer = Buffer::new(
            device.allocator.clone(),
            vk::BufferCreateInfo::default()
                .size(DEBUG_BUFFER_SIZE)
                .usage(
                    vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
                )
                .sharing_mode(vk::SharingMode::EXCLUSIVE),
            TransferDirection::GpuToMem,
        )?;

        let debug_descriptor = DescriptorSet::new(
            descriptor_pool.clone(),
            &vk::DescriptorSetAllocateInfo::default()
                .descriptor_pool(descriptor_pool.pool)
                .set_layouts(&[layout_debug.set_layout]),
        )?
        .pop()
        .unwrap();

        let debug_buffer_info = vk::DescriptorBufferInfo::default()
            .buffer(debug_buffer.buffer)
            .offset(0)
            .range(DEBUG_BUFFER_SIZE);
        unsafe {
            device.device.update_descriptor_sets(
                &[vk::WriteDescriptorSet::default()
                    .dst_set(debug_descriptor.descriptor_set)
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .buffer_info(std::slice::from_ref(&debug_buffer_info))],
                &[],
            );
        }

        Ok(Self {
            descriptor_heap,
            pipeline,
            buffer_pool,
            device,
            debug_buffer,
            debug_descriptor,
        })
    }

    pub(crate) fn free_descriptors(&mut self, descriptors: Descriptors) {
        self.descriptor_heap.free(descriptors);
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
                .usage(vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::VIDEO_ENCODE_SRC_KHR
                    | if std::env::var("DUMP_TRANSCODER_OUTPUT").is_ok() {
                        vk::ImageUsageFlags::TRANSFER_SRC
                    } else {
                        vk::ImageUsageFlags::empty()
                    })
                .sharing_mode(vk::SharingMode::CONCURRENT)
                .queue_family_indices(&queue_indices)
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .push_next(&mut profile_list_info);

            let image = Arc::new(Image::new(
                self.device.allocator.clone(),
                &create_info,
                config.tracker.image_layout_tracker.clone(),
            )?);

            self.device.device.set_label(image.image, Some("resize image"))?;

            result.push(ResizingImageBundle::new(image, 0)?);
        }

        Ok(result)
    }

    fn write_descriptors(
        &mut self,
        input: &ResizingImageBundle,
        outputs: &[ResizingImageBundle],
    ) -> Result<Descriptors, TranscoderError> {
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
                .image_info(&image_infos)
        })
        .collect::<Vec<_>>();
        unsafe { self.device.device.update_descriptor_sets(&writes, &[]) };

        Ok(descriptors)
    }

    pub(crate) fn run(
        &mut self,
        input_submission: &DecodeSubmission,
        decode_tracker: &mut DecoderTracker,
        configs: &mut [OutputConfig<'_>],
    ) -> Result<ResizeSubmission, TranscoderError> {
        let input =
            ResizingImageBundle::new(input_submission.image.clone(), input_submission.layer)?;
        let outputs = self.allocate_images(configs)?;
        let descriptors = self.write_descriptors(&input, &outputs)?;

        let mut buffer = self.buffer_pool.begin_buffer()?;
        self.device.device.set_label(buffer.buffer(), Some("resize pipeline buffer"))?;
        input.image.transition_layout_single_layer(
            &mut buffer,
            vk::PipelineStageFlags2::NONE..vk::PipelineStageFlags2::COMPUTE_SHADER,
            vk::AccessFlags2::NONE..vk::AccessFlags2::SHADER_STORAGE_READ,
            vk::ImageLayout::GENERAL,
            input_submission.layer,
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

        // TODO: flushing and signal the decoder

        // TODO: move this and make this better
        let dispatch_size = outputs
            .iter()
            .map(|&ResizingImageBundle { ref image, .. }| {
                image.extent.width * image.extent.height
            })
            .sum::<u32>();

        println!("dispatch_size: {}", dispatch_size);

        // Clear debug buffer before dispatch
        unsafe {
            self.device.device.cmd_fill_buffer(
                buffer.buffer(),
                self.debug_buffer.buffer,
                0,
                DEBUG_BUFFER_SIZE,
                0,
            );
            let fill_barrier = vk::MemoryBarrier2::default()
                .src_stage_mask(vk::PipelineStageFlags2::TRANSFER)
                .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
                .dst_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
                .dst_access_mask(
                    vk::AccessFlags2::SHADER_STORAGE_READ
                        | vk::AccessFlags2::SHADER_STORAGE_WRITE,
                );
            let dep_info = vk::DependencyInfo::default()
                .memory_barriers(std::slice::from_ref(&fill_barrier));
            self.device
                .device
                .cmd_pipeline_barrier2(buffer.buffer(), &dep_info);
        }

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
                    self.debug_descriptor.descriptor_set,
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

        // Debug: dump raw transcoder output images to verify green bar source
        let staging_buffers = if std::env::var("DUMP_TRANSCODER_OUTPUT").is_ok() {
            let barrier = vk::MemoryBarrier2::default()
                .src_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
                .src_access_mask(vk::AccessFlags2::SHADER_STORAGE_WRITE)
                .dst_stage_mask(vk::PipelineStageFlags2::TRANSFER)
                .dst_access_mask(vk::AccessFlags2::TRANSFER_READ);
            let dep_info = vk::DependencyInfo::default()
                .memory_barriers(std::slice::from_ref(&barrier));
            unsafe {
                self.device
                    .device
                    .cmd_pipeline_barrier2(buffer.buffer(), &dep_info);
            }

            let mut staging = Vec::new();
            for (idx, bundle) in outputs.iter().enumerate() {
                let w = bundle.image.extent.width;
                let h = bundle.image.extent.height;
                let y_size = (w as u64) * (h as u64);
                let total_size = y_size * 3 / 2;

                let staging_buf = Buffer::new_transfer(
                    self.device.allocator.clone(),
                    total_size,
                    TransferDirection::GpuToMem,
                )?;

                let copy_info = [
                    vk::BufferImageCopy::default()
                        .image_subresource(vk::ImageSubresourceLayers {
                            aspect_mask: vk::ImageAspectFlags::PLANE_0,
                            layer_count: 1,
                            base_array_layer: 0,
                            mip_level: 0,
                        })
                        .image_extent(vk::Extent3D {
                            width: w,
                            height: h,
                            depth: 1,
                        })
                        .buffer_offset(0)
                        .buffer_row_length(0)
                        .buffer_image_height(0),
                    vk::BufferImageCopy::default()
                        .image_subresource(vk::ImageSubresourceLayers {
                            aspect_mask: vk::ImageAspectFlags::PLANE_1,
                            layer_count: 1,
                            base_array_layer: 0,
                            mip_level: 0,
                        })
                        .image_extent(vk::Extent3D {
                            width: w / 2,
                            height: h / 2,
                            depth: 1,
                        })
                        .buffer_offset(y_size)
                        .buffer_row_length(0)
                        .buffer_image_height(0),
                ];

                unsafe {
                    self.device.device.cmd_copy_image_to_buffer(
                        buffer.buffer(),
                        bundle.image.image,
                        vk::ImageLayout::GENERAL,
                        staging_buf.buffer,
                        &copy_info,
                    );
                }

                staging.push((idx, staging_buf, w, h, total_size));
            }
            Some(staging)
        } else {
            None
        };

        let buffer = buffer.end()?;
        let buffer_info = vk::CommandBufferSubmitInfo::default().command_buffer(buffer.buffer());

        let (waits, signals) = configs
            .iter_mut()
            .map(|OutputConfig { tracker, .. }| {
                let value = tracker.semaphore_tracker.next_sem_value();
                let signal = vk::SemaphoreSubmitInfo::default()
                    .stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                    .semaphore(tracker.semaphore_tracker.semaphore.semaphore)
                    .value(value.0);
                let wait = tracker.semaphore_tracker.wait_for.take().map(|wait| {
                    vk::SemaphoreSubmitInfo::default()
                        .value(wait.value.0)
                        .semaphore(tracker.semaphore_tracker.semaphore.semaphore)
                        .stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                });
                (wait, (signal, value))
            })
            .unzip::<_, _, Vec<_>, Vec<_>>();

        let (mut signals, values) = signals.into_iter().unzip::<_, _, Vec<_>, Vec<_>>();

        let mut waits = waits.into_iter().flatten().collect::<Vec<_>>();
        waits.push(
            vk::SemaphoreSubmitInfo::default()
                .stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                .value(input_submission.semaphore_wait_value.0)
                .semaphore(decode_tracker.semaphore_tracker.semaphore.semaphore),
        );
        let next_decoder_value = decode_tracker.semaphore_tracker.next_sem_value();
        signals.push(
            vk::SemaphoreSubmitInfo::default()
                .stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                .value(next_decoder_value.0)
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
        decode_tracker.semaphore_tracker.wait_for = Some(TrackerWait {
            value: next_decoder_value,
            _state: DecoderTrackerWaitState::ExternalProcessing,
        });

        // Debug: read back staging buffers and debug buffer
        if let Some(staging_buffers) = staging_buffers {
            unsafe { self.device.device.device_wait_idle()? };
            for (idx, mut buf, w, h, total_size) in staging_buffers {
                let data = unsafe { buf.download_data_from_buffer(total_size as usize)? };
                let path = format!("/tmp/transcoder_output_{idx}_{w}x{h}.nv12");
                std::fs::write(&path, &data).unwrap_or_else(|e| {
                    eprintln!("Failed to write {path}: {e}");
                });
                eprintln!("Dumped transcoder output {idx} ({w}x{h}) to {path}");
            }

            // Read debug storage buffer
            let data = unsafe {
                self.debug_buffer
                    .download_data_from_buffer(DEBUG_BUFFER_SIZE as usize)?
            };
            let u32s = unsafe {
                std::slice::from_raw_parts(data.as_ptr() as *const u32, data.len() / 4)
            };

            // Per-output dimensions from shader's textureDimensions
            eprintln!("=== Per-output textureDimensions ===");
            for i in 0..configs.len() {
                let base = i * 8;
                eprintln!(
                    "  Output {i}: dest_y={}x{}, dest_uv={}x{}, src_y={}x{}, src_uv={}x{}",
                    u32s[base], u32s[base + 1],
                    u32s[base + 2], u32s[base + 3],
                    u32s[base + 4], u32s[base + 5],
                    u32s[base + 6], u32s[base + 7],
                );
            }

            // Y values for bottom 16 rows of last output
            let last = configs.last().unwrap();
            let w = last.width as usize;
            let h = last.height as usize;

            eprintln!("=== Last output Y values (bottom 16 rows) ===");
            for row in 0..16usize {
                let y = h - 16 + row;
                let samples: Vec<String> = [0, w / 4, w / 2, 3 * w / 4, w - 1]
                    .iter()
                    .map(|&x| {
                        let offset = 64 + row * w + x;
                        let val = f32::from_bits(u32s[offset]);
                        format!("x={x}:Y={val:.4}")
                    })
                    .collect();
                eprintln!("  y={y}: {}", samples.join(", "));
            }

            // UV values for bottom 16 rows of last output (even rows only)
            let uv_base = 64 + 16 * w;
            let uv_w = w / 2;
            eprintln!("=== Last output UV values (bottom 8 UV rows) ===");
            for row in 0..8usize {
                let y = h - 16 + row * 2;
                let samples: Vec<String> = [0, uv_w / 4, uv_w / 2, 3 * uv_w / 4, uv_w - 1]
                    .iter()
                    .map(|&ux| {
                        let offset = uv_base + (row * uv_w + ux) * 2;
                        let u_val = f32::from_bits(u32s[offset]);
                        let v_val = f32::from_bits(u32s[offset + 1]);
                        format!("x={ux}:U={u_val:.4},V={v_val:.4}")
                    })
                    .collect();
                eprintln!("  y={y}: {}", samples.join(", "));
            }
        }

        Ok(ResizeSubmission {
            outputs,
            input,
            descriptors,
        })
    }

    pub(crate) fn mark_command_buffers_completed(&self) {
        self.buffer_pool.mark_all_submitted_as_free();
    }
}
