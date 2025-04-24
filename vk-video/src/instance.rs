use std::sync::Arc;

use ash::{vk, Entry};

use crate::{device::VulkanDevice, wrappers::*, VulkanInitError};

/// Context for all encoders, decoders. Also contains a [`wgpu::Instance`].
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
            vec![c"VK_LAYER_KHRONOS_validation"]
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

    pub fn create_device(
        &self,
        wgpu_features: wgpu::Features,
        wgpu_limits: wgpu::Limits,
        compatible_surface: Option<&wgpu::Surface<'_>>,
    ) -> Result<Arc<VulkanDevice>, VulkanInitError> {
        Ok(VulkanDevice::new(self, wgpu_features, wgpu_limits, compatible_surface)?.into())
    }
}

impl std::fmt::Debug for VulkanInstance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VulkanInstance").finish()
    }
}
