use std::collections::HashMap;

use ash::vk;
use h264_reader::nal::{
    pps::PicParameterSet,
    sps::{Profile, SeqParameterSet},
};
use images::DecodingImages;
use parameters::VideoSessionParametersManager;

use super::{
    h264_level_idc_to_max_dpb_mbs, vk_to_h264_level_idc, wrappers::DecodeInputBuffer,
    CommandBuffer, DecodeQueryPool, Fence, H264ProfileInfo, SeqParameterSetExt, VideoSession,
    VulkanDecoderError, VulkanDevice,
};

mod images;
mod parameters;

pub(super) struct VideoSessionResources<'a> {
    pub(crate) profile_info: H264ProfileInfo<'a>,
    pub(crate) video_session: VideoSession,
    pub(crate) parameters_manager: VideoSessionParametersManager,
    pub(crate) decoding_images: DecodingImages<'a>,
    pub(crate) sps: HashMap<u8, SeqParameterSet>,
    pub(crate) pps: HashMap<(u8, u8), PicParameterSet>,
    pub(crate) decode_query_pool: Option<DecodeQueryPool>,
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
        vulkan_ctx: &VulkanDevice,
        decode_buffer: &CommandBuffer,
        sps: SeqParameterSet,
        fence_memory_barrier_completed: &Fence,
    ) -> Result<Self, VulkanDecoderError> {
        let profile_info = H264ProfileInfo::from_sps_decode(&sps)?;

        let level_idc = sps.level_idc;
        let max_level_idc = vk_to_h264_level_idc(vulkan_ctx.h264_caps.max_level_idc)?;

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
            vulkan_ctx,
            &profile_info.profile_info,
            max_coded_extent,
            max_dpb_slots,
            max_active_references,
            &vulkan_ctx.video_capabilities.std_header_version,
        )?;

        let mut parameters_manager =
            VideoSessionParametersManager::new(vulkan_ctx, video_session.session)?;

        parameters_manager.put_sps(&sps)?;

        let decoding_images = Self::new_decoding_images(
            vulkan_ctx,
            &profile_info,
            max_coded_extent,
            max_dpb_slots,
            decode_buffer,
            fence_memory_barrier_completed,
        )?;

        let sps = HashMap::from_iter([(sps.id().id(), sps)]);
        let decode_query_pool = if vulkan_ctx
            .queues
            .h264_decode
            .supports_result_status_queries()
        {
            Some(DecodeQueryPool::new(
                vulkan_ctx.device.clone(),
                profile_info.profile_info,
            )?)
        } else {
            None
        };

        let decode_buffer = DecodeInputBuffer::new(vulkan_ctx.allocator.clone(), &profile_info)?;

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
        })
    }

    pub(crate) fn process_sps(
        &mut self,
        vulkan_ctx: &VulkanDevice,
        decode_buffer: &CommandBuffer,
        sps: SeqParameterSet,
        fence_memory_barrier_completed: &Fence,
    ) -> Result<(), VulkanDecoderError> {
        let new_profile = H264ProfileInfo::from_sps_decode(&sps)?;

        if self.profile_info != new_profile {
            return Err(VulkanDecoderError::ProfileChangeUnsupported);
        }

        if self.level_idc != sps.level_idc {
            return Err(VulkanDecoderError::LevelChangeUnsupported);
        }

        let max_coded_extent = sps.size()?;
        // +1 for current frame
        let max_dpb_slots = sps.max_num_ref_frames + 1;
        let max_active_references = sps.max_num_ref_frames;

        if self.video_session.max_coded_extent.width >= max_coded_extent.width
            && self.video_session.max_coded_extent.height >= max_coded_extent.height
            && self.video_session.max_dpb_slots >= max_dpb_slots
        {
            // no need to change the session
            self.parameters_manager.put_sps(&sps)?;
            return Ok(());
        }

        self.video_session = VideoSession::new(
            vulkan_ctx,
            &self.profile_info.profile_info,
            max_coded_extent,
            max_dpb_slots,
            max_active_references,
            &vulkan_ctx.video_capabilities.std_header_version,
        )?;

        self.parameters_manager
            .change_session(self.video_session.session)?;
        self.parameters_manager.put_sps(&sps)?;

        self.decoding_images = Self::new_decoding_images(
            vulkan_ctx,
            &self.profile_info,
            max_coded_extent,
            max_dpb_slots,
            decode_buffer,
            fence_memory_barrier_completed,
        )?;

        self.sps.insert(sps.id().id(), sps);

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

    fn new_decoding_images<'a>(
        vulkan_ctx: &VulkanDevice,
        profile: &H264ProfileInfo,
        max_coded_extent: vk::Extent2D,
        max_dpb_slots: u32,
        decode_buffer: &CommandBuffer,
        fence_memory_barrier_completed: &Fence,
    ) -> Result<DecodingImages<'a>, VulkanDecoderError> {
        // FIXME: usually, sps arrives either at the start of the stream (when all spses are sent
        // at the begginning of the stream) or right before an IDR. It is however possible for an
        // sps nal to arrive in between P-frames. This would cause us to loose the reference
        // pictures we need to decode the stream until we receive a new IDR. Don't know if this is
        // an issue worth fixing, I don't think I ever saw a stream like this.
        let (decoding_images, memory_barrier) = DecodingImages::new(
            vulkan_ctx,
            profile,
            &vulkan_ctx.h264_dpb_format_properties,
            &vulkan_ctx.h264_dst_format_properties,
            max_coded_extent,
            max_dpb_slots,
        )?;

        decode_buffer.begin()?;

        unsafe {
            vulkan_ctx.device.cmd_pipeline_barrier2(
                **decode_buffer,
                &vk::DependencyInfo::default().image_memory_barriers(&memory_barrier),
            );
        }

        decode_buffer.end()?;

        vulkan_ctx.queues.h264_decode.submit(
            decode_buffer,
            &[],
            &[],
            Some(**fence_memory_barrier_completed),
        )?;

        // TODO: this shouldn't be a fence
        fence_memory_barrier_completed.wait_and_reset(u64::MAX)?;

        Ok(decoding_images)
    }

    pub(crate) fn free_reference_picture(&mut self, i: usize) -> Result<(), VulkanDecoderError> {
        self.decoding_images.free_reference_picture(i)
    }
}
