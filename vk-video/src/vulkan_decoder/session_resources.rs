use std::collections::HashMap;

use ash::vk;
use h264_reader::nal::{
    pps::PicParameterSet,
    sps::{Profile, SeqParameterSet},
};
use images::DecodingImages;
use parameters::VideoSessionParametersManager;

use crate::{
    wrappers::{
        h264_level_idc_to_max_dpb_mbs, vk_to_h264_level_idc, CommandBuffer, DecodeInputBuffer,
        DecodingQueryPool, Fence, H264DecodeProfileInfo, ProfileInfo, SeqParameterSetExt,
        VideoSession,
    },
    VulkanDecoderError, VulkanDevice,
};

mod images;
mod parameters;

pub(super) struct VideoSessionResources<'a> {
    pub(crate) profile_info: ProfileInfo<'a, vk::VideoDecodeH264ProfileInfoKHR<'a>>,
    pub(crate) video_session: VideoSession,
    pub(crate) parameters_manager: VideoSessionParametersManager,
    pub(crate) decoding_images: DecodingImages<'a>,
    pub(crate) sps: HashMap<u8, SeqParameterSet>,
    pub(crate) pps: HashMap<(u8, u8), PicParameterSet>,
    pub(crate) sps_needing_reset: HashMap<u8, SeqParameterSet>,
    pub(crate) decode_query_pool: Option<DecodingQueryPool>,
    pub(crate) level_idc: u8,
    pub(crate) max_num_reorder_frames: u64,
    pub(crate) decode_buffer: DecodeInputBuffer,
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
    pub(crate) fn new_from_sps(
        vulkan_device: &VulkanDevice,
        decode_buffer: &CommandBuffer,
        sps: SeqParameterSet,
        fence_memory_barrier_completed: &Fence,
    ) -> Result<Self, VulkanDecoderError> {
        let profile_info = ProfileInfo::from_sps_decode(&sps)?;

        let level_idc = sps.level_idc;
        let max_level_idc = vk_to_h264_level_idc(
            vulkan_device
                .decode_capabilities
                .h264_decode_capabilities
                .max_level_idc,
        )?;

        if level_idc > max_level_idc {
            return Err(VulkanDecoderError::InvalidInputData(
                format!("stream has level_idc = {level_idc}, while the GPU can decode at most {max_level_idc}")
            ));
        }

        let max_coded_extent = sps.size()?;
        // +1 for current frame
        let max_dpb_slots = sps.max_num_ref_frames + 1;
        let max_active_references = sps.max_num_ref_frames;
        let max_num_reorder_frames = calculate_max_num_reorder_frames(&sps)?;

        let video_session = VideoSession::new(
            vulkan_device,
            &profile_info.profile_info,
            max_coded_extent,
            max_dpb_slots,
            max_active_references,
            vk::VideoSessionCreateFlagsKHR::empty(),
            &vulkan_device
                .decode_capabilities
                .video_capabilities
                .std_header_version,
        )?;

        let mut parameters_manager =
            VideoSessionParametersManager::new(vulkan_device, video_session.session)?;

        parameters_manager.put_sps(&sps)?;

        let decoding_images = Self::new_decoding_images(
            vulkan_device,
            &profile_info,
            max_coded_extent,
            max_dpb_slots,
            decode_buffer,
            fence_memory_barrier_completed,
        )?;

        let sps = HashMap::from_iter([(sps.id().id(), sps)]);
        let decode_query_pool = if vulkan_device
            .queues
            .h264_decode
            .supports_result_status_queries()
        {
            Some(DecodingQueryPool::new(
                vulkan_device.device.clone(),
                profile_info.profile_info,
            )?)
        } else {
            None
        };

        let decode_buffer = DecodeInputBuffer::new(vulkan_device.allocator.clone(), &profile_info)?;

        Ok(VideoSessionResources {
            profile_info,
            video_session,
            parameters_manager,
            decoding_images,
            sps,
            pps: HashMap::new(),
            decode_query_pool,
            level_idc,
            max_num_reorder_frames,
            decode_buffer,
            sps_needing_reset: HashMap::new(),
        })
    }

    pub(crate) fn process_sps(&mut self, sps: SeqParameterSet) -> Result<(), VulkanDecoderError> {
        let new_profile = ProfileInfo::from_sps_decode(&sps)?;

        let max_coded_extent = sps.size()?;
        // +1 for current frame
        let max_dpb_slots = sps.max_num_ref_frames + 1;

        if self.video_session.max_coded_extent.width >= max_coded_extent.width
            && self.video_session.max_coded_extent.height >= max_coded_extent.height
            && self.video_session.max_dpb_slots >= max_dpb_slots
            && self.profile_info == new_profile
        {
            // no need to change the session
            self.put_sps(sps)?;
            return Ok(());
        }

        self.sps_needing_reset.insert(sps.id().id(), sps);

        Ok(())
    }

    pub(crate) fn process_pps(&mut self, pps: PicParameterSet) -> Result<(), VulkanDecoderError> {
        self.parameters_manager.put_pps(&pps)?;
        self.pps.insert(
            (pps.seq_parameter_set_id.id(), pps.pic_parameter_set_id.id()),
            pps,
        );
        Ok(())
    }

    pub(crate) fn reset_session(
        &mut self,
        vulkan_device: &VulkanDevice,
        decode_buffer: &CommandBuffer,
        sps: SeqParameterSet,
        fence_memory_barrier_completed: &Fence,
    ) -> Result<(), VulkanDecoderError> {
        let new_profile = ProfileInfo::from_sps_decode(&sps)?;

        // +1 for current frame
        let max_dpb_slots = sps.max_num_ref_frames + 1;
        let max_active_references = sps.max_num_ref_frames;
        let max_coded_extent = sps.size()?;

        let level_idc = sps.level_idc;
        let max_level_idc = vk_to_h264_level_idc(
            vulkan_device
                .decode_capabilities
                .h264_decode_capabilities
                .max_level_idc,
        )?;

        if level_idc > max_level_idc {
            return Err(VulkanDecoderError::InvalidInputData(
                format!("stream has level_idc = {level_idc}, while the GPU can decode at most {max_level_idc}")
            ));
        }

        self.level_idc = level_idc;
        self.max_num_reorder_frames = calculate_max_num_reorder_frames(&sps)?;

        if self.profile_info != new_profile {
            self.profile_info = new_profile;

            self.decode_query_pool = match vulkan_device
                .queues
                .h264_decode
                .supports_result_status_queries()
            {
                true => Some(DecodingQueryPool::new(
                    vulkan_device.device.clone(),
                    self.profile_info.profile_info,
                )?),
                false => None,
            };
            self.decode_buffer =
                DecodeInputBuffer::new(vulkan_device.allocator.clone(), &self.profile_info)?;
        }

        self.video_session = VideoSession::new(
            vulkan_device,
            &self.profile_info.profile_info,
            max_coded_extent,
            max_dpb_slots,
            max_active_references,
            vk::VideoSessionCreateFlagsKHR::empty(),
            &vulkan_device
                .decode_capabilities
                .video_capabilities
                .std_header_version,
        )?;

        self.parameters_manager
            .change_session(self.video_session.session)?;

        self.decoding_images = Self::new_decoding_images(
            vulkan_device,
            &self.profile_info,
            self.video_session.max_coded_extent,
            self.video_session.max_dpb_slots,
            decode_buffer,
            fence_memory_barrier_completed,
        )?;

        self.put_sps(sps)?;

        Ok(())
    }

    fn put_sps(&mut self, sps: SeqParameterSet) -> Result<(), VulkanDecoderError> {
        self.parameters_manager.put_sps(&sps)?;
        self.sps.insert(sps.id().id(), sps);
        Ok(())
    }

    /// Creates a new buffer of reference images used for decoding.
    ///
    /// If you're replacing existing decoding images, make sure the old references won't be used,
    /// e.g. do it right before decoding IDR. Otherwise it may result in error because the decoder
    /// could try to use references which no longer exist if there's a non IDR frame after SPS.
    fn new_decoding_images<'a>(
        vulkan_device: &VulkanDevice,
        profile: &H264DecodeProfileInfo,
        max_coded_extent: vk::Extent2D,
        max_dpb_slots: u32,
        decode_buffer: &CommandBuffer,
        fence_memory_barrier_completed: &Fence,
    ) -> Result<DecodingImages<'a>, VulkanDecoderError> {
        decode_buffer.begin()?;

        let decoding_images = DecodingImages::new(
            vulkan_device,
            decode_buffer,
            profile,
            &vulkan_device.decode_capabilities.h264_dpb_format_properties,
            &vulkan_device.decode_capabilities.h264_dst_format_properties,
            max_coded_extent,
            max_dpb_slots,
        )?;

        decode_buffer.end()?;

        vulkan_device.queues.h264_decode.submit(
            decode_buffer,
            &[],
            &[],
            Some(**fence_memory_barrier_completed),
        )?;

        // TODO: this shouldn't be a fence
        fence_memory_barrier_completed.wait_and_reset(u64::MAX)?;

        Ok(decoding_images)
    }

    pub(crate) fn free_reference_picture(&mut self, i: usize) {
        self.decoding_images.free_reference_picture(i);
    }
}
