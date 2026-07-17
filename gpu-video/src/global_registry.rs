use std::sync::{Arc, LazyLock, RwLock};

use rustc_hash::FxHashMap;

use crate::device::VideoDeviceBackend;

#[derive(Default)]
pub(crate) struct GlobalRegistry {
    devices: FxHashMap<VideoDeviceKey, Arc<dyn VideoDeviceBackend>>,
}

static REGISTRY: LazyLock<RwLock<GlobalRegistry>> =
    LazyLock::new(|| RwLock::new(GlobalRegistry::default()));

impl GlobalRegistry {
    pub(crate) fn register_device(key: VideoDeviceKey, device: Arc<dyn VideoDeviceBackend>) {
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

    #[cfg_attr(video_toolbox, allow(unused))]
    pub(crate) fn unregister_device(key: &VideoDeviceKey) {
        let mut registry = REGISTRY.write().unwrap();
        if registry.devices.remove(key).is_none() {
            tracing::debug!("Tried to unregister device that does not exist in the registry");
        }
    }

    pub(crate) fn get_device(
        key: &VideoDeviceKey,
    ) -> Result<Arc<dyn VideoDeviceBackend>, RegistryError> {
        let registry = REGISTRY.read().unwrap();
        registry
            .devices
            .get(key)
            .cloned()
            .ok_or(RegistryError::DeviceNotFound)
    }
}

#[allow(dead_code)]
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub(crate) enum VideoDeviceKey {
    Vulkan {
        device_handle: u64,
        queue_handle: u64,
    },
    Metal {
        registry_id: u64,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error(
        "Could not find the device in the registry. Make sure the device was created with video capabilities"
    )]
    DeviceNotFound,
}
