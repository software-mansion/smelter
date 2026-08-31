use std::{
    collections::VecDeque,
    num::NonZeroU32,
    sync::{Arc, Mutex},
};

use ash::vk;
use tracing::warn;

use crate::{
    EncodedOutputChunk, InputFrame, RawFrameData, VideoBackendError,
    backends::vulkan::{
        VulkanCommonError,
        codec::{
            EncodeCodec,
            h264::{H264Codec, encode::H264WriteParametersInfo},
            h265::{H265Codec, encode::H265WriteParametersInfo},
        },
        vulkan_device::EncodingDevice,
        wrappers::{
            Buffer, CommandBufferPool, CommandBufferPoolStorage, DecodedPicturesBuffer,
            EncodeInputImage, EncodeInputImagePool, EncodeOutputBuffer, EncodeOutputBufferPool,
            Image, ImageLayoutTracker, ImageView, ImageWithView, OpenCommandBuffer, QueryPool,
            ResultQuery, ResultQueryData, SemaphoreWaitValue, Tracker, TrackerKind,
            VideoEncodeQueueExt, VideoQueueExt, VideoSession, VideoSessionParameters,
        },
    },
    device::{ColorRange, ColorSpace, Rational},
    encoders::{VideoEncoderError, VideoEncoderParametersInfoH264, VideoEncoderParametersInfoH265},
    parameters::RateControl,
};

pub(crate) mod callback_encoder;

const MB: u64 = 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum VulkanEncoderError {
    #[error("The device does not support Vulkan Video encoding")]
    VulkanEncoderUnsupported,

    #[error("The profile '{0}' is not supported by this device")]
    ProfileUnsupported(String),

    #[error("Invalid encoder parameters, field: {field} - problem: {problem}")]
    ParametersError {
        field: &'static str,
        problem: String,
    },

    #[error(
        "The byte length of the provided frame ({bytes}) is not the same as the picture size calculated from the dimensions ({size_from_resolution})"
    )]
    InconsistentPictureByteSize {
        bytes: usize,
        size_from_resolution: usize,
    },

    #[error(
        "The provided frame dimensions ({provided:?}) do not match the resolution the encoder was configured with ({expected:?})"
    )]
    WrongFrameDimensions {
        provided: (u32, u32),
        expected: (u32, u32),
    },

    #[error("Vulkan error: {0}")]
    VkError(#[from] ash::vk::Result),

    #[error("Cannot find enough memory of the right type on the device")]
    NoMemory,

    #[error(transparent)]
    VulkanCommonError(#[from] VulkanCommonError),

    #[error("This device does not support the required capabilities: {0}")]
    UnsupportedDeviceCapabilities(&'static str),

    #[error("Encode operation failed with status {0:?}")]
    EncodeOperationFailed(vk::QueryResultStatusKHR),

    #[cfg(feature = "wgpu")]
    #[error(transparent)]
    WgpuTextureEncoderError(#[from] crate::encoders::WgpuTextureEncoderError),
}

impl From<VulkanEncoderError> for VideoEncoderError {
    fn from(err: VulkanEncoderError) -> Self {
        match err {
            VulkanEncoderError::VulkanEncoderUnsupported => VideoEncoderError::EncoderUnsupported,
            VulkanEncoderError::ProfileUnsupported(profile) => {
                VideoEncoderError::ProfileUnsupported(profile)
            }
            VulkanEncoderError::ParametersError { field, problem } => {
                VideoEncoderError::ParametersError { field, problem }
            }
            VulkanEncoderError::InconsistentPictureByteSize {
                bytes,
                size_from_resolution,
            } => VideoEncoderError::InconsistentPictureByteSize {
                bytes,
                size_from_resolution,
            },
            VulkanEncoderError::WrongFrameDimensions { provided, expected } => {
                VideoEncoderError::InconsistentPictureDimensions { provided, expected }
            }
            #[cfg(feature = "wgpu")]
            VulkanEncoderError::WgpuTextureEncoderError(err) => {
                VideoEncoderError::WgpuTextureEncoderError(err)
            }
            VulkanEncoderError::VkError(_)
            | VulkanEncoderError::NoMemory
            | VulkanEncoderError::VulkanCommonError(_)
            | VulkanEncoderError::UnsupportedDeviceCapabilities(_)
            | VulkanEncoderError::EncodeOperationFailed(_) => {
                Self::BackendError(VideoBackendError {
                    message: err.to_string(),
                    source: Box::new(err),
                })
            }
        }
    }
}

struct VideoSessionResources<'a> {
    max_dpb_slots: u32,
    video_session: Arc<VideoSession>,
    parameters: Arc<VideoSessionParameters>,
    dpb: DecodedPicturesBuffer<'a>,
    quality_level: u32,
    rate_control: RateControl,
    framerate: Rational,
}

impl VideoSessionResources<'_> {
    fn new<C: EncodeCodec>(
        encoding_device: &EncodingDevice,
        command_buffer: &mut OpenCommandBuffer,
        image_tracker: Arc<Mutex<ImageLayoutTracker>>,
        parameters: &FullEncoderParameters<C>,
        profile_info: &vk::VideoProfileInfoKHR,
    ) -> Result<Self, VulkanEncoderError> {
        let encode_capabilities = C::encode_codec_profile_capabilities(
            &encoding_device.native_encode_capabilities,
            parameters.profile,
        )?;

        let extent = vk::Extent2D {
            width: parameters.width.get(),
            height: parameters.height.get(),
        };

        let max_references = parameters.max_references.get();
        let max_dpb_slots = max_references + 1; // +1 for current picture

        let video_session = VideoSession::new(
            &encoding_device.vulkan_device,
            &encoding_device.encode_queues,
            profile_info,
            extent,
            max_dpb_slots,
            max_references,
            vk::VideoSessionCreateFlagsKHR::ALLOW_ENCODE_PARAMETER_OPTIMIZATIONS,
            &encode_capabilities.video_capabilities.std_header_version,
        )?;

        let use_separate_images = encode_capabilities
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

        let codec_parameters =
            C::codec_parameters(parameters, &encode_capabilities.codec_encode_capabilities)?;

        let session_parameters = VideoSessionParameters::new::<C>(
            encoding_device.vulkan_device.device.clone(),
            video_session.session,
            C::vk_parameters(&codec_parameters),
            None,
            Some(parameters.quality_level),
        )?;

        Ok(Self {
            video_session: Arc::new(video_session),
            dpb,
            parameters: Arc::new(session_parameters),
            max_dpb_slots,
            quality_level: parameters.quality_level,
            rate_control: RateControl::EncoderDefault,
            framerate: parameters.framerate,
        })
    }
}

struct EncodingQueryPool {
    pool: Arc<QueryPool>,
    freelist: Arc<Mutex<Vec<u32>>>,
}

impl EncodingQueryPool {
    pub(crate) fn new<C: EncodeCodec>(
        encoding_device: &EncodingDevice,
        profile: C::Profile,
        profile_info: vk::VideoProfileInfoKHR,
        query_count: u32,
    ) -> Result<Self, VulkanEncoderError> {
        let encode_capabilities = C::encode_codec_profile_capabilities(
            &encoding_device.native_encode_capabilities,
            profile,
        )?;

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
            query_count,
            Some(profile_info),
            Some(
                vk::QueryPoolVideoEncodeFeedbackCreateInfoKHR::default().encode_feedback_flags(
                    vk::VideoEncodeFeedbackFlagsKHR::BITSTREAM_BYTES_WRITTEN
                        | vk::VideoEncodeFeedbackFlagsKHR::BITSTREAM_BUFFER_OFFSET,
                ),
            ),
        )?;

        Ok(Self {
            pool: Arc::new(pool),
            freelist: Arc::new(Mutex::new((0..query_count).collect())),
        })
    }

    fn query(&self) -> ResultQuery<EncodeFeedback> {
        let query_index = self.freelist.lock().unwrap().pop().unwrap();

        ResultQuery::new(
            self.pool.clone(),
            query_index,
            Arc::downgrade(&self.freelist),
        )
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct EncodeFeedback {
    offset: u32,
    bytes_written: u32,
    status: vk::QueryResultStatusKHR,
}

impl ResultQueryData for EncodeFeedback {}

pub(crate) enum EncoderTrackerWaitState {
    InitializeEncoder,
    #[cfg_attr(not(feature = "transcoder"), allow(dead_code))]
    ResizeInput,
    CopyBufferToImage,
    #[cfg_attr(not(feature = "wgpu"), allow(dead_code))]
    CopyImageToImage,
    Encode,
}

#[derive(Clone)]
pub(crate) struct EncoderCommandBufferPools {
    transfer: CommandBufferPool,
    encode: CommandBufferPool,
}

impl EncoderCommandBufferPools {
    fn new(device: &EncodingDevice) -> Result<Self, VulkanEncoderError> {
        let transfer = CommandBufferPool::new(
            device.vulkan_device.clone(),
            device.queues.transfer.family_index,
        )?;
        let encode = CommandBufferPool::new(
            device.vulkan_device.clone(),
            device.encode_queues.family_index,
        )?;

        Ok(Self { transfer, encode })
    }
}

impl CommandBufferPoolStorage for EncoderCommandBufferPools {
    fn mark_submitted_as_free(&self, last_waited_for: SemaphoreWaitValue) {
        self.transfer.mark_submitted_as_free(last_waited_for);
        self.encode.mark_submitted_as_free(last_waited_for);
    }
}

pub(crate) trait DynVulkanEncoder<'a>: Send {
    fn encode(
        &mut self,
        image: Arc<Image>,
        force_idr: bool,
        pts: Option<u64>,
    ) -> Result<UnwaitedEncodeSubmission, VulkanEncoderError>;
    #[cfg_attr(not(feature = "transcoder"), allow(dead_code))]
    fn tracker(&mut self) -> &mut Tracker<EncoderTrackerKind>;
}

pub(crate) struct EncoderTrackerKind {}

impl TrackerKind for EncoderTrackerKind {
    type WaitState = EncoderTrackerWaitState;

    type CommandBufferPools = EncoderCommandBufferPools;
}

pub(crate) type EncoderTracker = Tracker<EncoderTrackerKind>;

pub(crate) struct InFlightEncodeResources {
    _video_session: Arc<VideoSession>,
    _video_session_params: Arc<VideoSessionParameters>,
    _dpb_image_with_view: Arc<ImageWithView>,
    _image: Arc<Image>,
    _view: ImageView,
    _input_buffer: Option<Buffer>,
    input_image: Option<EncodeInputImage>,
    #[cfg(feature = "wgpu")]
    _hal_command_encoder: Option<wgpu::hal::vulkan::CommandEncoder>,
}

impl Drop for InFlightEncodeResources {
    fn drop(&mut self) {
        if let Some(input_image) = self.input_image.take() {
            input_image.release_to_pool();
        }
    }
}

pub(crate) struct EncodeSubmission {
    pub(crate) is_idr: bool,
    pub(crate) wait_value: SemaphoreWaitValue,
    pub(crate) pts: Option<u64>,
    prepended_stream_params: Vec<u8>,
    query: ResultQuery<EncodeFeedback>,
    output_buffer: Option<EncodeOutputBuffer>,
    in_flight_resources: InFlightEncodeResources,
}

impl Drop for EncodeSubmission {
    fn drop(&mut self) {
        if let Some(output_buffer) = self.output_buffer.take() {
            output_buffer.release_to_pool();
        }
    }
}

impl EncodeSubmission {
    pub(crate) fn download(mut self) -> Result<EncodedOutputChunk<Vec<u8>>, VulkanEncoderError> {
        let feedback = self.query.get_result_blocking()?;
        if feedback.status != vk::QueryResultStatusKHR::COMPLETE {
            return Err(VulkanEncoderError::EncodeOperationFailed(feedback.status));
        }

        let mut output = std::mem::take(&mut self.prepended_stream_params);

        let output_buffer = self.output_buffer.as_mut().unwrap();
        let encoded = unsafe {
            output_buffer.buffer.download_data_from_buffer_at(
                feedback.offset as usize,
                feedback.bytes_written as usize,
            )?
        };

        output.extend_from_slice(&encoded);

        Ok(EncodedOutputChunk {
            data: output,
            pts: self.pts,
            is_keyframe: self.is_idr,
        })
    }
}

pub(crate) struct UnwaitedEncodeSubmission(pub(crate) EncodeSubmission);

impl UnwaitedEncodeSubmission {
    #[cfg_attr(not(feature = "transcoder"), allow(dead_code))]
    pub(crate) fn mark_waited(self, tracker: &EncoderTracker) -> WaitedEncodeSubmission {
        tracker.mark_waited(self.0.wait_value);
        WaitedEncodeSubmission(self.0)
    }
}

#[cfg_attr(not(feature = "transcoder"), allow(dead_code))]
pub struct WaitedEncodeSubmission(pub(crate) EncodeSubmission);

impl WaitedEncodeSubmission {
    #[cfg_attr(not(feature = "transcoder"), allow(dead_code))]
    pub(crate) fn download(self) -> Result<EncodedOutputChunk<Vec<u8>>, VulkanEncoderError> {
        self.0.download()
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct FullEncoderParameters<C: EncodeCodec> {
    pub(crate) idr_period: NonZeroU32,
    pub(crate) width: NonZeroU32,
    pub(crate) height: NonZeroU32,
    pub(crate) rate_control: RateControl,
    pub(crate) max_references: NonZeroU32,
    pub(crate) profile: C::Profile,
    pub(crate) quality_level: u32,
    pub(crate) framerate: Rational,
    pub(crate) usage_flags: vk::VideoEncodeUsageFlagsKHR,
    pub(crate) tuning_mode: vk::VideoEncodeTuningModeKHR,
    pub(crate) content_flags: vk::VideoEncodeContentFlagsKHR,
    pub(crate) inline_stream_params: bool,
    pub(crate) color_space: ColorSpace,
    pub(crate) color_range: ColorRange,
    pub(crate) max_in_flight_submissions: NonZeroU32,
}

impl<C: EncodeCodec> From<&FullEncoderParameters<C>> for vk::VideoEncodeUsageInfoKHR<'_> {
    fn from(params: &FullEncoderParameters<C>) -> Self {
        vk::VideoEncodeUsageInfoKHR::default()
            .video_usage_hints(params.usage_flags)
            .tuning_mode(params.tuning_mode)
            .video_content_hints(params.content_flags)
    }
}

pub(crate) struct VulkanEncoder<'a, C: EncodeCodec> {
    pub(crate) tracker: EncoderTracker,
    query_pool: EncodingQueryPool,
    profile: C::Profile,
    session_resources: VideoSessionResources<'a>,
    idr_period_counter: u32,
    idr_period: u32,
    input_image_pool: EncodeInputImagePool<'a>,
    output_buffer_pool: EncodeOutputBufferPool<'a>,
    counters: C::EncodingCounters,
    active_reference_slots: VecDeque<(usize, C::ReferenceInfo)>,
    rate_control: RateControl,
    inline_stream_params: bool,
    encoding_device: Arc<EncodingDevice>,
}

impl<'a> VideoEncoderParametersInfoH264 for VulkanEncoder<'a, H264Codec> {
    fn sps(&self) -> Result<Vec<u8>, VideoEncoderError> {
        self.stream_parameters(H264WriteParametersInfo {
            write_sps: true,
            write_pps: false,
        })
        .map_err(Into::into)
    }

    fn pps(&self) -> Result<Vec<u8>, VideoEncoderError> {
        self.stream_parameters(H264WriteParametersInfo {
            write_sps: false,
            write_pps: true,
        })
        .map_err(Into::into)
    }
}

impl<'a> VideoEncoderParametersInfoH265 for VulkanEncoder<'a, H265Codec> {
    fn vps(&self) -> Result<Vec<u8>, VideoEncoderError> {
        self.stream_parameters(H265WriteParametersInfo {
            write_vps: true,
            write_sps: false,
            write_pps: false,
        })
        .map_err(Into::into)
    }

    fn sps(&self) -> Result<Vec<u8>, VideoEncoderError> {
        self.stream_parameters(H265WriteParametersInfo {
            write_vps: false,
            write_sps: true,
            write_pps: false,
        })
        .map_err(Into::into)
    }

    fn pps(&self) -> Result<Vec<u8>, VideoEncoderError> {
        self.stream_parameters(H265WriteParametersInfo {
            write_vps: false,
            write_sps: false,
            write_pps: true,
        })
        .map_err(Into::into)
    }
}

impl<'a, C: EncodeCodec + 'a> VulkanEncoder<'a, C> {
    const OUTPUT_BUFFER_LEN: u64 = 4 * MB;

    pub(crate) fn new(
        encoding_device: Arc<EncodingDevice>,
        parameters: FullEncoderParameters<C>,
    ) -> Result<Self, VulkanEncoderError> {
        let profile_info = Arc::new(C::profile_info(&parameters));

        let command_buffer_pools = EncoderCommandBufferPools::new(&encoding_device)?;
        let mut tracker = EncoderTracker::new(
            encoding_device.device.clone(),
            command_buffer_pools,
            Some("encoder"),
        )?;

        let query_pool = EncodingQueryPool::new::<C>(
            &encoding_device,
            parameters.profile,
            profile_info.profile_info,
            parameters.max_in_flight_submissions.get(),
        )?;

        // TODO: the buffers in this pool should grow when necessary
        let output_buffer_pool = EncodeOutputBufferPool::new(
            encoding_device.allocator.clone(),
            profile_info.clone(),
            Self::OUTPUT_BUFFER_LEN,
        );

        let mut buffer = tracker.command_buffer_pools.encode.begin_buffer()?;

        let session_resources = VideoSessionResources::new(
            &encoding_device,
            &mut buffer,
            tracker.image_layout_tracker.clone(),
            &parameters,
            &profile_info.profile_info,
        )?;

        encoding_device.encode_queues.submit_chain_semaphore(
            buffer.end()?,
            &mut tracker,
            vk::PipelineStageFlags2::ALL_COMMANDS,
            vk::PipelineStageFlags2::ALL_COMMANDS,
            EncoderTrackerWaitState::InitializeEncoder,
        )?;

        let input_image_queue_families = [
            encoding_device.queues.transfer.family_index as u32,
            encoding_device.queues.wgpu.family_index as u32,
        ];

        let input_image_pool = EncodeInputImagePool::new(
            encoding_device.clone(),
            profile_info,
            session_resources.video_session.max_coded_extent.into(),
            input_image_queue_families.into(),
            tracker.image_layout_tracker.clone(),
        );

        Ok(Self {
            idr_period_counter: 0,
            counters: C::EncodingCounters::default(),
            active_reference_slots: VecDeque::with_capacity(session_resources.dpb.len as usize),
            profile: parameters.profile,
            encoding_device,
            input_image_pool,
            tracker,
            query_pool,
            session_resources,
            idr_period: parameters.idr_period.get(),
            output_buffer_pool,
            rate_control: parameters.rate_control,
            inline_stream_params: parameters.inline_stream_params,
        })
    }

    fn begin_video_coding(&self, buffer: vk::CommandBuffer) {
        let mut codec_layers =
            C::codec_rate_control_layer_info(self.session_resources.rate_control);
        let layers = self.rate_control_layers_for(
            self.session_resources.rate_control,
            codec_layers.as_mut().map(|o| &mut o[..]),
        );
        let mut codec_rate_control =
            C::codec_rate_control_info(layers.as_ref().map(|o| &o[..]), self.idr_period);
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

        if let (Some(encode_rate_control), Some(codec_rate_control)) =
            (encode_rate_control.as_mut(), codec_rate_control.as_mut())
        {
            begin_info = begin_info
                .push_next(encode_rate_control)
                .push_next(codec_rate_control);
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

        let mut codec_layers = C::codec_rate_control_layer_info(rate_control);
        let layers =
            self.rate_control_layers_for(rate_control, codec_layers.as_mut().map(|o| &mut o[..]));
        let mut codec_rate_control =
            C::codec_rate_control_info(layers.as_ref().map(|o| &o[..]), self.idr_period);
        let mut encode_rate_control =
            self.encoder_rate_control_for(rate_control, layers.as_ref().map(|o| &o[..]));

        let flags = vk::VideoCodingControlFlagsKHR::RESET
            | vk::VideoCodingControlFlagsKHR::ENCODE_QUALITY_LEVEL;

        let mut control_info = vk::VideoCodingControlInfoKHR::default()
            .flags(flags)
            .push_next(&mut quality_level);

        if let (Some(encode_rate_control), Some(codec_rate_control)) =
            (encode_rate_control.as_mut(), codec_rate_control.as_mut())
        {
            control_info = control_info
                .flags(control_info.flags | vk::VideoCodingControlFlagsKHR::ENCODE_RATE_CONTROL)
                .push_next(codec_rate_control)
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
        frame: &InputFrame<RawFrameData>,
        input_image: &Arc<Image>,
    ) -> Result<Buffer, VulkanEncoderError> {
        let extent = input_image.extent;

        if frame.data.width != extent.width || frame.data.height != extent.height {
            return Err(VulkanEncoderError::WrongFrameDimensions {
                provided: (frame.data.width, frame.data.height),
                expected: (extent.width, extent.height),
            });
        }

        if frame.data.width as usize * frame.data.height as usize * 3 / 2 != frame.data.frame.len()
        {
            return Err(VulkanEncoderError::InconsistentPictureByteSize {
                bytes: frame.data.frame.len(),
                size_from_resolution: frame.data.width as usize * frame.data.height as usize * 3
                    / 2,
            });
        }

        let mut cmd_buffer = self.tracker.command_buffer_pools.transfer.begin_buffer()?;

        input_image.transition_layout_single_layer(
            &mut cmd_buffer,
            vk::PipelineStageFlags2::ALL_COMMANDS..vk::PipelineStageFlags2::COPY,
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
                    input_image.image,
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
                                width: extent.width,
                                height: extent.height,
                                depth: 1,
                            }),
                        vk::BufferImageCopy::default()
                            .buffer_offset(extent.width as u64 * extent.height as u64)
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
                                width: extent.width / 2,
                                height: extent.height / 2,
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

        Ok(buffer)
    }

    #[cfg(feature = "wgpu")]
    fn copy_wgpu_texture_to_image(
        &mut self,
        wgpu_device: &wgpu::Device,
        wgpu_queue: &wgpu::Queue,
        frame: &InputFrame<wgpu::Texture>,
        input_image: &Arc<Image>,
    ) -> Result<wgpu::hal::vulkan::CommandEncoder, VulkanEncoderError> {
        use crate::encoders::WgpuTextureEncoderError;
        use wgpu::hal::{CommandEncoder, Device, Queue, vulkan::Api as VkApi};

        let encode_texture_extent = wgpu::Extent3d {
            width: input_image.extent.width,
            height: input_image.extent.height,
            depth_or_array_layers: input_image.extent.depth,
        };

        if !frame.data.usage().contains(wgpu::TextureUsages::COPY_SRC) {
            return Err(WgpuTextureEncoderError::NoCopySrcTextureUsage(frame.data.usage()).into());
        }
        if frame.data.format() != wgpu::TextureFormat::NV12 {
            return Err(WgpuTextureEncoderError::NotNV12Texture(frame.data.format()).into());
        }
        if frame.data.size() != encode_texture_extent {
            return Err(WgpuTextureEncoderError::InconsistentPictureDimensions {
                provided_dimensions: frame.data.size(),
                expected_dimensions: encode_texture_extent,
            }
            .into());
        }

        let hal_device = unsafe { wgpu_device.as_hal::<VkApi>().unwrap() };
        let hal_queue = unsafe { wgpu_queue.as_hal::<VkApi>().unwrap() };

        let input_image_clone = input_image.clone();
        let hal_texture = unsafe {
            hal_device.texture_from_raw(
                input_image.image,
                &wgpu::hal::TextureDescriptor {
                    label: None,
                    size: encode_texture_extent,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::NV12,
                    usage: wgpu::TextureUses::COPY_DST,
                    memory_flags: wgpu::hal::MemoryFlags::empty(),
                    view_formats: Vec::new(),
                },
                Some(Box::new(move || {
                    drop(input_image_clone);
                })),
                wgpu::hal::vulkan::TextureMemory::External,
            )
        };

        let texture = unsafe {
            wgpu_device.create_texture_from_hal::<VkApi>(
                hal_texture,
                &wgpu::TextureDescriptor {
                    label: None,
                    size: encode_texture_extent,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::NV12,
                    usage: wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                },
                wgpu::TextureUses::UNINITIALIZED,
            )
        };

        // Copy is on the wgpu core queue because it will handle `frame.data` layout transitions for us
        let mut encoder = wgpu_device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_texture(
            frame.data.as_image_copy(),
            texture.as_image_copy(),
            encode_texture_extent,
        );

        wgpu_queue.submit([encoder.finish()]);

        self.tracker
            .image_layout_tracker
            .lock()
            .unwrap()
            .map
            .insert(
                input_image.key(),
                vec![vk::ImageLayout::TRANSFER_DST_OPTIMAL].into_boxed_slice(),
            );

        // wgpu core queue makes it impossible to specify signal semaphores
        // so we have to make an empty submit on the wgpu hal queue just for the synchronization
        //
        // TODO: it'd be better to create one encoder and just reuse it
        //       because it creates a new command pool every time it's created
        let mut hal_encoder = unsafe {
            hal_device
                .create_command_encoder(&wgpu::hal::CommandEncoderDescriptor {
                    label: Some("vulkan video synchronize with wgpu"),
                    queue: &hal_queue,
                })
                .unwrap()
        };
        let command_buffer = unsafe {
            hal_encoder
                .begin_encoding(None)
                .map_err(WgpuTextureEncoderError::from)?;
            hal_encoder
                .end_encoding()
                .map_err(WgpuTextureEncoderError::from)?
        };

        let mut semaphore_submit_info = self
            .tracker
            .semaphore_tracker
            .next_submit_info(EncoderTrackerWaitState::CopyImageToImage);
        unsafe {
            hal_queue
                .submit(
                    &[&command_buffer],
                    &[],
                    semaphore_submit_info.wgpu_wait_info(),
                )
                .map_err(WgpuTextureEncoderError::from)?;
        }

        semaphore_submit_info.mark_submitted();

        Ok(hal_encoder)
    }

    pub fn stream_parameters(
        &self,
        info: C::CodecWriteParametersInfo,
    ) -> Result<Vec<u8>, VulkanEncoderError> {
        let mut codec_get_info = C::codec_session_parameters_get_info(info);

        let get_info = vk::VideoEncodeSessionParametersGetInfoKHR::default()
            .video_session_parameters(self.session_resources.parameters.parameters)
            .push_next(&mut codec_get_info);

        let data = unsafe {
            self.encoding_device
                .vulkan_device
                .device
                .video_encode_queue_ext
                .get_encoded_video_session_parameters_khr(&get_info, None)?
        };

        Ok(data)
    }

    pub fn encode_bytes(
        &mut self,
        frame: &InputFrame<RawFrameData>,
        force_idr: bool,
    ) -> Result<UnwaitedEncodeSubmission, VulkanEncoderError> {
        let input_image = self.input_image_pool.vk_image()?;
        let buffer = self.transfer_buffer_to_image(frame, &input_image.image)?;

        let mut submission = self.encode(input_image.image.clone(), force_idr, frame.pts)?;
        // TODO: i don't like it
        submission.0.in_flight_resources._input_buffer = Some(buffer);
        submission.0.in_flight_resources.input_image = Some(input_image);

        Ok(submission)
    }

    #[cfg(feature = "wgpu")]
    pub fn encode_texture(
        &mut self,
        wgpu_device: &wgpu::Device,
        wgpu_queue: &wgpu::Queue,
        frame: InputFrame<wgpu::Texture>,
        force_idr: bool,
    ) -> Result<UnwaitedEncodeSubmission, VulkanEncoderError> {
        let input_image = self.input_image_pool.vk_image()?;
        let hal_encoder =
            self.copy_wgpu_texture_to_image(wgpu_device, wgpu_queue, &frame, &input_image.image)?;

        let mut submission = self.encode(input_image.image.clone(), force_idr, frame.pts)?;
        // TODO: i don't like it too
        submission.0.in_flight_resources.input_image = Some(input_image);
        submission.0.in_flight_resources._hal_command_encoder = Some(hal_encoder);

        Ok(submission)
    }

    fn encoder_rate_control_for<'b>(
        &self,
        rate_control: RateControl,
        layers: Option<&'b [vk::VideoEncodeRateControlLayerInfoKHR]>,
    ) -> Option<vk::VideoEncodeRateControlInfoKHR<'b>> {
        let layers = layers?;

        match rate_control {
            RateControl::EncoderDefault => None,

            RateControl::VariableBitrate {
                virtual_buffer_size,
                ..
            } => Some(
                vk::VideoEncodeRateControlInfoKHR::default()
                    .rate_control_mode(vk::VideoEncodeRateControlModeFlagsKHR::VBR)
                    .layers(layers)
                    .virtual_buffer_size_in_ms(virtual_buffer_size.as_millis() as u32)
                    .initial_virtual_buffer_size_in_ms(0),
            ),

            RateControl::ConstantBitrate {
                virtual_buffer_size,
                ..
            } => Some(
                vk::VideoEncodeRateControlInfoKHR::default()
                    .rate_control_mode(vk::VideoEncodeRateControlModeFlagsKHR::CBR)
                    .layers(layers)
                    .virtual_buffer_size_in_ms(virtual_buffer_size.as_millis() as u32)
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

    fn rate_control_layers_for<'b, 'c: 'b>(
        &self,
        rate_control: RateControl,
        codec_layer_info: Option<&'b mut [C::CodecRateControlLayerInfo<'c>]>,
    ) -> Option<Vec<vk::VideoEncodeRateControlLayerInfoKHR<'b>>> {
        let codec_layer_info = codec_layer_info?;
        if let RateControl::EncoderDefault = rate_control {
            return None;
        }

        if codec_layer_info.is_empty() {
            warn!("No layers set for rate control.");
            return None;
        }

        let result = codec_layer_info
            .iter_mut()
            .map(|codec_layer_info| {
                let mut layer_info = vk::VideoEncodeRateControlLayerInfoKHR::default()
                    .frame_rate_numerator(self.session_resources.framerate.numerator)
                    .frame_rate_denominator(self.session_resources.framerate.denominator.get());

                match rate_control {
                    RateControl::EncoderDefault => unreachable!(),
                    RateControl::VariableBitrate {
                        average_bitrate,
                        max_bitrate,
                        ..
                    } => {
                        layer_info = layer_info
                            .average_bitrate(average_bitrate)
                            .max_bitrate(max_bitrate)
                            .push_next(codec_layer_info)
                    }

                    RateControl::ConstantBitrate { bitrate, .. } => {
                        layer_info = layer_info
                            .average_bitrate(bitrate)
                            .max_bitrate(bitrate)
                            .push_next(codec_layer_info)
                    }

                    RateControl::Disabled => layer_info = layer_info.push_next(codec_layer_info),
                }

                layer_info
            })
            .collect();

        Some(result)
    }
}

impl<'a, C: EncodeCodec + 'a> DynVulkanEncoder<'a> for VulkanEncoder<'a, C> {
    fn encode(
        &mut self,
        image: Arc<Image>,
        force_idr: bool,
        pts: Option<u64>,
    ) -> Result<UnwaitedEncodeSubmission, VulkanEncoderError> {
        let query = self.query_pool.query();
        let output_buffer = self.output_buffer_pool.buffer()?;

        let is_idr = force_idr || self.idr_period_counter == 0;

        if is_idr {
            self.idr_period_counter = 0;
            C::counters_idr(&mut self.counters);
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

        query.reset(cmd_buffer.buffer());

        self.begin_video_coding(cmd_buffer.buffer());

        if is_idr {
            self.issue_coding_control_reset_for(cmd_buffer.buffer(), self.rate_control);
        }

        // bugs in nvidia driver I encountered on this journey:
        //
        // bug1: if primary pic type is set to I instead of IDR, the encode command will submit
        // successfully, the fence will trigger, signalling it has been executed, but if you then
        // query the implementation for the status of the operation, it will behave as though the
        // operation never happened (which means it will not return an error!). The division
        // between I and IDR is invented in the vulkan spec, in h264 the values are equivalent.
        //
        // bug2: when rate control is disabled, you have to specify the temporal layer count to 0.
        // You pass a table length and a pointer to a table with temporal layer descriptions. Even
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

        let profile_capabilities = C::encode_codec_profile_capabilities(
            &self.encoding_device.native_encode_capabilities,
            self.profile,
        )?;

        let bitstream_unit_data =
            C::bitstream_unit_data(&profile_capabilities.codec_encode_capabilities, is_idr);
        let bitstream_unit_info = C::bitstream_unit_info(
            &bitstream_unit_data,
            self.rate_control,
            &profile_capabilities.quality_level_properties
                [self.session_resources.quality_level as usize],
            is_idr,
        );

        let bitstream_unit_infos = [bitstream_unit_info];

        let reference_list_info =
            C::reference_list_info(&self.counters, &self.active_reference_slots);

        let picture_info_data = C::picture_info_data(
            &self.counters,
            &profile_capabilities.codec_encode_capabilities,
            is_idr,
            &reference_list_info,
        );

        let mut picture_info = C::picture_info(&picture_info_data, &bitstream_unit_infos);

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
            .map(|(i, info)| (*i, C::dpb_slot_info(info)))
            .collect::<Vec<_>>();

        std_reference_info.iter_mut().for_each(|(i, std_info)| {
            let slot = reference_slots
                .iter_mut()
                .find(|reference_slot| reference_slot.slot_index == (*i) as i32)
                .unwrap();
            *slot = slot.push_next(std_info);
        });

        let new_slot_reference_info = C::new_slot_reference_info(&self.counters, is_idr);
        let mut new_slot_dpb_info = C::new_slot_dpb_slot_info(&new_slot_reference_info);

        let setup_reference_slot_video_resource_info = self
            .session_resources
            .dpb
            .video_resource_info(setup_reference_slot_idx)
            .unwrap();

        let setup_reference_slot = vk::VideoReferenceSlotInfoKHR::default()
            .slot_index(setup_reference_slot_idx as i32)
            .picture_resource(setup_reference_slot_video_resource_info)
            .push_next(&mut new_slot_dpb_info);

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
            .dst_buffer(output_buffer.buffer.buffer)
            .dst_buffer_range(Self::OUTPUT_BUFFER_LEN)
            .dst_buffer_offset(0)
            .src_picture_resource(src_picture_resource)
            .setup_reference_slot(&setup_reference_slot)
            .push_next(&mut picture_info);

        if !reference_slots.is_empty() {
            encode_info = encode_info.reference_slots(&reference_slots);
        }

        query.begin_query(cmd_buffer.buffer());

        unsafe {
            self.encoding_device
                .vulkan_device
                .device
                .video_encode_queue_ext
                .cmd_encode_video_khr(cmd_buffer.buffer(), &encode_info);
        }

        query.end_query(cmd_buffer.buffer());

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

        let wait_value = self.encoding_device.encode_queues.submit_chain_semaphore(
            cmd_buffer.end()?,
            &mut self.tracker,
            vk::PipelineStageFlags2::ALL_COMMANDS,
            vk::PipelineStageFlags2::ALL_COMMANDS,
            EncoderTrackerWaitState::Encode,
        )?;

        C::advance_counters(&mut self.counters, is_idr);
        drop(std_reference_info);

        self.active_reference_slots
            .push_back((setup_reference_slot_idx, new_slot_reference_info));

        self.idr_period_counter += 1;
        self.idr_period_counter %= self.idr_period;

        let prepended_stream_params = if is_idr && self.inline_stream_params {
            self.stream_parameters(C::codec_write_parameters_info_all())?
        } else {
            Vec::new()
        };

        let in_flight_resources = InFlightEncodeResources {
            _video_session: self.session_resources.video_session.clone(),
            _video_session_params: self.session_resources.parameters.clone(),
            _dpb_image_with_view: self.session_resources.dpb.image.image_with_view.clone(),
            _image: image,
            _view: view,
            _input_buffer: None,
            input_image: None,
            #[cfg(feature = "wgpu")]
            _hal_command_encoder: None,
        };

        Ok(UnwaitedEncodeSubmission(EncodeSubmission {
            is_idr,
            wait_value,
            pts,
            prepended_stream_params,
            query,
            output_buffer: Some(output_buffer),
            in_flight_resources,
        }))
    }

    fn tracker(&mut self) -> &mut Tracker<EncoderTrackerKind> {
        &mut self.tracker
    }
}

impl RateControl {
    pub(crate) fn to_vk(self) -> vk::VideoEncodeRateControlModeFlagsKHR {
        match self {
            RateControl::EncoderDefault => vk::VideoEncodeRateControlModeFlagsKHR::DEFAULT,
            RateControl::VariableBitrate { .. } => vk::VideoEncodeRateControlModeFlagsKHR::VBR,
            RateControl::ConstantBitrate { .. } => vk::VideoEncodeRateControlModeFlagsKHR::CBR,
            RateControl::Disabled => vk::VideoEncodeRateControlModeFlagsKHR::DISABLED,
        }
    }
}

impl From<crate::parameters::EncoderUsage> for vk::VideoEncodeUsageFlagsKHR {
    fn from(usage: crate::parameters::EncoderUsage) -> Self {
        use crate::parameters::EncoderUsage;
        match usage {
            EncoderUsage::Default => Self::DEFAULT,
            EncoderUsage::Transcoding => Self::TRANSCODING,
            EncoderUsage::Streaming => Self::STREAMING,
            EncoderUsage::Recording => Self::RECORDING,
            EncoderUsage::Conferencing => Self::CONFERENCING,
        }
    }
}
