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
        todo!()
    }

    pub(crate) const fn access(self, write: bool) -> vk::AccessFlags2 {
        todo!()
    }

    pub(crate) const fn layout(self) -> Option<vk::ImageLayout> {
        todo!()
    }

    pub(crate) const fn required_usage(self) -> ResourceUsage {
        todo!()
    }

    pub(crate) const fn required_queue_flags(self) -> vk::QueueFlags {
        todo!()
    }

    pub(crate) const fn is_dpb(self) -> bool {
        todo!()
    }

    pub(crate) const fn can_read(self) -> bool {
        todo!()
    }

    pub(crate) const fn can_write(self) -> bool {
        todo!()
    }
}
