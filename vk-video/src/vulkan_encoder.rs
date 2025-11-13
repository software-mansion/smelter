use std::{
    collections::VecDeque,
    num::NonZeroU32,
    sync::{Arc, Mutex},
};

use ash::vk;
use encode_parameter_sets::{pps, sps};
use yuv_converter::Converter;

use crate::{
    EncodedOutputChunk, Frame, RawFrameData, VulkanCommonError,
    device::{EncodingDevice, Rational},
    parameters::H264Profile,
    wrappers::{
        Buffer, CommandBufferPool, CommandBufferPoolStorage, DecodedPicturesBuffer, Image,
        ImageLayoutTracker, ImageView, OpenCommandBuffer, ProfileInfo, QueryPool, Tracker,
        TrackerKind, VideoEncodeQueueExt, VideoQueueExt, VideoSession, VideoSessionParameters,
    },
};

mod encode_parameter_sets;
pub(crate) mod yuv_converter;

const MB: u64 = 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum VulkanEncoderError {
    #[error("Vulkan error: {0}")]
    VkError(#[from] ash::vk::Result),

    #[error("Cannot find enough memory of the right type on the deivce")]
    NoMemory,

    #[error("The supplied textures format is {0:?}, when it should be NV12")]
    NotNV12Texture(wgpu::TextureFormat),

    #[error(transparent)]
    VulkanCommonError(#[from] VulkanCommonError),

    #[error("The device does not support vulkan h264 encoding")]
    VulkanEncoderUnsupported,

    #[error(
        "The byte length of the provided frame ({bytes}) is not the same as the picture size calculated from the dimensions ({size_from_resolution})"
    )]
    InconsistentPictureDimensions {
        bytes: usize,
        size_from_resolution: usize,
    },

    #[error("The profile '{0:?}' is not supported by this device")]
    ProfileUnsupported(H264Profile),

    #[error("This device does not support the required capabilities: {0}")]
    UnsupportedDeviceCapabilities(&'static str),

    #[error("Encode operation failed with status {0:?}")]
    EncodeOperationFailed(vk::QueryResultStatusKHR),

    #[error("Invalid encoder parameters, field: {field} - problem: {problem}")]
    ParametersError {
        field: &'static str,
        problem: String,
    },
}

struct VideoSessionResources<'a> {
    max_dpb_slots: u32,
    video_session: VideoSession,
    parameters: VideoSessionParameters,
    dpb: DecodedPicturesBuffer<'a>,
    quality_level: u32,
    rate_control: RateControl,
    framerate: Rational,
}

impl VideoSessionResources<'_> {
    fn new(
        encoding_device: &EncodingDevice,
        command_buffer: &mut OpenCommandBuffer,
        image_tracker: Arc<Mutex<ImageLayoutTracker>>,
        parameters: FullEncoderParameters,
        profile_info: &vk::VideoProfileInfoKHR,
    ) -> Result<Self, VulkanEncoderError> {
        let encode_capabilities = encoding_device
            .native_encode_capabilities
            .profile(parameters.profile)
            .ok_or(VulkanEncoderError::ProfileUnsupported(parameters.profile))?;

        let extent = vk::Extent2D {
            width: parameters.width.get(),
            height: parameters.height.get(),
        };

        let max_references = parameters.max_references.get();
        let max_dpb_slots = max_references + 1; // +1 for current picture

        let video_session = VideoSession::new(
            &encoding_device.vulkan_device,
            &encoding_device.h264_encode_queue,
            profile_info,
            extent,
            max_dpb_slots,
            max_references,
            vk::VideoSessionCreateFlagsKHR::ALLOW_ENCODE_PARAMETER_OPTIMIZATIONS,
            &encode_capabilities.video_capabilities.std_header_version,
        )?;

        let use_separate_images = encoding_device
            .native_encode_capabilities
            .profile(parameters.profile)
            .unwrap()
            .video_capabilities
            .flags
            .contains(vk::VideoCapabilityFlagsKHR::SEPARATE_REFERENCE_IMAGES);

        let dpb = DecodedPicturesBuffer::new(
            encoding_device,
            command_buffer,
            image_tracker,
            use_separate_images,
            profile_info,
            vk::ImageUsageFlags::VIDEO_ENCODE_DPB_KHR,
            &encode_capabilities.encode_dpb_properties[0],
            extent,
            max_dpb_slots,
            None,
            vk::ImageLayout::VIDEO_ENCODE_DPB_KHR,
        )?;

        let sps = sps(
            parameters.profile,
            extent.width,
            extent.height,
            max_references,
        )?;
        let pps = pps();

        let session_parameters = VideoSessionParameters::new(
            encoding_device.vulkan_device.device.clone(),
            video_session.session,
            &[sps],
            &[pps],
            None,
            Some(parameters.quality_level),
        )?;

        Ok(Self {
            video_session,
            dpb,
            parameters: session_parameters,
            max_dpb_slots,
            quality_level: parameters.quality_level,
            rate_control: RateControl::EncoderDefault,
            framerate: parameters.framerate,
        })
    }
}

pub(crate) type H264EncodeProfileInfo<'a> = ProfileInfo<'a>;

impl H264EncodeProfileInfo<'_> {
    pub(crate) fn new_encode(parameters: &FullEncoderParameters) -> Self {
        let h264_profile = vk::VideoEncodeH264ProfileInfoKHR::default()
            .std_profile_idc(parameters.profile.to_profile_idc());

        let profile = vk::VideoProfileInfoKHR::default()
            .video_codec_operation(vk::VideoCodecOperationFlagsKHR::ENCODE_H264)
            .chroma_subsampling(vk::VideoChromaSubsamplingFlagsKHR::TYPE_420)
            .luma_bit_depth(vk::VideoComponentBitDepthFlagsKHR::TYPE_8)
            .chroma_bit_depth(vk::VideoComponentBitDepthFlagsKHR::TYPE_8);

        let h264_profile = Box::new(h264_profile);

        let usage_info = vk::VideoEncodeUsageInfoKHR::default()
            .video_usage_hints(parameters.usage_flags)
            .tuning_mode(parameters.tuning_mode)
            .video_content_hints(parameters.content_flags);

        let usage_info = Box::new(usage_info);

        ProfileInfo::new(profile, vec![h264_profile, usage_info])
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
        encoding_device: &EncodingDevice,
        profile: H264Profile,
        profile_info: vk::VideoProfileInfoKHR,
    ) -> Result<Self, VulkanEncoderError> {
        let encode_capabilities = encoding_device
            .native_encode_capabilities
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
            encoding_device.vulkan_device.device.clone(),
            vk::QueryType::VIDEO_ENCODE_FEEDBACK_KHR,
            1,
            Some(profile_info),
            Some(
                vk::QueryPoolVideoEncodeFeedbackCreateInfoKHR::default().encode_feedback_flags(
                    vk::VideoEncodeFeedbackFlagsKHR::BITSTREAM_BYTES_WRITTEN
                        | vk::VideoEncodeFeedbackFlagsKHR::BITSTREAM_BUFFER_OFFSET,
                ),
            ),
        )?;

        Ok(Self { pool })
    }

    pub(crate) fn get_result_blocking(&self) -> Result<EncodeFeedback, VulkanEncoderError> {
        let mut result = [EncodeFeedback {
            offset: 0,
            bytes_written: 0,
            status: vk::QueryResultStatusKHR::NOT_READY,
        }];

        unsafe {
            self.pool.device.get_query_pool_results(
                self.pool.pool,
                0,
                &mut result,
                vk::QueryResultFlags::WAIT | vk::QueryResultFlags::WITH_STATUS_KHR,
            )?
        };

        Ok(result[0])
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct EncodeFeedback {
    offset: u32,
    bytes_written: u32,
    status: vk::QueryResultStatusKHR,
}

pub(crate) enum EncoderTrackerWaitState {
    InitializeEncoder,
    CopyBufferToImage,
    Convert,
    Encode,
}

struct EncoderCommandBufferPools {
    transfer: CommandBufferPool,
    encode: CommandBufferPool,
}

impl EncoderCommandBufferPools {
    fn new(device: &EncodingDevice) -> Result<Self, VulkanEncoderError> {
        let transfer =
            CommandBufferPool::new(device.vulkan_device.clone(), device.queues.transfer.idx)?;
        let encode =
            CommandBufferPool::new(device.vulkan_device.clone(), device.h264_encode_queue.idx)?;

        Ok(Self { transfer, encode })
    }
}

impl CommandBufferPoolStorage for EncoderCommandBufferPools {
    fn mark_submitted_as_free(&mut self) {
        self.transfer.mark_submitted_as_free();
        self.encode.mark_submitted_as_free();
    }
}

struct EncoderTrackerKind {}

impl TrackerKind for EncoderTrackerKind {
    type WaitState = EncoderTrackerWaitState;

    type CommandBufferPools = EncoderCommandBufferPools;
}

type EncoderTracker = Tracker<EncoderTrackerKind>;

pub struct VulkanEncoder<'a> {
    tracker: EncoderTracker,
    query_pool: EncodingQueryPool,
    profile: H264Profile,
    profile_info: H264EncodeProfileInfo<'a>,
    session_resources: VideoSessionResources<'a>,
    idr_period_counter: u32,
    idr_period: u32,
    output_buffer: Buffer,
    idr_pic_id: u16,
    frame_num: u32,
    pic_order_cnt: u8,
    active_reference_slots: VecDeque<(usize, vk::native::StdVideoEncodeH264ReferenceInfo)>,
    rate_control: RateControl,
    converter: Option<Converter>,
    encoding_device: Arc<EncodingDevice>,
}

#[derive(Debug, Clone, Copy)]
pub struct FullEncoderParameters {
    pub(crate) idr_period: NonZeroU32,
    pub(crate) width: NonZeroU32,
    pub(crate) height: NonZeroU32,
    pub(crate) rate_control: RateControl,
    pub(crate) max_references: NonZeroU32,
    pub(crate) profile: H264Profile,
    pub(crate) quality_level: u32,
    pub(crate) framerate: Rational,
    pub(crate) usage_flags: vk::VideoEncodeUsageFlagsKHR,
    pub(crate) tuning_mode: vk::VideoEncodeTuningModeKHR,
    pub(crate) content_flags: vk::VideoEncodeContentFlagsKHR,
}

impl VulkanEncoder<'_> {
    const OUTPUT_BUFFER_LEN: u64 = 4 * MB;

    pub(crate) fn new_with_converter(
        device: Arc<EncodingDevice>,
        parameters: FullEncoderParameters,
    ) -> Result<Self, VulkanEncoderError> {
        let mut enc = Self::new(device.clone(), parameters)?;

        let conv = Converter::new(
            device.clone(),
            parameters.width.get(),
            parameters.height.get(),
            &enc.profile_info,
            enc.tracker.image_layout_tracker.clone(),
        )
        .unwrap();

        enc.converter = Some(conv);

        Ok(enc)
    }

    pub(crate) fn new(
        encoding_device: Arc<EncodingDevice>,
        parameters: FullEncoderParameters,
    ) -> Result<Self, VulkanEncoderError> {
        let profile_info = H264EncodeProfileInfo::new_encode(&parameters);

        let command_buffer_pools = EncoderCommandBufferPools::new(&encoding_device)?;
        let mut tracker =
            EncoderTracker::new(encoding_device.device.clone(), command_buffer_pools)?;

        let query_pool = EncodingQueryPool::new(
            &encoding_device,
            parameters.profile,
            profile_info.profile_info,
        )?;

        // TODO: this buffer should grow when necessary
        let output_buffer = Buffer::new_encode(
            encoding_device.allocator.clone(),
            Self::OUTPUT_BUFFER_LEN,
            &profile_info,
        )?;

        let mut buffer = tracker.command_buffer_pools.encode.begin_buffer()?;

        let session_resources = VideoSessionResources::new(
            &encoding_device,
            &mut buffer,
            tracker.image_layout_tracker.clone(),
            parameters,
            &profile_info.profile_info,
        )?;

        encoding_device.h264_encode_queue.submit_chain_semaphore(
            buffer.end()?,
            &mut tracker,
            vk::PipelineStageFlags2::ALL_COMMANDS,
            vk::PipelineStageFlags2::ALL_COMMANDS,
            EncoderTrackerWaitState::InitializeEncoder,
        )?;

        Ok(Self {
            idr_period_counter: 0,
            idr_pic_id: 0,
            frame_num: 0,
            pic_order_cnt: 0,
            active_reference_slots: VecDeque::with_capacity(session_resources.dpb.len as usize),
            profile: parameters.profile,
            profile_info,
            encoding_device,
            tracker,
            query_pool,
            session_resources,
            idr_period: parameters.idr_period.get(),
            output_buffer,
            rate_control: parameters.rate_control,
            converter: None,
        })
    }

    fn begin_video_coding(&self, buffer: vk::CommandBuffer) {
        let mut h264_layers =
            self.h264_rate_control_layers_for(self.session_resources.rate_control);
        let layers = self.rate_control_layers_for(
            self.session_resources.rate_control,
            h264_layers.as_mut().map(|o| &mut o[..]),
        );
        let mut h264_rate_control = self.h264_rate_control(layers.as_ref().map(|o| &o[..]));
        let mut encode_rate_control = self.encoder_rate_control_for(
            self.session_resources.rate_control,
            layers.as_ref().map(|o| &o[..]),
        );

        let mut reference_slot_info = self.session_resources.dpb.reference_slot_info();
        reference_slot_info.sort_by_key(|s| {
            if s.slot_index == -1 {
                return usize::MAX;
            }

            let (i, _) = self
                .active_reference_slots
                .iter()
                .enumerate()
                .find(|(_, (slot_idx, _))| (*slot_idx) as i32 == s.slot_index)
                .unwrap();

            i
        });

        // Absolutely crucial for nvidia GPUs, nothing works without this.
        reference_slot_info.reverse();

        let mut begin_info = vk::VideoBeginCodingInfoKHR::default()
            .video_session(self.session_resources.video_session.session)
            .video_session_parameters(self.session_resources.parameters.parameters)
            .reference_slots(&reference_slot_info);

        if let (Some(encode_rate_control), Some(h264_rate_control)) =
            (encode_rate_control.as_mut(), h264_rate_control.as_mut())
        {
            begin_info = begin_info
                .push_next(encode_rate_control)
                .push_next(h264_rate_control);
        }

        unsafe {
            self.encoding_device
                .vulkan_device
                .device
                .video_queue_ext
                .cmd_begin_video_coding_khr(buffer, &begin_info);
        }
    }

    fn issue_coding_control_reset_for(
        &mut self,
        buffer: vk::CommandBuffer,
        rate_control: RateControl,
    ) {
        let mut quality_level = vk::VideoEncodeQualityLevelInfoKHR::default()
            .quality_level(self.session_resources.quality_level);

        let mut h264_layers = self.h264_rate_control_layers_for(rate_control);
        let layers =
            self.rate_control_layers_for(rate_control, h264_layers.as_mut().map(|o| &mut o[..]));
        let mut h264_rate_control = self.h264_rate_control(layers.as_ref().map(|o| &o[..]));
        let mut encode_rate_control =
            self.encoder_rate_control_for(rate_control, layers.as_ref().map(|o| &o[..]));

        let mut flags = vk::VideoCodingControlFlagsKHR::RESET
            | vk::VideoCodingControlFlagsKHR::ENCODE_QUALITY_LEVEL;

        if encode_rate_control.is_some() {
            flags |= vk::VideoCodingControlFlagsKHR::ENCODE_RATE_CONTROL;
        }

        let mut control_info = vk::VideoCodingControlInfoKHR::default()
            .flags(flags)
            .push_next(&mut quality_level);

        if let (Some(encode_rate_control), Some(h264_rate_control)) =
            (encode_rate_control.as_mut(), h264_rate_control.as_mut())
        {
            control_info = control_info
                .push_next(h264_rate_control)
                .push_next(encode_rate_control);
        }

        unsafe {
            self.encoding_device
                .vulkan_device
                .device
                .video_queue_ext
                .cmd_control_video_coding_khr(buffer, &control_info);
        }

        self.session_resources.rate_control = rate_control;
    }

    fn transfer_buffer_to_image(
        &mut self,
        frame: &Frame<RawFrameData>,
    ) -> Result<(Image, Buffer), VulkanEncoderError> {
        let extent = vk::Extent3D {
            width: frame.data.width,
            height: frame.data.height,
            depth: 1,
        };

        if frame.data.width as usize * frame.data.height as usize * 3 / 2 != frame.data.frame.len()
        {
            return Err(VulkanEncoderError::InconsistentPictureDimensions {
                bytes: frame.data.frame.len(),
                size_from_resolution: frame.data.width as usize * frame.data.height as usize * 3
                    / 2,
            });
        }

        let mut profile_list_info = vk::VideoProfileListInfoKHR::default()
            .profiles(std::slice::from_ref(&self.profile_info.profile_info));

        let queue_family_indices = [
            self.encoding_device.queues.transfer.idx as u32,
            self.encoding_device.h264_encode_queue.idx as u32,
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

        let image = Image::new(
            self.encoding_device.allocator.clone(),
            &image_create_info,
            self.tracker.image_layout_tracker.clone(),
        )?;

        let mut cmd_buffer = self.tracker.command_buffer_pools.transfer.begin_buffer()?;

        image.transition_layout_single_layer(
            &mut cmd_buffer,
            vk::PipelineStageFlags2::NONE..vk::PipelineStageFlags2::COPY,
            vk::AccessFlags2::NONE..vk::AccessFlags2::TRANSFER_WRITE,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            0,
        )?;

        let buffer = Buffer::new_transfer_with_data(
            self.encoding_device.allocator.clone(),
            &frame.data.frame,
        )?;

        unsafe {
            self.encoding_device
                .vulkan_device
                .device
                .cmd_copy_buffer_to_image(
                    cmd_buffer.buffer(),
                    *buffer,
                    *image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
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
                                width: frame.data.width,
                                height: frame.data.height,
                                depth: 1,
                            }),
                        vk::BufferImageCopy::default()
                            .buffer_offset(frame.data.width as u64 * frame.data.height as u64)
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
                                width: frame.data.width / 2,
                                height: frame.data.height / 2,
                                depth: 1,
                            }),
                    ],
                );
        }

        self.encoding_device
            .queues
            .transfer
            .submit_chain_semaphore(
                cmd_buffer.end()?,
                &mut self.tracker,
                vk::PipelineStageFlags2::COPY,
                vk::PipelineStageFlags2::COPY,
                EncoderTrackerWaitState::CopyBufferToImage,
            )?;

        Ok((image, buffer))
    }

    fn encode(
        &mut self,
        image: Arc<Image>,
        force_idr: bool,
    ) -> Result<Vec<u8>, VulkanEncoderError> {
        let is_idr = force_idr || self.idr_period_counter == 0;
        let mut idr_pic_id = 0;

        if is_idr {
            self.idr_period_counter = 0;
            idr_pic_id = self.idr_pic_id;
            self.idr_pic_id = self.idr_pic_id.wrapping_add(1);
            self.frame_num = 0;
            self.pic_order_cnt = 0;
            self.active_reference_slots.clear();
            self.session_resources.dpb.reset_all_allocations();
        } else if self.active_reference_slots.len() == self.session_resources.max_dpb_slots as usize
        {
            if let Some((oldest_reference, _)) = self.active_reference_slots.pop_front() {
                self.session_resources
                    .dpb
                    .free_reference_picture(oldest_reference);
            }
        }

        let mut cmd_buffer = self.tracker.command_buffer_pools.encode.begin_buffer()?;

        image.transition_layout_single_layer(
            &mut cmd_buffer,
            vk::PipelineStageFlags2::NONE..vk::PipelineStageFlags2::VIDEO_ENCODE_KHR,
            vk::AccessFlags2::NONE..vk::AccessFlags2::VIDEO_ENCODE_READ_KHR,
            vk::ImageLayout::VIDEO_ENCODE_SRC_KHR,
            0,
        )?;

        let mut view_usage_create_info = vk::ImageViewUsageCreateInfo::default()
            .usage(vk::ImageUsageFlags::VIDEO_ENCODE_SRC_KHR);

        let view_create_info = vk::ImageViewCreateInfo::default()
            .flags(vk::ImageViewCreateFlags::empty())
            .image(image.image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(vk::Format::G8_B8R8_2PLANE_420_UNORM)
            .components(vk::ComponentMapping::default())
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                level_count: 1,
                base_mip_level: 0,
                layer_count: 1,
                base_array_layer: 0,
            })
            .push_next(&mut view_usage_create_info);

        let view = ImageView::new(
            self.encoding_device.vulkan_device.device.clone(),
            image.clone(),
            &view_create_info,
        )?;

        self.query_pool.reset(cmd_buffer.buffer());

        self.begin_video_coding(cmd_buffer.buffer());

        if is_idr {
            // TODO: controllable rate control, framerate and all stream parameters
            self.issue_coding_control_reset_for(cmd_buffer.buffer(), self.rate_control);
        }

        let frame_num = self.frame_num;
        self.frame_num = self.frame_num.wrapping_add(1);

        let pic_order_cnt = self.pic_order_cnt;
        self.pic_order_cnt = self.pic_order_cnt.wrapping_add(2);

        // bugs in nvidia driver I encountered on this journey:
        //
        // bug1: if primary pic type is set to I instead of IDR, the encode command will submit
        // successfully, the fence will trigger, signalling it has been executed, but if you then
        // query the implementation for the status of the operation, it will behave as though the
        // operation never happened (which means it will not return an error!). The division
        // between I and IDR is invented in the vulkan spec, in h264 the values are equivalent.
        //
        // bug2: when rate control is disabled, you have to specify the temporal layer count to 0.
        // You pass a table length and a pointer to a bable with temporal layer descriptions. Even
        // when the length is set to 0, the pointer will be dereferenced. If you set it to NULL,
        // the program will (obviously) segfault.
        //
        // bug3: each dpb reference picture has to be in a separate VkImage, even though the spec
        // says these can be different layers of the same image (even though using layers of one
        // picture works in the decoder)
        //
        // bug4: when you pass the information about which decoded pictures buffer slots contain
        // references, the spec does not specify the order in which they should be arranged. The
        // internal implementation expects a very specific order though: from the most recent to
        // the oldest. It was natural for me to keep references in a FIFO queue, where I append
        // new pictures to the back and pop old pictures from the front when they're no longer
        // needed. After hours of trying to figure out what the problem was I jokingly said to a
        // colleague that we should just try reversing the order we have. It ended up working.
        // I don't know how anyone is supposed to find this.

        let primary_pic_type = if is_idr {
            vk::native::StdVideoH264PictureType_STD_VIDEO_H264_PICTURE_TYPE_IDR
        } else {
            vk::native::StdVideoH264PictureType_STD_VIDEO_H264_PICTURE_TYPE_P
        };

        let slice_header = vk::native::StdVideoEncodeH264SliceHeader {
            flags: vk::native::StdVideoEncodeH264SliceHeaderFlags {
                _bitfield_align_1: [],
                _bitfield_1: vk::native::StdVideoEncodeH264SliceHeaderFlags::new_bitfield_1(
                    1, // TODO: b-frames
                    1, // TODO: don't override always
                    0,
                ),
            },
            first_mb_in_slice: 0,
            slice_type: if is_idr {
                vk::native::StdVideoH264SliceType_STD_VIDEO_H264_SLICE_TYPE_I
            } else {
                vk::native::StdVideoH264SliceType_STD_VIDEO_H264_SLICE_TYPE_P
            }, // TODO: b-frames
            slice_alpha_c0_offset_div2: 0,
            slice_beta_offset_div2: 0,
            slice_qp_delta: 0, // TODO: check whether this will be overwritten in the bitstream
            reserved1: 0,
            cabac_init_idc: vk::native::StdVideoH264CabacInitIdc_STD_VIDEO_H264_CABAC_INIT_IDC_0, // TODO: check whether this will be overwritten in the bitstream
            disable_deblocking_filter_idc: 0, // TODO: enable for fast decoding?
            pWeightTable: std::ptr::null(),
        };

        let mut nalu_slice_entries =
            [vk::VideoEncodeH264NaluSliceInfoKHR::default().std_slice_header(&slice_header)];

        if let RateControl::Disabled = self.rate_control {
            if let Some(caps) = self
                .encoding_device
                .native_encode_capabilities
                .profile(self.profile)
            {
                let quality_properties =
                    &caps.quality_level_properties[self.session_resources.quality_level as usize];

                if !quality_properties.zeroed() {
                    let qp = quality_properties
                        .h264_quality_level_properties
                        .preferred_constant_qp;

                    if is_idr {
                        nalu_slice_entries[0].constant_qp = qp.qp_i;
                    } else {
                        nalu_slice_entries[0].constant_qp = qp.qp_p;
                    }
                }
            }
        }

        let mut ref_list0 = [0xff; 32];
        for (i, (slot, _)) in self.active_reference_slots.iter().rev().enumerate() {
            ref_list0[i] = *slot as u8;
        }

        let ref_lists = vk::native::StdVideoEncodeH264ReferenceListsInfo {
            flags: vk::native::StdVideoEncodeH264ReferenceListsInfoFlags {
                _bitfield_align_1: [],
                _bitfield_1: vk::native::StdVideoEncodeH264ReferenceListsInfoFlags::new_bitfield_1(
                    0, 0, 0,
                ),
            },
            num_ref_idx_l0_active_minus1: self.active_reference_slots.len().saturating_sub(1) as u8,
            num_ref_idx_l1_active_minus1: 0,
            RefPicList0: ref_list0,
            RefPicList1: [0xff; 32],
            refList0ModOpCount: 0,
            refList1ModOpCount: 0,
            refPicMarkingOpCount: 0,
            reserved1: [0; 7],
            pRefList0ModOperations: std::ptr::null(),
            pRefList1ModOperations: std::ptr::null(),
            pRefPicMarkingOperations: std::ptr::null(),
        };

        let std_h264_encode_info = vk::native::StdVideoEncodeH264PictureInfo {
            flags: vk::native::StdVideoEncodeH264PictureInfoFlags {
                _bitfield_align_1: [],
                _bitfield_1: vk::native::StdVideoEncodeH264PictureInfoFlags::new_bitfield_1(
                    is_idr as u32,
                    1, // TODO
                    is_idr as u32,
                    0, // long term refs
                    0, // adaptive reference control
                    0,
                ),
            },
            seq_parameter_set_id: 0,
            pic_parameter_set_id: 0,
            idr_pic_id,
            primary_pic_type,
            frame_num,
            PicOrderCnt: pic_order_cnt as i32,
            temporal_id: 0,
            reserved1: [0; 3],
            pRefLists: &ref_lists,
        };

        let mut h264_encode_info = vk::VideoEncodeH264PictureInfoKHR::default()
            .nalu_slice_entries(&nalu_slice_entries)
            .generate_prefix_nalu(false)
            .std_picture_info(&std_h264_encode_info);

        let setup_reference_slot_idx = self.session_resources.dpb.allocate_reference_picture()?;

        let mut reference_slots = self
            .session_resources
            .dpb
            .reference_slot_info()
            .into_iter()
            .filter(|i| i.slot_index >= 0 && i.slot_index != setup_reference_slot_idx as i32)
            .collect::<Vec<_>>();

        let mut std_reference_info = self
            .active_reference_slots
            .iter()
            .rev()
            .map(|(i, info)| {
                (
                    *i,
                    vk::VideoEncodeH264DpbSlotInfoKHR::default().std_reference_info(info),
                )
            })
            .collect::<Vec<_>>();

        std_reference_info.iter_mut().for_each(|(i, std_info)| {
            let slot = reference_slots
                .iter_mut()
                .find(|reference_slot| reference_slot.slot_index == (*i) as i32)
                .unwrap();
            *slot = slot.push_next(std_info);
        });

        let std_new_slot_reference_info = vk::native::StdVideoEncodeH264ReferenceInfo {
            flags: vk::native::StdVideoEncodeH264ReferenceInfoFlags {
                _bitfield_align_1: [],
                _bitfield_1: vk::native::StdVideoEncodeH264ReferenceInfoFlags::new_bitfield_1(0, 0),
            },
            primary_pic_type,
            FrameNum: frame_num,
            PicOrderCnt: pic_order_cnt as i32,
            long_term_pic_num: 0,
            long_term_frame_idx: 0,
            temporal_id: 0,
        };

        let mut new_slot_reference_info = vk::VideoEncodeH264DpbSlotInfoKHR::default()
            .std_reference_info(&std_new_slot_reference_info);

        let setup_reference_slot_video_resource_info = self
            .session_resources
            .dpb
            .video_resource_info(setup_reference_slot_idx)
            .unwrap();

        let setup_reference_slot = vk::VideoReferenceSlotInfoKHR::default()
            .slot_index(setup_reference_slot_idx as i32)
            .picture_resource(setup_reference_slot_video_resource_info)
            .push_next(&mut new_slot_reference_info);

        let extent = image.extent;

        let src_picture_resource = vk::VideoPictureResourceInfoKHR::default()
            .coded_offset(vk::Offset2D::default())
            .coded_extent(vk::Extent2D {
                width: extent.width,
                height: extent.height,
            })
            .base_array_layer(0)
            .image_view_binding(view.view);

        let mut encode_info = vk::VideoEncodeInfoKHR::default()
            .dst_buffer(self.output_buffer.buffer)
            .dst_buffer_range(Self::OUTPUT_BUFFER_LEN)
            .dst_buffer_offset(0)
            .src_picture_resource(src_picture_resource)
            .setup_reference_slot(&setup_reference_slot)
            .push_next(&mut h264_encode_info);

        if !reference_slots.is_empty() {
            encode_info = encode_info.reference_slots(&reference_slots);
        }

        self.query_pool.begin_query(cmd_buffer.buffer());

        unsafe {
            self.encoding_device
                .vulkan_device
                .device
                .video_encode_queue_ext
                .cmd_encode_video_khr(cmd_buffer.buffer(), &encode_info);
        }

        self.query_pool.end_query(cmd_buffer.buffer());

        unsafe {
            self.encoding_device
                .vulkan_device
                .device
                .video_queue_ext
                .cmd_end_video_coding_khr(
                    cmd_buffer.buffer(),
                    &vk::VideoEndCodingInfoKHR::default(),
                );
        }

        self.encoding_device
            .h264_encode_queue
            .submit_chain_semaphore(
                cmd_buffer.end()?,
                &mut self.tracker,
                vk::PipelineStageFlags2::VIDEO_ENCODE_KHR,
                vk::PipelineStageFlags2::ALL_COMMANDS,
                EncoderTrackerWaitState::Encode,
            )?;

        self.tracker.wait(u64::MAX)?;

        let feedback = self.query_pool.get_result_blocking()?;

        if feedback.status != vk::QueryResultStatusKHR::COMPLETE {
            return Err(VulkanEncoderError::EncodeOperationFailed(feedback.status));
        }

        let mut output = if is_idr {
            let mut h264_get_info = vk::VideoEncodeH264SessionParametersGetInfoKHR::default()
                .write_std_sps(true)
                .write_std_pps(true)
                .std_sps_id(0)
                .std_pps_id(0);

            let get_info = vk::VideoEncodeSessionParametersGetInfoKHR::default()
                .video_session_parameters(self.session_resources.parameters.parameters)
                .push_next(&mut h264_get_info);

            unsafe {
                self.encoding_device
                    .vulkan_device
                    .device
                    .video_encode_queue_ext
                    .get_encoded_video_session_parameters_khr(&get_info, None)?
            }
        } else {
            Vec::new()
        };

        self.active_reference_slots
            .push_back((setup_reference_slot_idx, std_new_slot_reference_info));

        let encoded = unsafe {
            self.output_buffer
                .download_data_from_buffer(feedback.bytes_written as usize)?
        };

        output.extend_from_slice(&encoded);

        self.idr_period_counter += 1;
        self.idr_period_counter %= self.idr_period;

        Ok(output)
    }

    pub fn encode_bytes(
        &mut self,
        frame: &Frame<RawFrameData>,
        force_idr: bool,
    ) -> Result<EncodedOutputChunk<Vec<u8>>, VulkanEncoderError> {
        let (image, _buffer) = self.transfer_buffer_to_image(frame)?;

        let image = Arc::new(image);

        let is_keyframe = force_idr || self.idr_period_counter == 0;
        let result = self.encode(image, force_idr)?;

        Ok(EncodedOutputChunk {
            data: result,
            pts: frame.pts,
            is_keyframe,
        })
    }

    /// # Safety
    /// - The texture cannot be a surface texture
    /// - The texture has to be transitioned to [`wgpu::TextureUses::RESOURCE`] usage
    pub unsafe fn encode_texture(
        &mut self,
        frame: Frame<wgpu::Texture>,
        force_idr: bool,
    ) -> Result<EncodedOutputChunk<Vec<u8>>, VulkanEncoderError> {
        let convert_state = unsafe {
            self.converter
                .as_ref()
                .unwrap()
                .convert(frame.data, &mut self.tracker)
        }
        .unwrap();

        let is_keyframe = force_idr || self.idr_period_counter == 0;
        let result = self.encode(convert_state.image, force_idr)?;

        Ok(EncodedOutputChunk {
            data: result,
            pts: frame.pts,
            is_keyframe,
        })
    }

    fn encoder_rate_control_for<'a>(
        &self,
        rate_control: RateControl,
        layers: Option<&'a [vk::VideoEncodeRateControlLayerInfoKHR]>,
    ) -> Option<vk::VideoEncodeRateControlInfoKHR<'a>> {
        let layers = layers?;

        match rate_control {
            RateControl::EncoderDefault => None,
            RateControl::Vbr { .. } => Some(
                vk::VideoEncodeRateControlInfoKHR::default()
                    .rate_control_mode(vk::VideoEncodeRateControlModeFlagsKHR::VBR)
                    .layers(layers)
                    .virtual_buffer_size_in_ms(1000)
                    .initial_virtual_buffer_size_in_ms(0),
            ),
            RateControl::Disabled => {
                let mut rate_control = vk::VideoEncodeRateControlInfoKHR::default()
                    .rate_control_mode(vk::VideoEncodeRateControlModeFlagsKHR::DISABLED)
                    .layers(layers);

                rate_control.layer_count = 0;
                Some(rate_control)
            }
        }
    }

    fn h264_rate_control_layers_for(
        &self,
        rate_control: RateControl,
    ) -> Option<Vec<vk::VideoEncodeH264RateControlLayerInfoKHR<'_>>> {
        let layer_info = vk::VideoEncodeH264RateControlLayerInfoKHR::default()
            .use_min_qp(false)
            .use_max_qp(false)
            .use_max_frame_size(false);

        match rate_control {
            RateControl::EncoderDefault => return None,
            RateControl::Vbr { .. } => {}
            RateControl::Disabled => {}
        }

        Some(vec![layer_info])
    }

    fn rate_control_layers_for<'a>(
        &self,
        rate_control: RateControl,
        h264_layer_info: Option<&'a mut [vk::VideoEncodeH264RateControlLayerInfoKHR<'a>]>,
    ) -> Option<Vec<vk::VideoEncodeRateControlLayerInfoKHR<'a>>> {
        let h264_layer_info = h264_layer_info?;
        let mut layer_info = vk::VideoEncodeRateControlLayerInfoKHR::default()
            .frame_rate_numerator(self.session_resources.framerate.numerator)
            .frame_rate_denominator(self.session_resources.framerate.denominator.get());

        match rate_control {
            RateControl::EncoderDefault => return None,
            RateControl::Vbr {
                average_bitrate,
                max_bitrate,
            } => {
                layer_info = layer_info
                    .average_bitrate(average_bitrate)
                    .max_bitrate(max_bitrate)
                    .push_next(&mut h264_layer_info[0])
            }
            RateControl::Disabled => layer_info = layer_info.push_next(&mut h264_layer_info[0]),
        }

        Some(vec![layer_info])
    }

    fn h264_rate_control(
        &self,
        layers: Option<&[vk::VideoEncodeRateControlLayerInfoKHR]>,
    ) -> Option<vk::VideoEncodeH264RateControlInfoKHR<'_>> {
        let layers = layers?;

        Some(
            vk::VideoEncodeH264RateControlInfoKHR::default()
                .temporal_layer_count(layers.len() as u32)
                .flags(
                    vk::VideoEncodeH264RateControlFlagsKHR::REGULAR_GOP
                        | vk::VideoEncodeH264RateControlFlagsKHR::REFERENCE_PATTERN_FLAT,
                )
                .consecutive_b_frame_count(0)
                .gop_frame_count(self.idr_period)
                .idr_period(self.idr_period),
        )
    }
}

/// The rate control algorithm to be used by the encoder.
///
/// Note: `EncoderDefault` is not a good default! For most implementations it is the same as
/// specifying `Disabled`.
///
/// For most use cases, `Vbr` is the correct option
#[derive(Debug, Clone, Copy)]
pub enum RateControl {
    /// Use the default setting of the encoder implementation.
    EncoderDefault,
    /// Variable bitrate rate control. This setting fits most use cases. The encoder will try to
    /// keep the bitrate around the average, but may increase it temporarily up to the max when
    /// necessary. Bitrate is measured in bits/second.
    Vbr {
        average_bitrate: u64,
        max_bitrate: u64,
    },
    /// Rate control is turned off, frames are compressed with a constant rate. A more complicated
    /// frame will just be bigger.
    Disabled,
}

impl RateControl {
    pub(crate) fn to_vk(self) -> vk::VideoEncodeRateControlModeFlagsKHR {
        match self {
            RateControl::EncoderDefault => vk::VideoEncodeRateControlModeFlagsKHR::DEFAULT,
            RateControl::Vbr { .. } => vk::VideoEncodeRateControlModeFlagsKHR::VBR,
            RateControl::Disabled => vk::VideoEncodeRateControlModeFlagsKHR::DISABLED,
        }
    }
}
