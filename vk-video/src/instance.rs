use std::sync::Arc;

use ash::{Entry, vk};

use crate::{VulkanInitError, adapter::VulkanAdapter, wrappers::*};

/// Context for all encoders and decoders. Also contains a [`wgpu::Instance`].
pub struct VulkanInstance {
    pub(crate) wgpu_instance: wgpu::Instance,
    _entry: Arc<Entry>,
    pub(crate) instance: Arc<Instance>,
    _debug_messenger: Option<DebugMessenger>,
}

impl VulkanInstance {
    pub fn new() -> Result<Arc<Self>, VulkanInitError> {
        let entry = Arc::new(unsafe { Entry::load()? });
        Self::new_from_entry(entry)
    }

    pub fn wgpu_instance(&self) -> wgpu::Instance {
        self.wgpu_instance.clone()
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

        let requested_layers = if cfg!(debug_assertions) {
            let mut res = vec![c"VK_LAYER_KHRONOS_validation"];

            if cfg!(feature = "vk_api_dump") {
                res.push(c"VK_LAYER_LUNARG_api_dump");
            }

            res
        } else {
            Vec::new()
        };

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

        let extensions = if cfg!(debug_assertions) {
            vec![vk::EXT_DEBUG_UTILS_NAME]
        } else {
            Vec::new()
        };

        let wgpu_extensions = wgpu::hal::vulkan::Instance::desired_extensions(
            &entry,
            api_version,
            wgpu::InstanceFlags::empty(),
        )?;

        let extensions = extensions
            .into_iter()
            .chain(wgpu_extensions)
            .collect::<Vec<_>>();

        let extension_ptrs = extensions.iter().map(|e| e.as_ptr()).collect::<Vec<_>>();

        let create_info = vk::InstanceCreateInfo::default()
            .application_info(&app_info)
            .enabled_layer_names(&layers)
            .enabled_extension_names(&extension_ptrs);

        let instance = unsafe { entry.create_instance(&create_info, None) }?;
        let video_queue_instance_ext = ash::khr::video_queue::Instance::new(&entry, &instance);
        let video_encode_queue_instance_ext =
            ash::khr::video_encode_queue::Instance::new(&entry, &instance);
        let debug_utils_instance_ext = ash::ext::debug_utils::Instance::new(&entry, &instance);

        let instance = Arc::new(Instance {
            instance,
            _entry: entry.clone(),
            video_queue_instance_ext,
            debug_utils_instance_ext,
            video_encode_queue_instance_ext,
        });

        let debug_messenger = if cfg!(debug_assertions) {
            Some(DebugMessenger::new(instance.clone())?)
        } else {
            None
        };

        let instance_clone = instance.clone();

        let wgpu_instance = unsafe {
            wgpu::hal::vulkan::Instance::from_raw(
                (*entry).clone(),
                instance.instance.clone(),
                api_version,
                0,
                None,
                extensions,
                wgpu::InstanceFlags::ALLOW_UNDERLYING_NONCOMPLIANT_ADAPTER,
                false,
                Some(Box::new(move || {
                    drop(instance_clone);
                })),
            )?
        };

        let wgpu_instance =
            unsafe { wgpu::Instance::from_hal::<wgpu::hal::vulkan::Api>(wgpu_instance) };

        Ok(Self {
            _entry: entry,
            instance,
            _debug_messenger: debug_messenger,
            wgpu_instance,
        }
        .into())
    }

    /// Creates an adapter that supports both decoding and encoding.
    ///
    /// If your hardware only supports one of the operations, use [`VulkanInstance::iter_adapters`] and choose an adapter manually.
    pub fn create_adapter<'a>(
        &'a self,
        compatible_surface: Option<&'a wgpu::Surface<'_>>,
    ) -> Result<VulkanAdapter<'a>, VulkanInitError> {
        self.iter_adapters(compatible_surface)?
            .find(|a| a.supports_decoding() && a.supports_encoding())
            .ok_or(VulkanInitError::NoDevice)
    }

    /// Iterator over all available [`VulkanAdapter`]s that support at least decoding or encoding.
    pub fn iter_adapters<'a>(
        &'a self,
        compatible_surface: Option<&'a wgpu::Surface<'_>>,
    ) -> Result<impl Iterator<Item = VulkanAdapter<'a>> + 'a, VulkanInitError> {
        crate::adapter::iter_adapters(self, compatible_surface)
    }
}

impl std::fmt::Debug for VulkanInstance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VulkanInstance").finish()
    }
}
