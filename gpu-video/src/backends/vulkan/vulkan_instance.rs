use std::{ffi::CStr, sync::Arc};

use ash::{Entry, vk};

use crate::{
    VideoBackendError, VideoInstanceInitError,
    adapter::VideoAdapter,
    backends::vulkan::vulkan_adapter::VulkanAdapter,
    instance::{VideoInstanceBackend, VideoInstanceDescriptor},
    wrappers::*,
};

pub struct VulkanInstance {
    _entry: Entry,
    pub(crate) instance: Arc<Instance>,
    _debug_messenger: Option<DebugMessenger>,
}

impl VideoInstanceBackend for VulkanInstance {
    fn iter_adapters<'a>(
        &'a self,
    ) -> Result<Box<dyn Iterator<Item = VideoAdapter<'a>> + 'a>, VideoInstanceInitError> {
        VulkanInstance::iter_adapters(self)
    }
}

impl VulkanInstance {
    pub(crate) fn new(desc: &VideoInstanceDescriptor) -> Result<Self, VulkanInstanceInitError> {
        let entry = unsafe { Entry::load()? };
        Self::new_from_entry(entry, &mut Vec::new(), desc)
    }

    pub fn new_from_entry(
        entry: Entry,
        extensions: &mut Vec<&'static CStr>,
        desc: &VideoInstanceDescriptor,
    ) -> Result<Self, VulkanInstanceInitError> {
        let api_version = vk::make_api_version(0, 1, 3, 0);
        let app_info = vk::ApplicationInfo {
            api_version,
            ..Default::default()
        };

        let mut requested_layers = Vec::new();
        if desc.enable_validations {
            requested_layers.push(c"VK_LAYER_KHRONOS_validation");
        }
        if desc.enable_api_dump {
            requested_layers.push(c"VK_LAYER_LUNARG_api_dump");
        }

        let instance_layer_properties = unsafe { entry.enumerate_instance_layer_properties()? };
        let instance_layer_names = instance_layer_properties
            .iter()
            .map(|layer| layer.layer_name_as_c_str())
            .collect::<Result<Vec<_>, _>>()
            .map_err(VulkanInstanceInitError::InvalidLayerName)?;

        let layers = requested_layers
            .into_iter()
            .filter(|requested_layer_name| {
                instance_layer_names
                    .iter()
                    .any(|instance_layer_name| instance_layer_name == requested_layer_name)
            })
            .map(|layer| layer.as_ptr())
            .collect::<Vec<_>>();

        if !extensions.contains(&vk::EXT_DEBUG_UTILS_NAME) {
            extensions.push(vk::EXT_DEBUG_UTILS_NAME);
        }

        let extension_ptrs = extensions.iter().map(|e| e.as_ptr()).collect::<Vec<_>>();

        let create_info = vk::InstanceCreateInfo::default()
            .application_info(&app_info)
            .enabled_layer_names(&layers)
            .enabled_extension_names(&extension_ptrs);

        let instance = unsafe { entry.create_instance(&create_info, None) }?;
        let instance = Arc::new(Instance::new(
            instance.clone(),
            entry.clone(),
            desc.enable_validations,
            true,
        ));

        let debug_messenger = if desc.enable_validations {
            Some(DebugMessenger::new(instance.clone())?)
        } else {
            None
        };

        Ok(Self {
            _entry: entry,
            instance,
            _debug_messenger: debug_messenger,
        })
    }

    /// Creates an instance that does not own `ash::Instance`. The instance is not destroyed on drop.
    #[cfg_attr(not(feature = "wgpu"), allow(dead_code))]
    pub(crate) fn new_unowned(
        instance: ash::Instance,
        entry: Entry,
        desc: &VideoInstanceDescriptor,
    ) -> Self {
        let instance = Arc::new(Instance::new(
            instance.clone(),
            entry.clone(),
            desc.enable_validations,
            false,
        ));
        Self {
            _entry: entry,
            instance,
            _debug_messenger: None,
        }
    }

    #[cfg_attr(not(feature = "expose-backends"), allow(dead_code))]
    pub fn raw_instance(&self) -> ash::Instance {
        self.instance.instance.clone()
    }

    pub fn iter_adapters<'a>(
        &'a self,
    ) -> Result<Box<dyn Iterator<Item = VideoAdapter<'a>> + 'a>, VideoInstanceInitError> {
        let physical_devices = unsafe {
            self.instance
                .enumerate_physical_devices()
                .map_err(VulkanInstanceInitError::VkError)?
        };
        Ok(Box::new(physical_devices.into_iter().filter_map(
            move |device| VulkanAdapter::new(self, device).map(VideoAdapter::from_backend),
        )))
    }
}

#[derive(thiserror::Error, Debug)]
pub enum VulkanInstanceInitError {
    #[error("Error loading vulkan: {0}")]
    LoadingError(#[from] ash::LoadingError),

    #[error("Vulkan error: {0}")]
    VkError(#[from] vk::Result),

    #[error("Missing required extension: {0}")]
    MissingExtension(String),

    #[error("Invalid layer name: {0}")]
    InvalidLayerName(#[source] std::ffi::FromBytesUntilNulError),
}

impl From<VulkanInstanceInitError> for VideoInstanceInitError {
    fn from(err: VulkanInstanceInitError) -> Self {
        match err {
            VulkanInstanceInitError::LoadingError(_)
            | VulkanInstanceInitError::VkError(_)
            | VulkanInstanceInitError::MissingExtension(_)
            | VulkanInstanceInitError::InvalidLayerName(_) => {
                Self::BackendError(VideoBackendError {
                    message: err.to_string(),
                    source: Box::new(err),
                })
            }
        }
    }
}
