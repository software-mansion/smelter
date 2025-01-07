use std::sync::{Arc, Mutex};

use ash::vk;

use crate::{VulkanCtxError, VulkanDevice};

use super::{CommandBuffer, Device, H264DecodeProfileInfo, Image, ImageView, MemoryAllocation, VideoQueueExt};

pub(crate) struct VideoSessionParameters {
    pub(crate) parameters: vk::VideoSessionParametersKHR,
    update_sequence_count: u32,
    device: Arc<Device>,
}

impl VideoSessionParameters {
    pub(crate) fn new(
        device: Arc<Device>,
        session: vk::VideoSessionKHR,
        initial_sps: &[vk::native::StdVideoH264SequenceParameterSet],
        initial_pps: &[vk::native::StdVideoH264PictureParameterSet],
        template: Option<&Self>,
        encode: bool,
    ) -> Result<Self, VulkanCtxError> {
        let parameters_add_info = vk::VideoDecodeH264SessionParametersAddInfoKHR::default()
            .std_sp_ss(initial_sps)
            .std_pp_ss(initial_pps);

        let encode_add_info = vk::VideoEncodeH264SessionParametersAddInfoKHR::default()
            .std_sp_ss(initial_sps)
            .std_pp_ss(initial_pps);

        let mut create_info = vk::VideoSessionParametersCreateInfoKHR::default()
            .flags(vk::VideoSessionParametersCreateFlagsKHR::empty())
            .video_session_parameters_template(
                template
                    .map(|t| t.parameters)
                    .unwrap_or_else(vk::VideoSessionParametersKHR::null),
            )
            .video_session(session);

        let mut h264_decode_info = vk::VideoDecodeH264SessionParametersCreateInfoKHR::default()
                .max_std_sps_count(32)
                .max_std_pps_count(32)
                .parameters_add_info(&parameters_add_info);

        let mut h264_encode_info = vk::VideoEncodeH264SessionParametersCreateInfoKHR::default()
            .max_std_sps_count(32)
            .max_std_pps_count(32)
            .parameters_add_info(&encode_add_info);

        if encode {
            create_info = create_info.push_next(&mut h264_encode_info);
        } else {
            create_info = create_info.push_next(&mut h264_decode_info);
        }

        let parameters = unsafe {
            device
                .video_queue_ext
                .create_video_session_parameters_khr(&create_info, None)?
        };

        Ok(Self {
            parameters,
            update_sequence_count: 0,
            device: device.clone(),
        })
    }

    pub(crate) fn add(
        &mut self,
        sps: &[vk::native::StdVideoH264SequenceParameterSet],
        pps: &[vk::native::StdVideoH264PictureParameterSet],
    ) -> Result<(), VulkanCtxError> {
        let mut parameters_add_info = vk::VideoDecodeH264SessionParametersAddInfoKHR::default()
            .std_sp_ss(sps)
            .std_pp_ss(pps);

        self.update_sequence_count += 1;
        let update_info = vk::VideoSessionParametersUpdateInfoKHR::default()
            .update_sequence_count(self.update_sequence_count)
            .push_next(&mut parameters_add_info);

        unsafe {
            self.device
                .video_queue_ext
                .update_video_session_parameters_khr(self.parameters, &update_info)?
        };

        Ok(())
    }
}

impl Drop for VideoSessionParameters {
    fn drop(&mut self) {
        unsafe {
            self.device
                .video_queue_ext
                .destroy_video_session_parameters_khr(self.parameters, None)
        }
    }
}

pub(crate) struct VideoSession {
    pub(crate) session: vk::VideoSessionKHR,
    pub(crate) device: Arc<Device>,
    pub(crate) _allocations: Vec<MemoryAllocation>,
    pub(crate) max_coded_extent: vk::Extent2D,
    pub(crate) max_dpb_slots: u32,
}

impl VideoSession {
    pub(crate) fn new(
        vulkan_ctx: &VulkanDevice,
        profile_info: &vk::VideoProfileInfoKHR,
        max_coded_extent: vk::Extent2D,
        max_dpb_slots: u32,
        max_active_references: u32,
        flags: vk::VideoSessionCreateFlagsKHR,
        std_header_version: &vk::ExtensionProperties,
    ) -> Result<Self, VulkanCtxError> {
        // TODO: this probably works, but this format needs to be detected and set
        // based on what the GPU supports
        let format = vk::Format::G8_B8R8_2PLANE_420_UNORM;

        let session_create_info = vk::VideoSessionCreateInfoKHR::default()
            .queue_family_index(vulkan_ctx.queues.h264_decode.idx as u32)
            .video_profile(profile_info)
            .picture_format(format)
            .flags(flags)
            .max_coded_extent(max_coded_extent)
            .reference_picture_format(format)
            .max_dpb_slots(max_dpb_slots)
            .max_active_reference_pictures(max_active_references)
            .std_header_version(std_header_version);

        let video_session = unsafe {
            vulkan_ctx
                .device
                .video_queue_ext
                .create_video_session_khr(&session_create_info, None)?
        };

        let memory_requirements = unsafe {
            vulkan_ctx
                .device
                .video_queue_ext
                .get_video_session_memory_requirements_khr(video_session)?
        };

        let allocations = memory_requirements
            .iter()
            .map(|req| {
                MemoryAllocation::new(
                    vulkan_ctx.allocator.clone(),
                    &req.memory_requirements,
                    &vk_mem::AllocationCreateInfo {
                        usage: vk_mem::MemoryUsage::Unknown,
                        ..Default::default()
                    },
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        let memory_bind_infos = memory_requirements
            .into_iter()
            .zip(allocations.iter())
            .map(|(req, allocation)| {
                let allocation_info = allocation.allocation_info();
                vk::BindVideoSessionMemoryInfoKHR::default()
                    .memory_bind_index(req.memory_bind_index)
                    .memory(allocation_info.device_memory)
                    .memory_offset(allocation_info.offset)
                    .memory_size(allocation_info.size)
            })
            .collect::<Vec<_>>();

        unsafe {
            vulkan_ctx
                .device
                .video_queue_ext
                .bind_video_session_memory_khr(video_session, &memory_bind_infos)?
        };

        Ok(VideoSession {
            session: video_session,
            _allocations: allocations,
            device: vulkan_ctx.device.clone(),
            max_coded_extent,
            max_dpb_slots,
        })
    }
}

impl Drop for VideoSession {
    fn drop(&mut self) {
        unsafe {
            self.device
                .video_queue_ext
                .destroy_video_session_khr(self.session, None)
        };
    }
}

impl From<crate::parser::ReferencePictureInfo> for vk::native::StdVideoDecodeH264ReferenceInfo {
    fn from(picture_info: crate::parser::ReferencePictureInfo) -> Self {
        vk::native::StdVideoDecodeH264ReferenceInfo {
            flags: vk::native::StdVideoDecodeH264ReferenceInfoFlags {
                __bindgen_padding_0: [0; 3],
                _bitfield_align_1: [],
                _bitfield_1: vk::native::StdVideoDecodeH264ReferenceInfoFlags::new_bitfield_1(
                    0,
                    0,
                    picture_info.LongTermPicNum.is_some().into(),
                    picture_info.non_existing.into(),
                ),
            },
            FrameNum: picture_info.FrameNum,
            PicOrderCnt: picture_info.PicOrderCnt,
            reserved: 0,
        }
    }
}

impl From<crate::parser::PictureInfo> for vk::native::StdVideoDecodeH264ReferenceInfo {
    fn from(picture_info: crate::parser::PictureInfo) -> Self {
        vk::native::StdVideoDecodeH264ReferenceInfo {
            flags: vk::native::StdVideoDecodeH264ReferenceInfoFlags {
                __bindgen_padding_0: [0; 3],
                _bitfield_align_1: [],
                _bitfield_1: vk::native::StdVideoDecodeH264ReferenceInfoFlags::new_bitfield_1(
                    0,
                    0,
                    picture_info.used_for_long_term_reference.into(),
                    picture_info.non_existing.into(),
                ),
            },
            FrameNum: picture_info.FrameNum,
            PicOrderCnt: picture_info.PicOrderCnt_as_reference_pic,
            reserved: 0,
        }
    }
}

pub(crate) struct CodingImageBundle<'a> {
    pub(crate) image: Arc<Mutex<Image>>,
    pub(crate) _image_view: ImageView,
    pub(crate) video_resource_info: Vec<vk::VideoPictureResourceInfoKHR<'a>>,
}

impl<'a> CodingImageBundle<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        vulkan_ctx: &VulkanDevice,
        command_buffer: &CommandBuffer,
        format: &vk::VideoFormatPropertiesKHR<'a>,
        dimensions: vk::Extent2D,
        image_usage: vk::ImageUsageFlags,
        profile_info: &vk::VideoProfileInfoKHR,
        array_layer_count: u32,
        queue_indices: Option<&[u32]>,
        layout: vk::ImageLayout,
    ) -> Result<Self, VulkanCtxError> {
        let mut profile_list_info = vk::VideoProfileListInfoKHR::default()
            .profiles(std::slice::from_ref(&profile_info));

        let mut image_create_info = vk::ImageCreateInfo::default()
            .flags(format.image_create_flags)
            .image_type(format.image_type)
            .format(format.format)
            .extent(vk::Extent3D {
                width: dimensions.width,
                height: dimensions.height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(array_layer_count)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(format.image_tiling)
            .usage(image_usage)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .push_next(&mut profile_list_info);

        match queue_indices {
            Some(indices) => {
                image_create_info = image_create_info
                    .sharing_mode(vk::SharingMode::CONCURRENT)
                    .queue_family_indices(indices);
            }
            None => {
                image_create_info = image_create_info.sharing_mode(vk::SharingMode::EXCLUSIVE);
            }
        }

        let image = Arc::new(Mutex::new(Image::new(
            vulkan_ctx.allocator.clone(),
            &image_create_info,
        )?));

        let subresource_range = vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: vk::REMAINING_ARRAY_LAYERS,
        };

        let image_view_create_info = vk::ImageViewCreateInfo::default()
            .flags(vk::ImageViewCreateFlags::empty())
            .image(image.lock().unwrap().image)
            .view_type(if array_layer_count == 1 {
                vk::ImageViewType::TYPE_2D
            } else {
                vk::ImageViewType::TYPE_2D_ARRAY
            })
            .format(format.format)
            .components(vk::ComponentMapping::default())
            .subresource_range(subresource_range);

        let image_view = ImageView::new(
            vulkan_ctx.device.clone(),
            image.clone(),
            &image_view_create_info,
        )?;

        let video_resource_info = (0..array_layer_count)
            .map(|i| {
                vk::VideoPictureResourceInfoKHR::default()
                    .coded_offset(vk::Offset2D { x: 0, y: 0 })
                    .coded_extent(dimensions)
                    .base_array_layer(i)
                    .image_view_binding(image_view.view)
            })
            .collect();

        image.lock().unwrap().transition_layout(
            command_buffer,
            vk::PipelineStageFlags2::NONE,
            vk::AccessFlags2::NONE,
            vk::PipelineStageFlags2::NONE,
            vk::AccessFlags2::NONE,
            layout,
            subresource_range,
        )?;

        Ok(Self {
            image,
            _image_view: image_view,
            video_resource_info,
        })
    }

    pub(crate) fn extent(&self) -> vk::Extent3D {
        self.image.lock().unwrap().extent
    }
}

pub(crate) struct DecodedPicturesBuffer<'a> {
    pub(crate) image: CodingImageBundle<'a>,
    // TODO: this should be a bitmap, not a vec stupid
    pub(crate) slot_active: Vec<bool>,
}

impl<'a> DecodedPicturesBuffer<'a> {
    pub(crate) fn new(
        vulkan_ctx: &VulkanDevice,
        command_buffer: &CommandBuffer,
        profile: &vk::VideoProfileInfoKHR,
        image_usage: vk::ImageUsageFlags,
        format: &vk::VideoFormatPropertiesKHR<'a>,
        dimensions: vk::Extent2D,
        max_dpb_slots: u32,
        queue_indices: Option<&'_ [u32]>,
        layout: vk::ImageLayout,
    ) -> Result<Self, VulkanCtxError> {
        let image = CodingImageBundle::new(
            vulkan_ctx,
            command_buffer,
            format,
            dimensions,
            image_usage,
            profile,
            max_dpb_slots,
            queue_indices,
            layout,
        )?;

        Ok(Self {
            image,
            slot_active: vec![false; max_dpb_slots as usize],
        })
    }

    pub(crate) fn reference_slot_info(&self) -> Vec<vk::VideoReferenceSlotInfoKHR> {
        self.image
            .video_resource_info
            .iter()
            .enumerate()
            .map(|(i, info)| {
                vk::VideoReferenceSlotInfoKHR::default()
                    .picture_resource(info)
                    .slot_index(if self.slot_active[i] {
                        i as i32
                    } else {
                        -1
                    })
            })
            .collect()
    }

    pub(crate) fn allocate_reference_picture(&mut self) -> Result<usize, VulkanCtxError> {
        let i = self
            .slot_active
            .iter()
            .enumerate()
            .find(|(_, &v)| !v)
            .map(|(i, _)| i)
            .ok_or(VulkanCtxError::NoFreeSlotsInDpb)?;

        self.slot_active[i] = true;

        Ok(i)
    }

    pub(crate) fn video_resource_info(&self, i: usize) -> Option<&vk::VideoPictureResourceInfoKHR> {
        self.image.video_resource_info.get(i)
    }

    pub(crate) fn free_reference_picture(&mut self, i: usize) {
        self.slot_active[i] = false;
    }

    pub(crate) fn reset_all_allocations(&mut self) {
        self.slot_active
            .iter_mut()
            .for_each(|slot| *slot = false);
    }
}

