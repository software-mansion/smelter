use std::sync::Arc;

use ash::vk;
use encode_parameter_sets::{pps, sps};

use crate::{
    vulkan_ctx::H264Profile,
    wrappers::{
        Buffer, CodingImageBundle, CommandBuffer, CommandPool, DecodedPicturesBuffer, Device,
        Fence, Image, ProfileInfo, QueryPool, Semaphore, TransferDirection, VideoEncodeQueueExt,
        VideoQueueExt, VideoSession, VideoSessionParameters,
    },
    Frame, RawFrame, VulkanCtxError, VulkanDevice,
};

mod encode_parameter_sets;

#[derive(Debug, thiserror::Error)]
pub enum VulkanEncoderError {
    #[error("Vulkan error: {0}")]
    VkError(#[from] ash::vk::Result),

    #[error("Cannot find enough memory of the right type on the deivce")]
    NoMemory,

    #[error("The supplied textures format is {0:?}, when it should be NV12")]
    NotNV12Texture(wgpu::TextureFormat),

    #[error(transparent)]
    VulkanCtxError(#[from] VulkanCtxError),

    #[error("The byte length of the provided frame ({bytes}) is not the same as the picture size calculated from the dimensions ({size_from_resolution})")]
    InconsistentPictureDimensions {
        bytes: usize,
        size_from_resolution: usize,
    },

    #[error("The profile '{0:?}' is not supported by this device")]
    ProfileUnsupported(H264Profile),

    #[error("This device does not support the required capabilities: {0}")]
    UnsupportedDeviceCapabilities(&'static str),

    #[error("The requested dimensions are not divisible by 16, which is unsupported")]
    DimensionsNotDivisibleBy16,
}

struct VideoSessionResources<'a> {
    video_session: VideoSession,
    parameters: VideoSessionParameters,
    dpb: DecodedPicturesBuffer<'a>,
}

impl VideoSessionResources<'_> {
    fn new(
        device: &VulkanDevice,
        command_buffer: &CommandBuffer,
        profile: H264Profile,
        profile_info: &vk::VideoProfileInfoKHR,
        extent: vk::Extent2D,
    ) -> Result<Self, VulkanEncoderError> {
        let encode_capabilities = device
            .encode_capabilities
            .profile(profile)
            .ok_or(VulkanEncoderError::ProfileUnsupported(profile))?;

        let max_references = encode_capabilities
            .h264_encode_capabilities
            .max_p_picture_l0_reference_count;
        let max_dbp_slots = max_references + 1; // +1 for current picture

        let video_session = VideoSession::new(
            device,
            profile_info,
            extent,
            max_dbp_slots,
            max_references,
            vk::VideoSessionCreateFlagsKHR::ALLOW_ENCODE_PARAMETER_OPTIMIZATIONS,
            &encode_capabilities.video_capabilities.std_header_version,
        )?;

        let dpb = DecodedPicturesBuffer::new(
            device,
            command_buffer,
            &profile_info,
            encode_capabilities.encode_dpb_properties[0].image_usage_flags,
            &encode_capabilities.encode_dpb_properties[0],
            extent,
            max_dbp_slots,
            None,
            vk::ImageLayout::VIDEO_ENCODE_DPB_KHR,
        )?;

        let sps = sps(profile, extent.width, extent.height, max_references)?;
        let pps = pps();

        let parameters = VideoSessionParameters::new(
            device.device.clone(),
            video_session.session,
            &[sps],
            &[pps],
            None,
            true,
        )?;

        Ok(Self {
            video_session,
            dpb,
            parameters,
        })
    }
}

type H264EncodeProfileInfo<'a> = ProfileInfo<'a, vk::VideoEncodeH264ProfileInfoKHR<'a>>;

impl<'a> H264EncodeProfileInfo<'a> {
    fn new_encode(profile: H264Profile) -> Self {
        let h264_profile =
            vk::VideoEncodeH264ProfileInfoKHR::default().std_profile_idc(profile.to_profile_idc());

        let profile = vk::VideoProfileInfoKHR::default()
            .video_codec_operation(vk::VideoCodecOperationFlagsKHR::ENCODE_H264)
            .chroma_subsampling(vk::VideoChromaSubsamplingFlagsKHR::TYPE_420)
            .luma_bit_depth(vk::VideoComponentBitDepthFlagsKHR::TYPE_8)
            .chroma_bit_depth(vk::VideoComponentBitDepthFlagsKHR::TYPE_8);

        ProfileInfo::new(profile, h264_profile)
    }
}

struct EncodingQueryPool {
    pool: QueryPool,
}

impl std::ops::Deref for EncodingQueryPool {
    type Target = QueryPool;

    fn deref(&self) -> &Self::Target {
        &self.pool
    }
}

impl EncodingQueryPool {
    pub(crate) fn new(
        device: &VulkanDevice,
        profile: H264Profile,
        profile_info: vk::VideoProfileInfoKHR,
    ) -> Result<Self, VulkanEncoderError> {
        let encode_capabilities = device
            .encode_capabilities
            .profile(profile)
            .ok_or(VulkanEncoderError::ProfileUnsupported(profile))?;

        if !encode_capabilities
            .encode_capabilities
            .supported_encode_feedback_flags
            .contains(vk::VideoEncodeFeedbackFlagsKHR::BITSTREAM_BYTES_WRITTEN)
        {
            return Err(VulkanEncoderError::UnsupportedDeviceCapabilities(
                "VkVideoEncodeFeedbackFlagsKHR::BITSTREAM_BYTES_WRITTEN",
            ));
        }

        let pool = QueryPool::new(
            device.device.clone(),
            vk::QueryType::VIDEO_ENCODE_FEEDBACK_KHR,
            1,
            Some(profile_info),
            Some(
                vk::QueryPoolVideoEncodeFeedbackCreateInfoKHR::default().encode_feedback_flags(
                    vk::VideoEncodeFeedbackFlagsKHR::BITSTREAM_BYTES_WRITTEN,
                ),
            ),
        )?;

        Ok(Self { pool })
    }

    pub(crate) fn get_result_blocking(&self) -> Result<vk::QueryResultStatusKHR, VulkanCtxError> {
        todo!();
    }
}

struct CommandBuffers {
    encode_buffer: CommandBuffer,
    transfer_buffer: CommandBuffer,
}

struct CommandPools {
    encode_pool: Arc<CommandPool>,
    transfer_pool: Arc<CommandPool>,
}

struct SyncStructures {
    fence_done: Fence,
    sem_transfer_done: Semaphore,
}

pub struct VulkanEncoder<'a> {
    device: Arc<VulkanDevice>,
    _command_pools: CommandPools,
    command_buffers: CommandBuffers,
    sync_structures: SyncStructures,
    query_pool: EncodingQueryPool,
    profile_info: H264EncodeProfileInfo<'a>,
    session_resources: VideoSessionResources<'a>,
}

impl VulkanEncoder<'_> {
    // TODO: make resolution variable
    pub fn new(
        device: Arc<VulkanDevice>,
        profile: H264Profile,
        width: u32,
        height: u32,
    ) -> Result<Self, VulkanEncoderError> {
        let profile_info = H264EncodeProfileInfo::new_encode(profile);
        let command_pools = CommandPools {
            encode_pool: CommandPool::new(device.clone(), device.queues.h264_encode.idx)?.into(),
            transfer_pool: CommandPool::new(device.clone(), device.queues.transfer.idx)?.into(),
        };

        let command_buffers = CommandBuffers {
            encode_buffer: CommandBuffer::new_primary(command_pools.encode_pool.clone())?,
            transfer_buffer: CommandBuffer::new_primary(command_pools.transfer_pool.clone())?,
        };

        let sync_structures = SyncStructures {
            fence_done: Fence::new(device.device.clone(), false)?,
            sem_transfer_done: Semaphore::new(device.device.clone())?,
        };

        let query_pool = EncodingQueryPool::new(&device, profile, profile_info.profile_info)?;

        command_buffers.encode_buffer.begin()?;

        let session_resources = VideoSessionResources::new(
            &device,
            &command_buffers.encode_buffer,
            profile,
            &profile_info.profile_info,
            vk::Extent2D { width, height },
        )?;

        command_buffers.encode_buffer.end()?;

        device.queues.h264_encode.submit(
            &command_buffers.encode_buffer,
            &[],
            &[],
            Some(*sync_structures.fence_done),
        )?;

        sync_structures.fence_done.wait_and_reset(u64::MAX)?;

        Ok(Self {
            profile_info,
            device,
            _command_pools: command_pools,
            command_buffers,
            sync_structures,
            query_pool,
            session_resources,
        })
    }

    pub fn encode_bytes(&mut self, frame: Frame<RawFrame>) -> Result<Vec<u8>, VulkanEncoderError> {
        let extent = vk::Extent3D {
            width: frame.frame.width,
            height: frame.frame.height,
            depth: 1,
        };

        if frame.frame.width as usize * frame.frame.height as usize * 3 / 2
            != frame.frame.data.len()
        {
            return Err(VulkanEncoderError::InconsistentPictureDimensions {
                bytes: frame.frame.data.len(),
                size_from_resolution: frame.frame.width as usize * frame.frame.height as usize * 3
                    / 2,
            });
        }

        let mut profile_list_info = vk::VideoProfileListInfoKHR::default()
            .profiles(std::slice::from_ref(&self.profile_info.profile_info));

        let queue_family_indices = [
            self.device.queues.transfer.idx as u32,
            self.device.queues.h264_encode.idx as u32,
        ];

        let image_create_info = vk::ImageCreateInfo::default()
            .flags(vk::ImageCreateFlags::empty())
            .image_type(vk::ImageType::TYPE_2D)
            .format(vk::Format::G8_B8R8_2PLANE_420_UNORM)
            .extent(extent)
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::VIDEO_ENCODE_SRC_KHR | vk::ImageUsageFlags::TRANSFER_DST)
            .sharing_mode(vk::SharingMode::CONCURRENT)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .queue_family_indices(&queue_family_indices)
            .push_next(&mut profile_list_info);

        let mut image = Image::new(self.device.allocator.clone(), &image_create_info)?;

        self.command_buffers.transfer_buffer.begin()?;

        image.transition_layout_single_layer(
            &self.command_buffers.transfer_buffer,
            vk::PipelineStageFlags2::NONE,
            vk::AccessFlags2::NONE,
            vk::PipelineStageFlags2::COPY,
            vk::AccessFlags2::TRANSFER_WRITE,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            0,
        )?;

        let buffer =
            Buffer::new_transfer_with_data(self.device.allocator.clone(), &frame.frame.data)?;

        unsafe {
            self.device.device.cmd_copy_buffer_to_image(
                *self.command_buffers.transfer_buffer,
                *buffer,
                *image,
                image.layout[0],
                &[
                    vk::BufferImageCopy::default()
                        .buffer_offset(0)
                        .buffer_row_length(0)
                        .buffer_image_height(0)
                        .image_subresource(vk::ImageSubresourceLayers {
                            aspect_mask: vk::ImageAspectFlags::PLANE_0,
                            layer_count: 1,
                            base_array_layer: 0,
                            mip_level: 0,
                        })
                        .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
                        .image_extent(vk::Extent3D {
                            width: frame.frame.width,
                            height: frame.frame.height,
                            depth: 1,
                        }),
                    vk::BufferImageCopy::default()
                        .buffer_offset(frame.frame.width as u64 * frame.frame.height as u64)
                        .buffer_row_length(0)
                        .buffer_image_height(0)
                        .image_subresource(vk::ImageSubresourceLayers {
                            aspect_mask: vk::ImageAspectFlags::PLANE_1,
                            layer_count: 1,
                            base_array_layer: 0,
                            mip_level: 0,
                        })
                        .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
                        .image_extent(vk::Extent3D {
                            width: frame.frame.width / 2,
                            height: frame.frame.height / 2,
                            depth: 1,
                        }),
                ],
            );
        }

        self.command_buffers.transfer_buffer.end()?;

        self.device.queues.transfer.submit(
            &self.command_buffers.transfer_buffer,
            &[],
            &[(
                *self.sync_structures.sem_transfer_done,
                vk::PipelineStageFlags2::COPY,
            )],
            None,
        )?;

        self.command_buffers.encode_buffer.begin()?;

        image.transition_layout_single_layer(
            &self.command_buffers.encode_buffer,
            vk::PipelineStageFlags2::NONE,
            vk::AccessFlags2::NONE,
            vk::PipelineStageFlags2::VIDEO_ENCODE_KHR,
            vk::AccessFlags2::VIDEO_ENCODE_READ_KHR,
            vk::ImageLayout::VIDEO_ENCODE_SRC_KHR,
            0,
        )?;

        unsafe {
            self.device
                .device
                .video_queue_ext
                .cmd_begin_video_coding_khr(
                    *self.command_buffers.encode_buffer,
                    &vk::VideoBeginCodingInfoKHR::default()
                        .video_session(self.session_resources.video_session.session)
                        .video_session_parameters(self.session_resources.parameters.parameters)
                        .reference_slots(&self.session_resources.dpb.reference_slot_info()),
                );
        }

        // TODO: this

        unsafe {
            self.device.device.video_queue_ext.cmd_end_video_coding_khr(
                *self.command_buffers.encode_buffer,
                &vk::VideoEndCodingInfoKHR::default(),
            );
        }

        self.command_buffers.encode_buffer.end()?;

        self.device.queues.h264_encode.submit(
            &self.command_buffers.encode_buffer,
            &[(
                *self.sync_structures.sem_transfer_done,
                vk::PipelineStageFlags2::VIDEO_ENCODE_KHR,
            )],
            &[],
            Some(*self.sync_structures.fence_done),
        )?;

        let start = std::time::Instant::now();
        self.sync_structures.fence_done.wait_and_reset(u64::MAX)?;

        println!("fence triggered after {}s", start.elapsed().as_secs_f64());

        todo!();
    }
}
