use std::sync::Arc;

use ash::vk;

use h264_reader::nal::{pps::PicParameterSet, sps::SeqParameterSet};
use session_resources::VideoSessionResources;
use tracing::error;
use wrappers::*;

use crate::parser::{DecodeInformation, DecoderInstruction, ReferenceId};

mod frame_sorter;
mod session_resources;
mod vulkan_ctx;
mod wrappers;

pub(crate) use frame_sorter::FrameSorter;
pub use vulkan_ctx::*;

pub struct VulkanDecoder<'a> {
    vulkan_device: Arc<VulkanDevice>,
    video_session_resources: Option<VideoSessionResources<'a>>,
    command_buffers: CommandBuffers,
    _command_pools: CommandPools,
    sync_structures: SyncStructures,
    reference_id_to_dpb_slot_index: std::collections::HashMap<ReferenceId, usize>,
}

struct SyncStructures {
    sem_decode_done: Semaphore,
    fence_transfer_done: Fence,
    fence_memory_barrier_completed: Fence,
}

struct CommandBuffers {
    decode_buffer: CommandBuffer,
    gpu_to_mem_transfer_buffer: CommandBuffer,
    vulkan_to_wgpu_transfer_buffer: CommandBuffer,
}

/// this cannot outlive the image and semaphore it borrows, but it seems impossible to encode that
/// in the lifetimes
struct DecodeSubmission {
    image: vk::Image,
    dimensions: vk::Extent2D,
    current_layout: vk::ImageLayout,
    layer: u32,
    wait_semaphore: vk::Semaphore,
    _input_buffer: Buffer,
    picture_order_cnt: i32,
    max_num_reorder_frames: u64,
    is_idr: bool,
    pts: Option<u64>,
}

#[derive(Debug, thiserror::Error)]
pub enum VulkanDecoderError {
    #[error("Vulkan error: {0}")]
    VkError(#[from] vk::Result),

    #[error("A NALU requiring a session received before a session was created (probably before receiving first SPS)")]
    NoSession,

    #[error("A slot in the Decoded Pictures Buffer was requested, but all slots are taken")]
    NoFreeSlotsInDpb,

    #[error("A picture which is not in the decoded pictures buffer was requested as a reference picture")]
    NonExistentReferenceRequested,

    #[error("A vulkan decode operation failed with code {0:?}")]
    DecodeOperationFailed(vk::QueryResultStatusKHR),

    #[error("Invalid input data for the decoder: {0}.")]
    InvalidInputData(String),

    #[error("Profile changed during the stream")]
    ProfileChangeUnsupported,

    #[error("Level changed during the stream")]
    LevelChangeUnsupported,

    #[error("Monochrome video is not supported")]
    MonochromeChromaFormatUnsupported,

    #[error(transparent)]
    VulkanCtxError(#[from] VulkanCtxError),
}

impl VulkanDecoder<'_> {
    pub fn new(vulkan_ctx: Arc<VulkanDevice>) -> Result<Self, VulkanDecoderError> {
        let decode_pool = Arc::new(CommandPool::new(
            vulkan_ctx.device.clone(),
            vulkan_ctx.queues.h264_decode.idx,
        )?);

        let transfer_pool = Arc::new(CommandPool::new(
            vulkan_ctx.device.clone(),
            vulkan_ctx.queues.transfer.idx,
        )?);

        let decode_buffer = CommandBuffer::new_primary(decode_pool.clone())?;

        let gpu_to_mem_transfer_buffer = CommandBuffer::new_primary(transfer_pool.clone())?;

        let vulkan_to_wgpu_transfer_buffer = CommandBuffer::new_primary(transfer_pool.clone())?;

        let command_pools = CommandPools {
            _decode_pool: decode_pool,
            _transfer_pool: transfer_pool,
        };

        let sync_structures = SyncStructures {
            sem_decode_done: Semaphore::new(vulkan_ctx.device.clone())?,
            fence_transfer_done: Fence::new(vulkan_ctx.device.clone(), false)?,
            fence_memory_barrier_completed: Fence::new(vulkan_ctx.device.clone(), false)?,
        };

        Ok(Self {
            vulkan_device: vulkan_ctx,
            video_session_resources: None,
            _command_pools: command_pools,
            command_buffers: CommandBuffers {
                decode_buffer,
                gpu_to_mem_transfer_buffer,
                vulkan_to_wgpu_transfer_buffer,
            },
            sync_structures,
            reference_id_to_dpb_slot_index: Default::default(),
        })
    }
}

pub(crate) struct DecodeResult<T> {
    frame: T,
    pts: Option<u64>,
    pic_order_cnt: i32,
    max_num_reorder_frames: u64,
    is_idr: bool,
}

impl VulkanDecoder<'_> {
    pub fn decode_to_bytes(
        &mut self,
        decoder_instructions: &[DecoderInstruction],
    ) -> Result<Vec<DecodeResult<Vec<u8>>>, VulkanDecoderError> {
        let mut result = Vec::new();
        for instruction in decoder_instructions {
            if let Some(output) = self.decode(instruction)? {
                result.push(DecodeResult {
                    pts: output.pts,
                    is_idr: output.is_idr,
                    max_num_reorder_frames: output.max_num_reorder_frames,
                    pic_order_cnt: output.picture_order_cnt,
                    frame: self.download_output(output)?,
                })
            }
        }

        Ok(result)
    }

    pub fn decode_to_wgpu_textures(
        &mut self,
        decoder_instructions: &[DecoderInstruction],
    ) -> Result<Vec<DecodeResult<wgpu::Texture>>, VulkanDecoderError> {
        let mut result = Vec::new();
        for instruction in decoder_instructions {
            if let Some(output) = self.decode(instruction)? {
                result.push(DecodeResult {
                    pts: output.pts,
                    is_idr: output.is_idr,
                    max_num_reorder_frames: output.max_num_reorder_frames,
                    pic_order_cnt: output.picture_order_cnt,
                    frame: self.output_to_wgpu_texture(output)?,
                })
            }
        }

        Ok(result)
    }

    fn decode(
        &mut self,
        instruction: &DecoderInstruction,
    ) -> Result<Option<DecodeSubmission>, VulkanDecoderError> {
        match instruction {
            DecoderInstruction::Decode {
                decode_info,
                reference_id,
            } => {
                return self
                    .process_reference_p_or_b_frame(decode_info, *reference_id)
                    .map(Option::Some)
            }

            DecoderInstruction::Idr {
                decode_info,
                reference_id,
            } => {
                return self
                    .process_idr(decode_info, *reference_id)
                    .map(Option::Some)
            }

            DecoderInstruction::Drop { reference_ids } => {
                for reference_id in reference_ids {
                    match self.reference_id_to_dpb_slot_index.remove(reference_id) {
                        Some(dpb_idx) => self
                            .video_session_resources
                            .as_mut()
                            .map(|s| s.free_reference_picture(dpb_idx)),
                        None => return Err(VulkanDecoderError::NonExistentReferenceRequested),
                    };
                }
            }

            DecoderInstruction::Sps(sps) => self.process_sps(sps)?,

            DecoderInstruction::Pps(pps) => self.process_pps(pps)?,
        }

        Ok(None)
    }

    fn process_sps(&mut self, sps: &SeqParameterSet) -> Result<(), VulkanDecoderError> {
        match self.video_session_resources.as_mut() {
            Some(session) => session.process_sps(
                &self.vulkan_device,
                &self.command_buffers.decode_buffer,
                sps.clone(),
                &self.sync_structures.fence_memory_barrier_completed,
            )?,
            None => {
                self.video_session_resources = Some(VideoSessionResources::new_from_sps(
                    &self.vulkan_device,
                    &self.command_buffers.decode_buffer,
                    sps.clone(),
                    &self.sync_structures.fence_memory_barrier_completed,
                )?)
            }
        }

        Ok(())
    }

    fn process_pps(&mut self, pps: &PicParameterSet) -> Result<(), VulkanDecoderError> {
        self.video_session_resources
            .as_mut()
            .ok_or(VulkanDecoderError::NoSession)?
            .process_pps(pps.clone())?;

        Ok(())
    }

    fn pad_size_to_alignment(size: u64, align: u64) -> u64 {
        if size % align == 0 {
            size
        } else {
            (size + align) / align * align
        }
    }

    fn process_idr(
        &mut self,
        decode_information: &DecodeInformation,
        reference_id: ReferenceId,
    ) -> Result<DecodeSubmission, VulkanDecoderError> {
        self.do_decode(decode_information, reference_id, true, true)
    }

    fn process_reference_p_or_b_frame(
        &mut self,
        decode_information: &DecodeInformation,
        reference_id: ReferenceId,
    ) -> Result<DecodeSubmission, VulkanDecoderError> {
        self.do_decode(decode_information, reference_id, false, true)
    }

    fn do_decode(
        &mut self,
        decode_information: &DecodeInformation,
        reference_id: ReferenceId,
        is_idr: bool,
        is_reference: bool,
    ) -> Result<DecodeSubmission, VulkanDecoderError> {
        // upload data to a buffer
        let size = Self::pad_size_to_alignment(
            decode_information.rbsp_bytes.len() as u64,
            self.vulkan_device
                .video_capabilities
                .min_bitstream_buffer_offset_alignment,
        );

        // decode
        let video_session_resources = self
            .video_session_resources
            .as_mut()
            .ok_or(VulkanDecoderError::NoSession)?;

        let decode_buffer = Buffer::new_with_decode_data(
            self.vulkan_device.allocator.clone(),
            &decode_information.rbsp_bytes,
            size,
            &video_session_resources.profile_info,
        )?;

        // IDR - remove all reference picures
        if is_idr {
            video_session_resources
                .decoding_images
                .reset_all_allocations();

            self.reference_id_to_dpb_slot_index = Default::default();
        }

        // begin video coding
        self.command_buffers.decode_buffer.begin()?;

        let memory_barrier = vk::MemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::VIDEO_DECODE_KHR)
            .src_access_mask(vk::AccessFlags2::VIDEO_DECODE_WRITE_KHR)
            .dst_stage_mask(vk::PipelineStageFlags2::VIDEO_DECODE_KHR)
            .dst_access_mask(
                vk::AccessFlags2::VIDEO_DECODE_READ_KHR | vk::AccessFlags2::VIDEO_DECODE_WRITE_KHR,
            );

        unsafe {
            self.vulkan_device.device.cmd_pipeline_barrier2(
                *self.command_buffers.decode_buffer,
                &vk::DependencyInfo::default().memory_barriers(&[memory_barrier]),
            )
        };

        if let Some(pool) = video_session_resources.decode_query_pool.as_ref() {
            pool.reset(*self.command_buffers.decode_buffer);
        }

        let reference_slots = video_session_resources
            .decoding_images
            .reference_slot_info();

        let begin_info = vk::VideoBeginCodingInfoKHR::default()
            .video_session(video_session_resources.video_session.session)
            .video_session_parameters(video_session_resources.parameters_manager.parameters())
            .reference_slots(&reference_slots);

        unsafe {
            self.vulkan_device
                .device
                .video_queue_ext
                .cmd_begin_video_coding_khr(*self.command_buffers.decode_buffer, &begin_info)
        };

        // IDR - issue the reset command to the video session
        if is_idr {
            let control_info = vk::VideoCodingControlInfoKHR::default()
                .flags(vk::VideoCodingControlFlagsKHR::RESET);

            unsafe {
                self.vulkan_device
                    .device
                    .video_queue_ext
                    .cmd_control_video_coding_khr(
                        *self.command_buffers.decode_buffer,
                        &control_info,
                    )
            };
        }

        // allocate a new reference picture and fill out the forms to get it set up
        let new_reference_slot_index = video_session_resources
            .decoding_images
            .allocate_reference_picture()?;

        let new_reference_slot_std_reference_info = decode_information.picture_info.into();
        let mut new_reference_slot_dpb_slot_info = vk::VideoDecodeH264DpbSlotInfoKHR::default()
            .std_reference_info(&new_reference_slot_std_reference_info);

        let new_reference_slot_video_picture_resource_info = video_session_resources
            .decoding_images
            .video_resource_info(new_reference_slot_index)
            .unwrap();

        let setup_reference_slot = vk::VideoReferenceSlotInfoKHR::default()
            .picture_resource(new_reference_slot_video_picture_resource_info)
            .slot_index(new_reference_slot_index as i32)
            .push_next(&mut new_reference_slot_dpb_slot_info);

        // prepare the reference list
        let reference_slots = video_session_resources
            .decoding_images
            .reference_slot_info();

        let references_std_ref_info = Self::prepare_references_std_ref_info(decode_information);

        let mut references_dpb_slot_info =
            Self::prepare_references_dpb_slot_info(&references_std_ref_info);

        let pic_reference_slots = Self::prepare_reference_list_slot_info(
            &self.reference_id_to_dpb_slot_index,
            &reference_slots,
            &mut references_dpb_slot_info,
            decode_information,
        )?;

        // prepare the decode target picture
        let std_picture_info = vk::native::StdVideoDecodeH264PictureInfo {
            flags: vk::native::StdVideoDecodeH264PictureInfoFlags {
                _bitfield_align_1: [],
                __bindgen_padding_0: [0; 3],
                _bitfield_1: vk::native::StdVideoDecodeH264PictureInfoFlags::new_bitfield_1(
                    matches!(
                        decode_information.header.field_pic,
                        h264_reader::nal::slice::FieldPic::Field(..)
                    )
                    .into(),
                    is_idr.into(),
                    is_idr.into(),
                    0,
                    is_reference.into(),
                    0,
                ),
            },
            PicOrderCnt: decode_information.picture_info.PicOrderCnt_for_decoding,
            seq_parameter_set_id: decode_information.sps_id,
            pic_parameter_set_id: decode_information.pps_id,
            frame_num: decode_information.header.frame_num,
            idr_pic_id: decode_information
                .header
                .idr_pic_id
                .map(|a| a as u16)
                .unwrap_or(0),
            reserved1: 0,
            reserved2: 0,
        };

        let slice_offsets = decode_information
            .slice_indices
            .iter()
            .map(|&x| x as u32)
            .collect::<Vec<_>>();

        let mut decode_h264_picture_info = vk::VideoDecodeH264PictureInfoKHR::default()
            .std_picture_info(&std_picture_info)
            .slice_offsets(&slice_offsets);

        let dst_picture_resource_info = &video_session_resources
            .decoding_images
            .target_picture_resource_info(new_reference_slot_index)
            .unwrap();

        // these 3 veriables are for copying the result later
        let (target_image, target_image_layout, target_layer) = video_session_resources
            .decoding_images
            .target_info(new_reference_slot_index);

        // fill out the final struct and issue the command
        let decode_info = vk::VideoDecodeInfoKHR::default()
            .src_buffer(*decode_buffer)
            .src_buffer_offset(0)
            .src_buffer_range(size)
            .dst_picture_resource(*dst_picture_resource_info)
            .setup_reference_slot(&setup_reference_slot)
            .reference_slots(&pic_reference_slots)
            .push_next(&mut decode_h264_picture_info);

        if let Some(pool) = video_session_resources.decode_query_pool.as_ref() {
            pool.begin_query(*self.command_buffers.decode_buffer);
        }

        unsafe {
            self.vulkan_device
                .device
                .video_decode_queue_ext
                .cmd_decode_video_khr(*self.command_buffers.decode_buffer, &decode_info)
        };

        if let Some(pool) = video_session_resources.decode_query_pool.as_ref() {
            pool.end_query(*self.command_buffers.decode_buffer);
        }

        unsafe {
            self.vulkan_device
                .device
                .video_queue_ext
                .cmd_end_video_coding_khr(
                    *self.command_buffers.decode_buffer,
                    &vk::VideoEndCodingInfoKHR::default(),
                )
        };

        self.command_buffers.decode_buffer.end()?;

        self.vulkan_device.queues.h264_decode.submit(
            &self.command_buffers.decode_buffer,
            &[],
            &[(
                *self.sync_structures.sem_decode_done,
                vk::PipelineStageFlags2::VIDEO_DECODE_KHR,
            )],
            None,
        )?;

        // after the decode save the new reference picture
        self.reference_id_to_dpb_slot_index
            .insert(reference_id, new_reference_slot_index);

        let sps = video_session_resources
            .sps
            .get(&decode_information.sps_id)
            .ok_or(VulkanDecoderError::NoSession)?;

        let dimensions = sps.size()?;

        Ok(DecodeSubmission {
            image: target_image,
            wait_semaphore: *self.sync_structures.sem_decode_done,
            layer: target_layer as u32,
            current_layout: target_image_layout,
            dimensions,
            _input_buffer: decode_buffer,
            picture_order_cnt: decode_information.picture_info.PicOrderCnt_for_decoding[0],
            max_num_reorder_frames: video_session_resources.max_num_reorder_frames,
            is_idr,
            pts: decode_information.pts,
        })
    }

    fn output_to_wgpu_texture(
        &self,
        decode_output: DecodeSubmission,
    ) -> Result<wgpu::Texture, VulkanDecoderError> {
        let copy_extent = vk::Extent3D {
            width: decode_output.dimensions.width,
            height: decode_output.dimensions.height,
            depth: 1,
        };

        let queue_indices = [
            self.vulkan_device.queues.transfer.idx as u32,
            self.vulkan_device.queues.wgpu.idx as u32,
        ];

        let create_info = vk::ImageCreateInfo::default()
            .flags(vk::ImageCreateFlags::MUTABLE_FORMAT)
            .image_type(vk::ImageType::TYPE_2D)
            .format(vk::Format::G8_B8R8_2PLANE_420_UNORM)
            .extent(copy_extent)
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(
                vk::ImageUsageFlags::SAMPLED
                    | vk::ImageUsageFlags::TRANSFER_DST
                    | vk::ImageUsageFlags::TRANSFER_SRC,
            )
            .sharing_mode(vk::SharingMode::CONCURRENT)
            .queue_family_indices(&queue_indices)
            .initial_layout(vk::ImageLayout::UNDEFINED);

        let image = Arc::new(Image::new(
            self.vulkan_device.allocator.clone(),
            &create_info,
        )?);

        self.command_buffers
            .vulkan_to_wgpu_transfer_buffer
            .begin()?;

        let memory_barrier_src = vk::ImageMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::NONE)
            .src_access_mask(vk::AccessFlags2::NONE)
            .dst_stage_mask(vk::PipelineStageFlags2::COPY)
            .dst_access_mask(vk::AccessFlags2::TRANSFER_READ)
            .old_layout(decode_output.current_layout)
            .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(decode_output.image)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: decode_output.layer,
                layer_count: 1,
            });

        let memory_barrier_dst = vk::ImageMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::NONE)
            .src_access_mask(vk::AccessFlags2::NONE)
            .dst_stage_mask(vk::PipelineStageFlags2::COPY)
            .dst_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(**image)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });

        unsafe {
            self.vulkan_device.device.cmd_pipeline_barrier2(
                *self.command_buffers.vulkan_to_wgpu_transfer_buffer,
                &vk::DependencyInfo::default()
                    .image_memory_barriers(&[memory_barrier_src, memory_barrier_dst]),
            )
        };

        let copy_info = [
            vk::ImageCopy::default()
                .src_subresource(vk::ImageSubresourceLayers {
                    base_array_layer: decode_output.layer,
                    mip_level: 0,
                    layer_count: 1,
                    aspect_mask: vk::ImageAspectFlags::PLANE_0,
                })
                .src_offset(vk::Offset3D::default())
                .dst_subresource(vk::ImageSubresourceLayers {
                    base_array_layer: 0,
                    mip_level: 0,
                    layer_count: 1,
                    aspect_mask: vk::ImageAspectFlags::PLANE_0,
                })
                .dst_offset(vk::Offset3D::default())
                .extent(copy_extent),
            vk::ImageCopy::default()
                .src_subresource(vk::ImageSubresourceLayers {
                    base_array_layer: decode_output.layer,
                    mip_level: 0,
                    layer_count: 1,
                    aspect_mask: vk::ImageAspectFlags::PLANE_1,
                })
                .src_offset(vk::Offset3D::default())
                .dst_subresource(vk::ImageSubresourceLayers {
                    base_array_layer: 0,
                    mip_level: 0,
                    layer_count: 1,
                    aspect_mask: vk::ImageAspectFlags::PLANE_1,
                })
                .dst_offset(vk::Offset3D::default())
                .extent(vk::Extent3D {
                    width: copy_extent.width / 2,
                    height: copy_extent.height / 2,
                    ..copy_extent
                }),
        ];

        unsafe {
            self.vulkan_device.device.cmd_copy_image(
                *self.command_buffers.vulkan_to_wgpu_transfer_buffer,
                decode_output.image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                **image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &copy_info,
            );
        }

        let memory_barrier_src = memory_barrier_src
            .src_stage_mask(vk::PipelineStageFlags2::COPY)
            .src_access_mask(vk::AccessFlags2::TRANSFER_READ)
            .dst_stage_mask(vk::PipelineStageFlags2::NONE)
            .dst_access_mask(vk::AccessFlags2::NONE)
            .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .new_layout(decode_output.current_layout);

        let memory_barrier_dst = memory_barrier_dst
            .src_stage_mask(vk::PipelineStageFlags2::COPY)
            .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::NONE)
            .dst_access_mask(vk::AccessFlags2::NONE)
            .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .new_layout(vk::ImageLayout::GENERAL);

        unsafe {
            self.vulkan_device.device.cmd_pipeline_barrier2(
                *self.command_buffers.vulkan_to_wgpu_transfer_buffer,
                &vk::DependencyInfo::default()
                    .image_memory_barriers(&[memory_barrier_src, memory_barrier_dst]),
            )
        };

        self.command_buffers.vulkan_to_wgpu_transfer_buffer.end()?;

        self.vulkan_device.queues.transfer.submit(
            &self.command_buffers.vulkan_to_wgpu_transfer_buffer,
            &[(
                decode_output.wait_semaphore,
                vk::PipelineStageFlags2::TOP_OF_PIPE,
            )],
            &[],
            Some(*self.sync_structures.fence_transfer_done),
        )?;

        self.sync_structures
            .fence_transfer_done
            .wait_and_reset(u64::MAX)?;

        let result = self
            .video_session_resources
            .as_ref()
            .and_then(|s| s.decode_query_pool.as_ref())
            .map(|pool| pool.get_result_blocking());

        if let Some(result) = result {
            let result = result?;
            if result.as_raw() < 0 {
                return Err(VulkanDecoderError::DecodeOperationFailed(result));
            }
        }

        // this has to be done with Option and mut, because the closure we create has to be FnMut.
        // this means we cannot consume its captures, so we have to take the option to be able to
        // drop the resource.
        let mut image_clone = Some(image.clone());

        let hal_texture = unsafe {
            wgpu::hal::vulkan::Device::texture_from_raw(
                **image,
                &wgpu::hal::TextureDescriptor {
                    label: Some("vulkan video output texture"),
                    usage: wgpu::hal::TextureUses::RESOURCE
                        | wgpu::hal::TextureUses::COPY_DST
                        | wgpu::hal::TextureUses::COPY_SRC,
                    memory_flags: wgpu::hal::MemoryFlags::empty(),
                    size: wgpu::Extent3d {
                        width: copy_extent.width,
                        height: copy_extent.height,
                        depth_or_array_layers: copy_extent.depth,
                    },
                    dimension: wgpu::TextureDimension::D2,
                    sample_count: 1,
                    view_formats: Vec::new(),
                    format: wgpu::TextureFormat::NV12,
                    mip_level_count: 1,
                },
                Some(Box::new(move || {
                    image_clone.take();
                })),
            )
        };

        let wgpu_texture = unsafe {
            self.vulkan_device
                .wgpu_device
                .create_texture_from_hal::<wgpu::hal::vulkan::Api>(
                    hal_texture,
                    &wgpu::TextureDescriptor {
                        label: Some("vulkan video output texture"),
                        usage: wgpu::TextureUsages::COPY_DST
                            | wgpu::TextureUsages::TEXTURE_BINDING
                            | wgpu::TextureUsages::COPY_SRC,
                        size: wgpu::Extent3d {
                            width: copy_extent.width,
                            height: copy_extent.height,
                            depth_or_array_layers: copy_extent.depth,
                        },
                        dimension: wgpu::TextureDimension::D2,
                        sample_count: 1,
                        view_formats: &[],
                        format: wgpu::TextureFormat::NV12,
                        mip_level_count: 1,
                    },
                )
        };

        Ok(wgpu_texture)
    }

    fn download_output(
        &self,
        decode_output: DecodeSubmission,
    ) -> Result<Vec<u8>, VulkanDecoderError> {
        let mut dst_buffer = self.copy_image_to_buffer(
            decode_output.image,
            decode_output.dimensions,
            decode_output.current_layout,
            decode_output.layer,
            &[(decode_output.wait_semaphore, vk::PipelineStageFlags2::COPY)],
            &[],
            Some(*self.sync_structures.fence_transfer_done),
        )?;

        self.sync_structures
            .fence_transfer_done
            .wait_and_reset(u64::MAX)?;

        let output = unsafe {
            dst_buffer.download_data_from_buffer(
                decode_output.dimensions.width as usize
                    * decode_output.dimensions.height as usize
                    * 3
                    / 2,
            )?
        };

        Ok(output)
    }

    fn prepare_references_std_ref_info(
        decode_information: &DecodeInformation,
    ) -> Vec<vk::native::StdVideoDecodeH264ReferenceInfo> {
        decode_information
            .reference_list
            .iter()
            .flatten()
            .map(|&ref_info| ref_info.into())
            .collect::<Vec<_>>()
    }

    fn prepare_references_dpb_slot_info(
        references_std_ref_info: &[vk::native::StdVideoDecodeH264ReferenceInfo],
    ) -> Vec<vk::VideoDecodeH264DpbSlotInfoKHR> {
        references_std_ref_info
            .iter()
            .map(|info| vk::VideoDecodeH264DpbSlotInfoKHR::default().std_reference_info(info))
            .collect::<Vec<_>>()
    }

    fn prepare_reference_list_slot_info<'a>(
        reference_id_to_dpb_slot_index: &std::collections::HashMap<ReferenceId, usize>,
        reference_slots: &'a [vk::VideoReferenceSlotInfoKHR<'a>],
        references_dpb_slot_info: &'a mut [vk::VideoDecodeH264DpbSlotInfoKHR<'a>],
        decode_information: &'a DecodeInformation,
    ) -> Result<Vec<vk::VideoReferenceSlotInfoKHR<'a>>, VulkanDecoderError> {
        let mut pic_reference_slots: Vec<vk::VideoReferenceSlotInfoKHR<'a>> = Vec::new();
        for (ref_info, dpb_slot_info) in decode_information
            .reference_list
            .iter()
            .flatten()
            .zip(references_dpb_slot_info.iter_mut())
        {
            let i = *reference_id_to_dpb_slot_index
                .get(&ref_info.id)
                .ok_or(VulkanDecoderError::NonExistentReferenceRequested)?;

            let reference = *reference_slots
                .get(i)
                .ok_or(VulkanDecoderError::NonExistentReferenceRequested)?;

            if reference.slot_index < 0 || reference.p_picture_resource.is_null() {
                return Err(VulkanDecoderError::NonExistentReferenceRequested);
            }

            let reference = reference.push_next(dpb_slot_info);

            if pic_reference_slots
                .iter()
                .all(|r| r.slot_index != reference.slot_index)
            {
                pic_reference_slots.push(reference);
            }
        }

        Ok(pic_reference_slots)
    }

    #[allow(clippy::too_many_arguments)]
    fn copy_image_to_buffer(
        &self,
        image: vk::Image,
        dimensions: vk::Extent2D,
        current_image_layout: vk::ImageLayout,
        layer: u32,
        wait_semaphores: &[(vk::Semaphore, vk::PipelineStageFlags2)],
        signal_semaphores: &[(vk::Semaphore, vk::PipelineStageFlags2)],
        fence: Option<vk::Fence>,
    ) -> Result<Buffer, VulkanDecoderError> {
        self.command_buffers.gpu_to_mem_transfer_buffer.begin()?;

        let memory_barrier = vk::ImageMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::NONE)
            .src_access_mask(vk::AccessFlags2::NONE)
            .dst_stage_mask(vk::PipelineStageFlags2::COPY)
            .dst_access_mask(vk::AccessFlags2::TRANSFER_READ)
            .old_layout(current_image_layout)
            .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(image)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: layer,
                layer_count: 1,
            });

        unsafe {
            self.vulkan_device.device.cmd_pipeline_barrier2(
                *self.command_buffers.gpu_to_mem_transfer_buffer,
                &vk::DependencyInfo::default().image_memory_barriers(&[memory_barrier]),
            )
        };

        let y_plane_size = dimensions.width as u64 * dimensions.height as u64;

        let dst_buffer = Buffer::new_transfer(
            self.vulkan_device.allocator.clone(),
            y_plane_size * 3 / 2,
            TransferDirection::GpuToMem,
        )?;

        let copy_info = [
            vk::BufferImageCopy::default()
                .image_subresource(vk::ImageSubresourceLayers {
                    mip_level: 0,
                    layer_count: 1,
                    base_array_layer: layer,
                    aspect_mask: vk::ImageAspectFlags::PLANE_0,
                })
                .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
                .image_extent(vk::Extent3D {
                    width: dimensions.width,
                    height: dimensions.height,
                    depth: 1,
                })
                .buffer_offset(0)
                .buffer_row_length(0)
                .buffer_image_height(0),
            vk::BufferImageCopy::default()
                .image_subresource(vk::ImageSubresourceLayers {
                    mip_level: 0,
                    layer_count: 1,
                    base_array_layer: layer,
                    aspect_mask: vk::ImageAspectFlags::PLANE_1,
                })
                .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
                .image_extent(vk::Extent3D {
                    width: dimensions.width / 2,
                    height: dimensions.height / 2,
                    depth: 1,
                })
                .buffer_offset(y_plane_size)
                .buffer_row_length(0)
                .buffer_image_height(0),
        ];

        unsafe {
            self.vulkan_device.device.cmd_copy_image_to_buffer(
                *self.command_buffers.gpu_to_mem_transfer_buffer,
                image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                *dst_buffer,
                &copy_info,
            )
        };

        let memory_barrier = memory_barrier
            .src_stage_mask(vk::PipelineStageFlags2::COPY)
            .src_access_mask(vk::AccessFlags2::TRANSFER_READ)
            .dst_stage_mask(vk::PipelineStageFlags2::NONE)
            .dst_access_mask(vk::AccessFlags2::NONE)
            .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .new_layout(current_image_layout);

        unsafe {
            self.vulkan_device.device.cmd_pipeline_barrier2(
                *self.command_buffers.gpu_to_mem_transfer_buffer,
                &vk::DependencyInfo::default().image_memory_barriers(&[memory_barrier]),
            )
        };

        self.command_buffers.gpu_to_mem_transfer_buffer.end()?;

        self.vulkan_device.queues.transfer.submit(
            &self.command_buffers.gpu_to_mem_transfer_buffer,
            wait_semaphores,
            signal_semaphores,
            fence,
        )?;

        Ok(dst_buffer)
    }
}
