use std::sync::Arc;

use ash::{Entry, vk};

use crate::{
    VulkanInitError,
    adapter::{VideoAdapter, VideoAdapterDescriptor},
    wrappers::*,
};

/// Context for all encoders and decoders. Also contains a [`wgpu::Instance`].
pub struct VideoInstance {
    _entry: Arc<Entry>,
    pub(crate) instance: Arc<Instance>,
    _debug_messenger: Option<DebugMessenger>,
}

impl VideoInstance {
    pub fn new() -> Result<Arc<Self>, VulkanInitError> {
        let entry = Arc::new(unsafe { Entry::load()? });
        Self::new_from_entry(entry)
    }

    pub fn new_from(
        vulkan_library_path: impl AsRef<std::ffi::OsStr>,
    ) -> Result<Arc<Self>, VulkanInitError> {
        let entry = Arc::new(unsafe { Entry::load_from(vulkan_library_path)? });
        Self::new_from_entry(entry)
    }

    fn new_from_entry(entry: Arc<Entry>) -> Result<Arc<Self>, VulkanInitError> {
        let api_version = vk::make_api_version(0, 1, 3, 0);
        let app_info = vk::ApplicationInfo {
            api_version,
            ..Default::default()
        };

        let mut requested_layers = Vec::new();

        if cfg!(feature = "vk-validation") {
            requested_layers.push(c"VK_LAYER_KHRONOS_validation");
        }

        if cfg!(feature = "vk-api-dump") {
            requested_layers.push(c"VK_LAYER_LUNARG_api_dump");
        }

        let instance_layer_properties = unsafe { entry.enumerate_instance_layer_properties()? };
        let instance_layer_names = instance_layer_properties
            .iter()
            .map(|layer| layer.layer_name_as_c_str())
            .collect::<Result<Vec<_>, _>>()?;

        let layers = requested_layers
            .into_iter()
            .filter(|requested_layer_name| {
                instance_layer_names
                    .iter()
                    .any(|instance_layer_name| instance_layer_name == requested_layer_name)
            })
            .map(|layer| layer.as_ptr())
            .collect::<Vec<_>>();

        let extensions = [vk::EXT_DEBUG_UTILS_NAME];
        let extension_ptrs = extensions.iter().map(|e| e.as_ptr()).collect::<Vec<_>>();

        let create_info = vk::InstanceCreateInfo::default()
            .application_info(&app_info)
            .enabled_layer_names(&layers)
            .enabled_extension_names(&extension_ptrs);

        let instance = unsafe { entry.create_instance(&create_info, None) }?;
        let instance = Arc::new(Instance::new(instance.clone(), entry.clone(), true));

        let debug_messenger = if cfg!(debug_assertions) {
            Some(DebugMessenger::new(instance.clone())?)
        } else {
            None
        };

        Ok(Self {
            _entry: entry,
            instance,
            _debug_messenger: debug_messenger,
        }
        .into())
    }

    /// Creates an instance that does not own `ash::Instance`. The instance is not destroyed on drop.
    pub fn new_unowned(instance: ash::Instance, entry: Arc<Entry>) -> Self {
        let instance = Arc::new(Instance::new(instance.clone(), entry.clone(), false));
        Self {
            _entry: entry,
            instance,
            _debug_messenger: None,
        }
    }

    /// Creates an adapter that meets requirements specified in the descriptor.
    pub fn create_adapter<'a>(
        &'a self,
        descriptor: &VideoAdapterDescriptor,
    ) -> Result<VideoAdapter<'a>, VulkanInitError> {
        self.iter_adapters()?
            .find(|adapter| {
                (!descriptor.supports_decoding || adapter.supports_decoding())
                    && (!descriptor.supports_encoding || adapter.supports_encoding())
            })
            .ok_or(VulkanInitError::NoDevice)
    }

    /// Iterator over all available [`VulkanAdapter`]s that support at least decoding or encoding.
    pub fn iter_adapters<'a>(
        &'a self,
    ) -> Result<impl Iterator<Item = VideoAdapter<'a>> + 'a, VulkanInitError> {
        crate::adapter::iter_adapters(self)
    }
}

impl std::fmt::Debug for VideoInstance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VulkanInstance").finish()
    }
}
