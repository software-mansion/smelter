use ash::vk;

use crate::wrappers::ImageKey;

pub(crate) mod vulkan_adapter;
pub(crate) mod vulkan_decoder;
pub(crate) mod vulkan_device;
pub(crate) mod vulkan_encoder;
pub(crate) mod vulkan_instance;

// TODO: move wrappers

#[derive(thiserror::Error, Debug)]
pub enum VulkanCommonError {
    #[error("Vulkan error: {0}")]
    VkError(#[from] vk::Result),

    #[error("Cannot find a queue with index {0}")]
    NoQueue(usize),

    #[error("Memory copy requested to a buffer that is not set up for receiving input")]
    UploadToImproperBuffer,

    #[error("A slot in the Decoded Pictures Buffer was requested, but all slots are taken")]
    NoFreeSlotsInDpb,

    #[error("DPB can have at most 32 slots, {0} was requested")]
    DpbTooLong(u32),

    #[error("Tried to wait for an unsignaled semaphore value")]
    SemaphoreWaitOnUnsignaledValue,

    #[error("Tried to register {0:x?} as a new image, while it already exists")]
    RegisteredNewImageTwice(ImageKey),

    #[error("Tried to access state of image {0:x?}, which does not exist")]
    TriedToAccessNonexistentImageState(ImageKey),

    #[error("Tried to unregister image {0:x?} that was not registered")]
    UnregisteredNonexistentImage(ImageKey),

    #[error("Unsupported image aspect: {0:?}")]
    UnsupportedImageAspect(vk::ImageAspectFlags),

    #[error(
        "The reference image is smaller than the requested extent. Requested: {requested:?}, max allowed: {max_extent:?}"
    )]
    ReferenceImageTooSmall {
        requested: vk::Extent2D,
        max_extent: vk::Extent2D,
    },
}
