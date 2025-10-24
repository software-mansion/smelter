use std::{collections::HashMap, sync::Arc};

use ash::vk;
use h264_reader::nal::{
    pps::PicParameterSet,
    sps::{Profile, SeqParameterSet},
};
use images::DecodingImages;
use parameters::{SessionParams, VideoSessionParametersManager};

use crate::{
    VulkanDecoderError,
    device::DecodingDevice,
    vulkan_decoder::{DecoderTracker, DecoderTrackerWaitState},
    wrappers::{
        CommandBuffer, DecodeInputBuffer, DecodingQueryPool, H264DecodeProfileInfo, ProfileInfo,
        SeqParameterSetExt, VideoSession, h264_level_idc_to_max_dpb_mbs, vk_to_h264_level_idc,
    },
};

mod images;
mod parameters;

pub(super) struct VideoSessionResources<'a> {
    pub video_session: VideoSession,
    pub parameters: SessionParams<'a>,
    pub parameters_manager: VideoSessionParametersManager,
    pub decoding_images: DecodingImages<'a>,
    pub sps: HashMap<u8, SeqParameterSet>,
    pub pps: HashMap<(u8, u8), PicParameterSet>,
    pub decode_query_pool: Option<DecodingQueryPool>,
    pub decode_buffer: DecodeInputBuffer,
    parameters_scheduled_for_reset: Option<SessionParams<'a>>,
}

fn calculate_max_num_reorder_frames(sps: &SeqParameterSet) -> Result<u64, VulkanDecoderError> {
    let fallback_max_num_reorder_frames = if [44u8, 86, 100, 110, 122, 244]
        .contains(&sps.profile_idc.into())
        && sps.constraint_flags.flag3()
    {
        0
    } else if let Profile::Baseline = sps.profile() {
        0
    } else {
        h264_level_idc_to_max_dpb_mbs(sps.level_idc)?
            / ((sps.pic_width_in_mbs_minus1 as u64 + 1)
                * (sps.pic_height_in_map_units_minus1 as u64 + 1))
                .min(16)
    };

    let max_num_reorder_frames = sps
        .vui_parameters
        .as_ref()
        .and_then(|v| v.bitstream_restrictions.as_ref())
        .map(|b| b.max_num_reorder_frames as u64)
        .unwrap_or(fallback_max_num_reorder_frames);

    Ok(max_num_reorder_frames)
}

impl VideoSessionResources<'_> {
    pub fn new_from_sps(
        decoding_device: &DecodingDevice,
        decode_buffer: &CommandBuffer,
        sps: SeqParameterSet,
        tracker: &mut DecoderTracker,
    ) -> Result<Self, VulkanDecoderError> {
        let profile_info = Arc::new(ProfileInfo::from_sps_decode(&sps)?);

        let level_idc = sps.level_idc;
        let max_level_idc = vk_to_h264_level_idc(
            decoding_device
                .profile_capabilities
                .h264_decode_capabilities
                .max_level_idc,
        )?;

        if level_idc > max_level_idc {
            return Err(VulkanDecoderError::InvalidInputData(format!(
                "stream has level_idc = {level_idc}, while the GPU can decode at most {max_level_idc}"
            )));
        }

        let max_coded_extent = sps.size()?;
        // +1 for current frame
        let max_dpb_slots = sps.max_num_ref_frames + 1;
        let max_active_references = sps.max_num_ref_frames;
        let max_num_reorder_frames = calculate_max_num_reorder_frames(&sps)?;

        let video_session = VideoSession::new(
            &decoding_device.vulkan_device,
            &decoding_device.h264_decode_queue,
            &profile_info.profile_info,
            max_coded_extent,
            max_dpb_slots,
            max_active_references,
            vk::VideoSessionCreateFlagsKHR::empty(),
            &decoding_device
                .profile_capabilities
                .video_capabilities
                .std_header_version,
        )?;

        let mut parameters_manager =
            VideoSessionParametersManager::new(decoding_device, video_session.session)?;

        parameters_manager.put_sps(&sps)?;

        let decoding_images = Self::new_decoding_images(
            decoding_device,
            &profile_info,
            max_coded_extent,
            max_dpb_slots,
            decode_buffer,
            tracker,
        )?;

        let sps = HashMap::from_iter([(sps.id().id(), sps)]);
        let decode_query_pool = if decoding_device
            .h264_decode_queue
            .supports_result_status_queries()
        {
            Some(DecodingQueryPool::new(
                decoding_device.vulkan_device.device.clone(),
                profile_info.profile_info,
            )?)
        } else {
            None
        };

        let decode_buffer =
            DecodeInputBuffer::new(decoding_device.allocator.clone(), &profile_info)?;

        let parameters = SessionParams {
            max_coded_extent,
            max_dpb_slots,
            max_active_references,
            max_num_reorder_frames,
            profile_info,
            level_idc,
        };

        Ok(VideoSessionResources {
            parameters,
            video_session,
            parameters_manager,
            decoding_images,
            sps,
            pps: HashMap::new(),
            decode_query_pool,
            decode_buffer,
            parameters_scheduled_for_reset: None,
        })
    }

    pub fn process_sps(&mut self, sps: SeqParameterSet) -> Result<(), VulkanDecoderError> {
        let new_session_params = SessionParams {
            max_coded_extent: sps.size()?,
            max_dpb_slots: sps.max_num_ref_frames + 1, // +1 for current frame
            max_active_references: sps.max_num_ref_frames,
            max_num_reorder_frames: calculate_max_num_reorder_frames(&sps)?,
            profile_info: Arc::new(ProfileInfo::from_sps_decode(&sps)?),
            level_idc: sps.level_idc,
        };
        let current_session_params = self
            .parameters_scheduled_for_reset
            .take()
            .unwrap_or_else(|| self.parameters.clone());

        self.parameters_scheduled_for_reset = Some(SessionParams::combine(
            current_session_params,
            new_session_params,
        ));

        self.parameters_manager.put_sps(&sps)?;
        self.sps.insert(sps.id().id(), sps);

        Ok(())
    }

    pub fn process_pps(&mut self, pps: PicParameterSet) -> Result<(), VulkanDecoderError> {
        self.parameters_manager.put_pps(&pps)?;
        self.pps.insert(
            (pps.seq_parameter_set_id.id(), pps.pic_parameter_set_id.id()),
            pps,
        );
        Ok(())
    }

    pub fn ensure_session(
        &mut self,
        decoding_device: &DecodingDevice,
        decode_buffer: &CommandBuffer,
        tracker: &mut DecoderTracker,
    ) -> Result<(), VulkanDecoderError> {
        let Some(new_params) = self.parameters_scheduled_for_reset.take() else {
            return Ok(());
        };

        if self.parameters.is_valid(&new_params) {
            // no need to change the session
            self.parameters.max_num_reorder_frames = new_params.max_num_reorder_frames;
            return Ok(());
        }

        let max_level_idc = vk_to_h264_level_idc(
            decoding_device
                .profile_capabilities
                .h264_decode_capabilities
                .max_level_idc,
        )?;

        if new_params.level_idc > max_level_idc {
            return Err(VulkanDecoderError::InvalidInputData(format!(
                "stream has level_idc = {}, while the GPU can decode at most {}",
                new_params.level_idc, max_level_idc
            )));
        }

        if self.parameters.profile_info != new_params.profile_info {
            self.decode_query_pool = match decoding_device
                .h264_decode_queue
                .supports_result_status_queries()
            {
                true => Some(DecodingQueryPool::new(
                    decoding_device.vulkan_device.device.clone(),
                    new_params.profile_info.profile_info,
                )?),
                false => None,
            };
            self.decode_buffer = DecodeInputBuffer::new(
                decoding_device.allocator.clone(),
                &new_params.profile_info,
            )?;
        }

        self.video_session = VideoSession::new(
            &decoding_device.vulkan_device,
            &decoding_device.h264_decode_queue,
            &new_params.profile_info.profile_info,
            new_params.max_coded_extent,
            new_params.max_dpb_slots,
            new_params.max_active_references,
            vk::VideoSessionCreateFlagsKHR::empty(),
            &decoding_device
                .profile_capabilities
                .video_capabilities
                .std_header_version,
        )?;

        self.parameters_manager
            .change_session(self.video_session.session)?;

        self.decoding_images = Self::new_decoding_images(
            decoding_device,
            &new_params.profile_info,
            self.video_session.max_coded_extent,
            self.video_session.max_dpb_slots,
            decode_buffer,
            tracker,
        )?;

        self.parameters = new_params;

        Ok(())
    }

    /// Creates a new buffer of reference images used for decoding.
    ///
    /// If you're replacing existing decoding images, make sure the old references won't be used,
    /// e.g. do it right before decoding IDR. Otherwise it may result in error because the decoder
    /// could try to use references which no longer exist if there's a non IDR frame after SPS.
    fn new_decoding_images<'a>(
        decoding_device: &DecodingDevice,
        profile: &H264DecodeProfileInfo,
        max_coded_extent: vk::Extent2D,
        max_dpb_slots: u32,
        decode_buffer: &CommandBuffer,
        tracker: &mut DecoderTracker,
    ) -> Result<DecodingImages<'a>, VulkanDecoderError> {
        decode_buffer.begin()?;

        let decoding_images = DecodingImages::new(
            decoding_device,
            decode_buffer,
            profile,
            &decoding_device
                .profile_capabilities
                .h264_dpb_format_properties,
            &decoding_device
                .profile_capabilities
                .h264_dst_format_properties,
            max_coded_extent,
            max_dpb_slots,
        )?;

        decode_buffer.end()?;

        decoding_device.h264_decode_queue.submit_chain_semaphore(
            decode_buffer,
            tracker,
            vk::PipelineStageFlags2::ALL_COMMANDS,
            vk::PipelineStageFlags2::ALL_COMMANDS,
            DecoderTrackerWaitState::NewDecodingImagesLayoutTransition,
        )?;

        Ok(decoding_images)
    }

    pub fn free_reference_picture(&mut self, i: usize) {
        self.decoding_images.free_reference_picture(i);
    }
}
