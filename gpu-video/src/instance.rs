use std::sync::Arc;

use crate::{
    VideoInitError,
    adapter::{VideoAdapter, VideoAdapterDescriptor},
    vulkan::vulkan_instance::VulkanInstance,
};

/// Describes a [`VideoInstance`].
/// Used by [`VideoInstance::new`]
#[derive(Debug, Clone, Default)]
pub struct VideoInstanceDescriptor {
    /// On vulkan, it enables Vulkan Validation Layers (VVL).
    pub enable_validations: bool,
    /// On vulkan, it prints all API calls to vulkan during runtime.
    pub enable_api_dump: bool,
}

// TODO: What error type should VideoInstance (it can't contain vulkan/metal specific things?)
// TODO: orginize things
pub trait VideoInstanceBackend {
    fn create_adapter<'a>(
        &'a self,
        desc: &VideoAdapterDescriptor,
    ) -> Result<VideoAdapter<'a>, VideoInitError>;

    fn iter_adapters<'a>(
        &'a self,
    ) -> Result<Box<dyn Iterator<Item = VideoAdapter<'a>> + 'a>, VideoInitError>;
}

/// Context for all encoders and decoders.
#[derive(Clone)]
pub struct VideoInstance {
    instance: Arc<dyn VideoInstanceBackend>,
}

impl VideoInstance {
    pub fn new(desc: &VideoInstanceDescriptor) -> Result<Self, VideoInitError> {
        #[cfg(vulkan)]
        let instance = VulkanInstance::new(desc)?;

        Ok(Self {
            instance: Arc::new(instance),
        })
    }

    /// Creates an adapter that meets requirements specified in the descriptor.
    pub fn create_adapter<'a>(
        &'a self,
        descriptor: &VideoAdapterDescriptor,
    ) -> Result<VideoAdapter<'a>, VideoInitError> {
        self.iter_adapters()?
            .find(|adapter| {
                (!descriptor.supports_decoding || adapter.supports_decoding())
                    && (!descriptor.supports_encoding || adapter.supports_encoding())
            })
            .ok_or(VideoInitError::NoDevice)
    }

    /// Iterator over all available [`VideoAdapter`]s that support at least decoding or encoding.
    pub fn iter_adapters<'a>(
        &'a self,
    ) -> Result<impl Iterator<Item = VideoAdapter<'a>>, VideoInitError> {
        self.instance.iter_adapters()
    }
}

impl std::fmt::Debug for VideoInstance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VulkanInstance").finish()
    }
}
