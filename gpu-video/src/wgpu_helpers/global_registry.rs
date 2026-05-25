use std::sync::{Arc, LazyLock, RwLock};

use ash::vk;
use rustc_hash::FxHashMap;
use wgpu::hal::vulkan::Api as VkApi;

use crate::VideoDevice;

#[derive(Default)]
pub(crate) struct GlobalRegistry {
    devices: FxHashMap<VideoDeviceKey, Arc<VideoDevice>>,
}

static REGISTRY: LazyLock<RwLock<GlobalRegistry>> =
    LazyLock::new(|| RwLock::new(GlobalRegistry::default()));

impl GlobalRegistry {
    pub(crate) fn register_device(handle: VideoDeviceKey, device: Arc<VideoDevice>) {
        let mut registry = REGISTRY.write().unwrap();

        use std::collections::hash_map::Entry;
        match registry.devices.entry(handle) {
            Entry::Occupied(_) => {
                tracing::debug!("Tried to register device that already exists in the registry");
            }
            Entry::Vacant(vacant_entry) => {
                vacant_entry.insert(device);
            }
        }
    }

    pub(crate) fn unregister_device(handle: &VideoDeviceKey) {
        let mut registry = REGISTRY.write().unwrap();
        if registry.devices.remove(handle).is_none() {
            tracing::debug!("Tried to unregister device that does not exist in the registry");
        }
    }

    pub(crate) fn get_device(handle: &VideoDeviceKey) -> Result<Arc<VideoDevice>, RegistryError> {
        let registry = REGISTRY.read().unwrap();
        registry
            .devices
            .get(handle)
            .cloned()
            .ok_or(RegistryError::DeviceNotFound)
    }
}

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub(crate) struct VideoDeviceKey(vk::Device, vk::Queue);

impl From<&wgpu::Device> for VideoDeviceKey {
    fn from(device: &wgpu::Device) -> Self {
        let hal_device = unsafe { device.as_hal::<VkApi>().unwrap() };
        Self(hal_device.raw_device().handle(), hal_device.raw_queue())
    }
}

// TODO: Maybe it would be better to just panic? It would be kinda inline with what wgpu would probably do?
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error(
        "Could not find the device in registry. Make sure the device was created with video capabilities"
    )]
    DeviceNotFound,
}
