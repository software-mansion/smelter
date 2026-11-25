use ash::vk;

use super::ResourceUsage;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Use {
    DecodeSrc,
    DecodeDpb,
    DecodeDpbOutput,
    DecodeDst,
    EncodeSrc,
    EncodeDpb,
    EncodeDst,
    CopySrc,
    CopyDst,
    Storage,
}

impl Use {
    pub(crate) const fn stage(self) -> vk::PipelineStageFlags2 {
        match self {
            Use::DecodeSrc | Use::DecodeDpb | Use::DecodeDpbOutput | Use::DecodeDst => {
                vk::PipelineStageFlags2::VIDEO_DECODE_KHR
            }
            Use::EncodeSrc | Use::EncodeDpb | Use::EncodeDst => {
                vk::PipelineStageFlags2::VIDEO_ENCODE_KHR
            }
            Use::CopySrc | Use::CopyDst => vk::PipelineStageFlags2::COPY,
            Use::Storage => vk::PipelineStageFlags2::COMPUTE_SHADER,
        }
    }

    pub(crate) const fn access(self, write: bool) -> vk::AccessFlags2 {
        match (self, write) {
            (Use::DecodeSrc | Use::DecodeDpb, false) => vk::AccessFlags2::VIDEO_DECODE_READ_KHR,
            (Use::DecodeDpb | Use::DecodeDpbOutput | Use::DecodeDst, true) => {
                vk::AccessFlags2::VIDEO_DECODE_WRITE_KHR
            }
            (Use::EncodeSrc | Use::EncodeDpb, false) => vk::AccessFlags2::VIDEO_ENCODE_READ_KHR,
            (Use::EncodeDpb | Use::EncodeDst, true) => vk::AccessFlags2::VIDEO_ENCODE_WRITE_KHR,
            (Use::CopySrc, false) => vk::AccessFlags2::TRANSFER_READ,
            (Use::CopyDst, true) => vk::AccessFlags2::TRANSFER_WRITE,
            (Use::Storage, false) => vk::AccessFlags2::SHADER_STORAGE_READ,
            (Use::Storage, true) => vk::AccessFlags2::SHADER_STORAGE_WRITE,
            _ => vk::AccessFlags2::NONE,
        }
    }

    pub(crate) const fn layout(self) -> Option<vk::ImageLayout> {
        match self {
            Use::DecodeSrc | Use::EncodeDst => None,
            Use::DecodeDpb | Use::DecodeDpbOutput => Some(vk::ImageLayout::VIDEO_DECODE_DPB_KHR),
            Use::DecodeDst => Some(vk::ImageLayout::VIDEO_DECODE_DST_KHR),
            Use::EncodeSrc => Some(vk::ImageLayout::VIDEO_ENCODE_SRC_KHR),
            Use::EncodeDpb => Some(vk::ImageLayout::VIDEO_ENCODE_DPB_KHR),
            Use::CopySrc => Some(vk::ImageLayout::TRANSFER_SRC_OPTIMAL),
            Use::CopyDst => Some(vk::ImageLayout::TRANSFER_DST_OPTIMAL),
            Use::Storage => Some(vk::ImageLayout::GENERAL),
        }
    }

    pub(crate) const fn required_usage(self, has: ResourceUsage) -> ResourceUsage {
        let image = match has {
            ResourceUsage::Image(_) => true,
            ResourceUsage::Buffer(_) | ResourceUsage::Session => false,
        };
        match self {
            Use::DecodeSrc => ResourceUsage::Buffer(vk::BufferUsageFlags::VIDEO_DECODE_SRC_KHR),
            Use::DecodeDpb => ResourceUsage::Image(vk::ImageUsageFlags::VIDEO_DECODE_DPB_KHR),
            Use::DecodeDpbOutput => ResourceUsage::Image(vk::ImageUsageFlags::from_raw(
                vk::ImageUsageFlags::VIDEO_DECODE_DPB_KHR.as_raw()
                    | vk::ImageUsageFlags::VIDEO_DECODE_DST_KHR.as_raw(),
            )),
            Use::DecodeDst => ResourceUsage::Image(vk::ImageUsageFlags::VIDEO_DECODE_DST_KHR),
            Use::EncodeSrc => ResourceUsage::Image(vk::ImageUsageFlags::VIDEO_ENCODE_SRC_KHR),
            Use::EncodeDpb => ResourceUsage::Image(vk::ImageUsageFlags::VIDEO_ENCODE_DPB_KHR),
            Use::EncodeDst => ResourceUsage::Buffer(vk::BufferUsageFlags::VIDEO_ENCODE_DST_KHR),
            Use::CopySrc if image => ResourceUsage::Image(vk::ImageUsageFlags::TRANSFER_SRC),
            Use::CopySrc => ResourceUsage::Buffer(vk::BufferUsageFlags::TRANSFER_SRC),
            Use::CopyDst if image => ResourceUsage::Image(vk::ImageUsageFlags::TRANSFER_DST),
            Use::CopyDst => ResourceUsage::Buffer(vk::BufferUsageFlags::TRANSFER_DST),
            Use::Storage if image => ResourceUsage::Image(vk::ImageUsageFlags::STORAGE),
            Use::Storage => ResourceUsage::Buffer(vk::BufferUsageFlags::STORAGE_BUFFER),
        }
    }

    pub(crate) const fn required_queue_flags(self) -> vk::QueueFlags {
        match self {
            Use::DecodeSrc | Use::DecodeDpb | Use::DecodeDpbOutput | Use::DecodeDst => {
                vk::QueueFlags::VIDEO_DECODE_KHR
            }
            Use::EncodeSrc | Use::EncodeDpb | Use::EncodeDst => vk::QueueFlags::VIDEO_ENCODE_KHR,
            Use::CopySrc | Use::CopyDst => vk::QueueFlags::from_raw(
                vk::QueueFlags::TRANSFER.as_raw()
                    | vk::QueueFlags::GRAPHICS.as_raw()
                    | vk::QueueFlags::COMPUTE.as_raw(),
            ),
            Use::Storage => vk::QueueFlags::COMPUTE,
        }
    }

    pub(crate) const fn needs_picture(self) -> bool {
        matches!(
            self,
            Use::DecodeDpb
                | Use::DecodeDpbOutput
                | Use::DecodeDst
                | Use::EncodeSrc
                | Use::EncodeDpb
        )
    }

    pub(crate) const fn is_dpb(self) -> bool {
        matches!(self, Use::DecodeDpb | Use::DecodeDpbOutput | Use::EncodeDpb)
    }

    pub(crate) const fn can_read(self) -> bool {
        !matches!(
            self,
            Use::DecodeDpbOutput | Use::DecodeDst | Use::EncodeDst | Use::CopyDst
        )
    }

    pub(crate) const fn can_write(self) -> bool {
        !matches!(self, Use::DecodeSrc | Use::EncodeSrc | Use::CopySrc)
    }
}
