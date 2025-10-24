use std::sync::{Arc, Mutex};

use ash::vk;

use crate::{
    VulkanDecoderError,
    device::DecodingDevice,
    vulkan_decoder::Image,
    wrappers::{CodingImageBundle, CommandBuffer, DecodedPicturesBuffer, H264DecodeProfileInfo},
};

pub struct DecodingImages<'a> {
    pub dpb: DecodedPicturesBuffer<'a>,
    pub dst_image: Option<CodingImageBundle<'a>>,
}

impl<'a> DecodingImages<'a> {
    pub fn target_picture_resource_info(
        &'a self,
        new_reference_slot_index: usize,
    ) -> Option<vk::VideoPictureResourceInfoKHR<'a>> {
        match &self.dst_image {
            Some(image) => Some(image.video_resource_info[0]),
            None => self.video_resource_info(new_reference_slot_index).copied(),
        }
    }

    pub fn target_info(
        &self,
        new_reference_slot_index: usize,
    ) -> (Arc<Mutex<Image>>, usize) {
        match &self.dst_image {
            Some(image) => (image.image_with_view.target_info(0), 0),
            None => (
                self.dpb
                    .image
                    .image_with_view
                    .target_info(new_reference_slot_index),
                new_reference_slot_index,
            ),
        }
    }

    pub fn new(
        decoding_device: &DecodingDevice,
        command_buffer: &CommandBuffer,
        profile: &H264DecodeProfileInfo,
        dpb_format: &vk::VideoFormatPropertiesKHR<'a>,
        dst_format: &Option<vk::VideoFormatPropertiesKHR<'a>>,
        dimensions: vk::Extent2D,
        max_dpb_slots: u32,
    ) -> Result<Self, VulkanDecoderError> {
        let dpb_image_usage = if dst_format.is_some() {
            dpb_format.image_usage_flags & vk::ImageUsageFlags::VIDEO_DECODE_DPB_KHR
        } else {
            dpb_format.image_usage_flags
                & (vk::ImageUsageFlags::VIDEO_DECODE_DPB_KHR
                    | vk::ImageUsageFlags::VIDEO_DECODE_DST_KHR
                    | vk::ImageUsageFlags::TRANSFER_SRC)
        };

        let queue_indices = [
            decoding_device.queues.transfer.idx as u32,
            decoding_device.h264_decode_queue.idx as u32,
        ];

        let dpb = DecodedPicturesBuffer::new(
            decoding_device,
            command_buffer,
            false,
            &profile.profile_info,
            dpb_image_usage,
            dpb_format,
            dimensions,
            max_dpb_slots,
            if dst_format.is_some() {
                None
            } else {
                Some(&queue_indices)
            },
            vk::ImageLayout::VIDEO_DECODE_DPB_KHR,
        )?;

        let dst_image = dst_format
            .map(|dst_format| {
                let dst_image_usage = dst_format.image_usage_flags
                    & (vk::ImageUsageFlags::VIDEO_DECODE_DST_KHR
                        | vk::ImageUsageFlags::TRANSFER_SRC);
                CodingImageBundle::new(
                    decoding_device,
                    command_buffer,
                    &dst_format,
                    dimensions,
                    dst_image_usage,
                    false,
                    &profile.profile_info,
                    1,
                    Some(&queue_indices),
                    vk::ImageLayout::VIDEO_DECODE_DST_KHR,
                )
            })
            .transpose()?;

        Ok(Self { dpb, dst_image })
    }

    #[allow(dead_code)]
    pub fn dbp_extent(&self) -> vk::Extent3D {
        self.dpb.image.extent()
    }

    #[allow(dead_code)]
    pub fn dst_extent(&self) -> Option<vk::Extent3D> {
        self.dst_image.as_ref().map(|i| i.extent())
    }

    pub fn reference_slot_info(&self) -> Vec<vk::VideoReferenceSlotInfoKHR<'_>> {
        self.dpb.reference_slot_info()
    }

    pub fn allocate_reference_picture(&mut self) -> Result<usize, VulkanDecoderError> {
        Ok(self.dpb.allocate_reference_picture()?)
    }

    pub fn video_resource_info(
        &self,
        i: usize,
    ) -> Option<&vk::VideoPictureResourceInfoKHR<'_>> {
        self.dpb.video_resource_info(i)
    }

    pub fn free_reference_picture(&mut self, i: usize) {
        self.dpb.free_reference_picture(i);
    }

    pub fn reset_all_allocations(&mut self) {
        self.dpb.reset_all_allocations();
    }
}
