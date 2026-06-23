use ash::vk;
use std::fmt::{self, Debug};

use crate::{
    VideoDeviceInitError,
    capabilities::EncodeCapabilities,
    device::{VideoDeviceDescriptor, caps::DecodeCapabilities},
};

#[cfg(feature = "wgpu")]
mod wgpu_api;
#[cfg(feature = "wgpu")]
pub use wgpu_api::*;

/// Backend implementation behind a [`VideoAdapter`].
pub trait VideoAdapterBackend {
    fn info(&self) -> &VideoAdapterInfo;

    fn create_device(
        self: Box<Self>,
        desc: &VideoDeviceDescriptor,
    ) -> Result<crate::VideoDevice, VideoDeviceInitError>;
}

/// Represents a handle to a physical device.
/// Can be used to create [`VideoDevice`](crate::VideoDevice).
pub struct VideoAdapter<'a> {
    adapter: Box<dyn VideoAdapterBackend + 'a>,
}

impl<'a> VideoAdapter<'a> {
    pub(crate) fn from_backend(backend: impl VideoAdapterBackend + 'a) -> Self {
        Self {
            adapter: Box::new(backend),
        }
    }

    pub fn info(&self) -> &VideoAdapterInfo {
        self.adapter.info()
    }

    pub fn supports_decoding(&self) -> bool {
        self.adapter.info().supports_decoding
    }

    pub fn supports_encoding(&self) -> bool {
        self.adapter.info().supports_encoding
    }

    pub fn create_device(
        self,
        desc: &VideoDeviceDescriptor,
    ) -> Result<crate::VideoDevice, VideoDeviceInitError> {
        self.adapter.create_device(desc)
    }
}

// TODO: maybe there should be a way of specifying power preference / device preference (like wgpu)
/// Describes a [`VideoAdapter`].
/// Used by [`VideoInstance::create_adapter`](crate::VideoInstance::create_adapter)
pub struct VideoAdapterDescriptor {
    pub supports_decoding: bool,
    pub supports_encoding: bool,
}

impl Default for VideoAdapterDescriptor {
    fn default() -> Self {
        Self {
            supports_decoding: true,
            supports_encoding: true,
        }
    }
}

#[derive(Clone)]
pub struct VideoAdapterInfo {
    pub name: String,
    pub driver_name: String,
    pub driver_info: String,
    pub device_type: vk::PhysicalDeviceType,
    pub supports_decoding: bool,
    pub supports_encoding: bool,
    pub device_properties: vk::PhysicalDeviceProperties,
    pub decode_capabilities: DecodeCapabilities,
    pub encode_capabilities: EncodeCapabilities,
}

impl Debug for VideoAdapterInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::fmt::Result {
        let version = {
            let version = self.device_properties.api_version;
            let major = vk::api_version_major(version);
            let minor = vk::api_version_minor(version);
            let patch = vk::api_version_patch(version);

            format!("{major}.{minor}.{patch}")
        };
        f.debug_struct("AdapterInfo")
            .field("name", &self.name)
            .field("device_type", &self.device_type)
            .field("api_version", &version)
            .field("driver", &self.driver_name)
            .field("driver_info", &self.driver_info)
            .field("vendor", &self.device_properties.vendor_id)
            .field("device", &self.device_properties.device_id)
            .field("supports_decoding", &self.supports_decoding)
            .field("supports_encoding", &self.supports_encoding)
            .finish()
    }
}
