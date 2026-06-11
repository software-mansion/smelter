use std::sync::{Arc, LazyLock, RwLock};

use ash::vk;
use rustc_hash::FxHashMap;
use wgpu::hal::vulkan::Api as VkApi;

use crate::device::VideoDevice;

#[derive(Default)]
pub(crate) struct GlobalRegistry {
    devices: FxHashMap<VideoDeviceKey, Arc<VideoDevice>>,
}

static REGISTRY: LazyLock<RwLock<GlobalRegistry>> =
    LazyLock::new(|| RwLock::new(GlobalRegistry::default()));

impl GlobalRegistry {
    pub(crate) fn register_device(key: VideoDeviceKey, device: Arc<VideoDevice>) {
        let mut registry = REGISTRY.write().unwrap();

        use std::collections::hash_map::Entry;
        match registry.devices.entry(key) {
            Entry::Occupied(_) => {
                tracing::debug!("Tried to register device that already exists in the registry");
            }
            Entry::Vacant(vacant_entry) => {
                vacant_entry.insert(device);
            }
        }
    }

    pub(crate) fn unregister_device(key: &VideoDeviceKey) {
        let mut registry = REGISTRY.write().unwrap();
        if registry.devices.remove(key).is_none() {
            tracing::debug!("Tried to unregister device that does not exist in the registry");
        }
    }

    pub(crate) fn get_device(key: &VideoDeviceKey) -> Result<Arc<VideoDevice>, RegistryError> {
        let registry = REGISTRY.read().unwrap();
        registry
            .devices
            .get(key)
            .cloned()
            .ok_or(RegistryError::DeviceNotFound)
    }
}

// TODO: metal key
#[cfg(vulkan)]
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub(crate) struct VideoDeviceKey(pub(crate) vk::Device, pub(crate) vk::Queue);

#[cfg(vulkan)]
impl From<&wgpu::Device> for VideoDeviceKey {
    fn from(device: &wgpu::Device) -> Self {
        let hal_device = unsafe { device.as_hal::<VkApi>().unwrap() };
        Self(hal_device.raw_device().handle(), hal_device.raw_queue())
    }
}

impl From<&crate::VideoDevice> for VideoDeviceKey {
    fn from(device: &crate::VideoDevice) -> Self {
        device.handle.0
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error(
        "Could not find the device in registry. Make sure the device was created with video capabilities"
    )]
    DeviceNotFound,
}
