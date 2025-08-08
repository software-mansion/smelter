use std::sync::Arc;

use ash::vk;

use crate::{
    vulkan_decoder::{H264ProfileInfo, Image, ImageView},
    VulkanDecoderError, VulkanDevice,
};

pub(crate) struct DecodingImages<'a> {
    pub(crate) dpb_image: DecodingImageBundle<'a>,
    pub(crate) dpb_slot_active: Vec<bool>,
    pub(crate) dst_image: Option<DecodingImageBundle<'a>>,
}

pub(crate) struct DecodingImageBundle<'a> {
    pub(crate) image: Arc<Image>,
    pub(crate) _image_view: ImageView,
    pub(crate) video_resource_info: Vec<vk::VideoPictureResourceInfoKHR<'a>>,
}

impl<'a> DecodingImageBundle<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        vulkan_ctx: &VulkanDevice,
        format: &vk::VideoFormatPropertiesKHR<'a>,
        dimensions: vk::Extent2D,
        image_usage: vk::ImageUsageFlags,
        profile_info: &H264ProfileInfo,
        array_layer_count: u32,
        queue_indices: Option<&[u32]>,
        layout: vk::ImageLayout,
    ) -> Result<(Self, vk::ImageMemoryBarrier2<'a>), VulkanDecoderError> {
        let mut profile_list_info = vk::VideoProfileListInfoKHR::default()
            .profiles(std::slice::from_ref(&profile_info.profile_info));

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

        let image = Arc::new(Image::new(
            vulkan_ctx.allocator.clone(),
            &image_create_info,
        )?);

        let subresource_range = vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: vk::REMAINING_ARRAY_LAYERS,
        };

        let image_view_create_info = vk::ImageViewCreateInfo::default()
            .flags(vk::ImageViewCreateFlags::empty())
            .image(**image)
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

        let image_memory_barrier = vk::ImageMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::NONE)
            .src_access_mask(vk::AccessFlags2::NONE)
            .dst_stage_mask(vk::PipelineStageFlags2::NONE)
            .dst_access_mask(vk::AccessFlags2::NONE)
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(layout)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(**image)
            .subresource_range(subresource_range);

        Ok((
            Self {
                image,
                _image_view: image_view,
                video_resource_info,
            },
            image_memory_barrier,
        ))
    }

    fn extent(&self) -> vk::Extent3D {
        self.image.extent
    }
}

impl<'a> DecodingImages<'a> {
    pub(crate) fn target_picture_resource_info(
        &'a self,
        new_reference_slot_index: usize,
    ) -> Option<vk::VideoPictureResourceInfoKHR<'a>> {
        match &self.dst_image {
            Some(image) => Some(image.video_resource_info[0]),
            None => self.video_resource_info(new_reference_slot_index).copied(),
        }
    }

    pub(crate) fn target_info(
        &self,
        new_reference_slot_index: usize,
    ) -> (vk::Image, vk::ImageLayout, usize) {
        match &self.dst_image {
            Some(image) => (**image.image, vk::ImageLayout::VIDEO_DECODE_DST_KHR, 0),
            None => (
                **self.dpb_image.image,
                vk::ImageLayout::VIDEO_DECODE_DPB_KHR,
                new_reference_slot_index,
            ),
        }
    }

    pub(crate) fn new(
        vulkan_ctx: &VulkanDevice,
        profile: &H264ProfileInfo,
        dpb_format: &vk::VideoFormatPropertiesKHR<'a>,
        dst_format: &Option<vk::VideoFormatPropertiesKHR<'a>>,
        dimensions: vk::Extent2D,
        max_dpb_slots: u32,
    ) -> Result<(Self, Vec<vk::ImageMemoryBarrier2<'a>>), VulkanDecoderError> {
        let dpb_image_usage = if dst_format.is_some() {
            dpb_format.image_usage_flags & vk::ImageUsageFlags::VIDEO_DECODE_DPB_KHR
        } else {
            dpb_format.image_usage_flags
                & (vk::ImageUsageFlags::VIDEO_DECODE_DPB_KHR
                    | vk::ImageUsageFlags::VIDEO_DECODE_DST_KHR
                    | vk::ImageUsageFlags::TRANSFER_SRC)
        };

        let queue_indices = [
            vulkan_ctx.queues.transfer.idx as u32,
            vulkan_ctx.queues.h264_decode.idx as u32,
        ];

        let (dpb_image, dpb_memory_barrier) = DecodingImageBundle::new(
            vulkan_ctx,
            dpb_format,
            dimensions,
            dpb_image_usage,
            profile,
            max_dpb_slots,
            if dst_format.is_some() {
                None
            } else {
                Some(&queue_indices)
            },
            vk::ImageLayout::VIDEO_DECODE_DPB_KHR,
        )?;

        let output = dst_format
            .map(|dst_format| {
                let dst_image_usage = dst_format.image_usage_flags
                    & (vk::ImageUsageFlags::VIDEO_DECODE_DST_KHR
                        | vk::ImageUsageFlags::TRANSFER_SRC);
                DecodingImageBundle::new(
                    vulkan_ctx,
                    &dst_format,
                    dimensions,
                    dst_image_usage,
                    profile,
                    1,
                    Some(&queue_indices),
                    vk::ImageLayout::VIDEO_DECODE_DST_KHR,
                )
            })
            .transpose()?;

        let (dst_image, dst_memory_barrier) = match output {
            Some((output_images, output_memory_barrier)) => {
                (Some(output_images), Some(output_memory_barrier))
            }
            None => (None, None),
        };

        let barriers = [dpb_memory_barrier]
            .into_iter()
            .chain(dst_memory_barrier)
            .collect::<Vec<_>>();

        Ok((
            Self {
                dpb_image,
                dpb_slot_active: vec![false; max_dpb_slots as usize],
                dst_image,
            },
            barriers,
        ))
    }

    #[allow(dead_code)]
    pub(crate) fn dbp_extent(&self) -> vk::Extent3D {
        self.dpb_image.extent()
    }

    #[allow(dead_code)]
    pub(crate) fn dst_extent(&self) -> Option<vk::Extent3D> {
        self.dst_image.as_ref().map(|i| i.extent())
    }

    pub(crate) fn reference_slot_info(&self) -> Vec<vk::VideoReferenceSlotInfoKHR<'_>> {
        self.dpb_image
            .video_resource_info
            .iter()
            .enumerate()
            .map(|(i, info)| {
                vk::VideoReferenceSlotInfoKHR::default()
                    .picture_resource(info)
                    .slot_index(if self.dpb_slot_active[i] {
                        i as i32
                    } else {
                        -1
                    })
            })
            .collect()
    }

    pub(crate) fn allocate_reference_picture(&mut self) -> Result<usize, VulkanDecoderError> {
        let i = self
            .dpb_slot_active
            .iter()
            .enumerate()
            .find(|(_, &v)| !v)
            .map(|(i, _)| i)
            .ok_or(VulkanDecoderError::NoFreeSlotsInDpb)?;

        self.dpb_slot_active[i] = true;

        Ok(i)
    }

    pub(crate) fn video_resource_info(
        &self,
        i: usize,
    ) -> Option<&vk::VideoPictureResourceInfoKHR<'_>> {
        self.dpb_image.video_resource_info.get(i)
    }

    pub(crate) fn free_reference_picture(&mut self, i: usize) -> Result<(), VulkanDecoderError> {
        self.dpb_slot_active[i] = false;

        Ok(())
    }

    pub(crate) fn reset_all_allocations(&mut self) {
        self.dpb_slot_active
            .iter_mut()
            .for_each(|slot| *slot = false);
    }
}
