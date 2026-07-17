use std::fmt::{self, Debug};

use crate::{
    VideoDeviceInitError,
    capabilities::{DecodeCapabilities, EncodeCapabilities},
    device::VideoDeviceDescriptor,
};

#[cfg(feature = "wgpu")]
mod wgpu_api;
#[cfg(feature = "wgpu")]
pub use wgpu_api::*;

pub(crate) trait VideoAdapterBackend {
    fn build_info(&self) -> VideoAdapterInfo;

    fn create_device(
        self: Box<Self>,
        desc: &VideoDeviceDescriptor,
    ) -> Result<crate::VideoDevice, VideoDeviceInitError>;
}

/// Represents a handle to a physical device.
/// Can be used to create [`VideoDevice`](crate::VideoDevice).
pub struct VideoAdapter<'a> {
    adapter: Box<dyn VideoAdapterBackend + 'a>,
    adapter_info: VideoAdapterInfo,
}

impl<'a> VideoAdapter<'a> {
    pub(crate) fn from_backend(backend: impl VideoAdapterBackend + 'a) -> Self {
        let adapter_info = backend.build_info();
        Self {
            adapter: Box::new(backend),
            adapter_info,
        }
    }

    pub fn info(&self) -> &VideoAdapterInfo {
        &self.adapter_info
    }

    pub fn supports_decoding(&self) -> bool {
        self.adapter_info.supports_decoding
    }

    pub fn supports_encoding(&self) -> bool {
        self.adapter_info.supports_encoding
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeviceType {
    Other,
    IntegratedGpu,
    DiscreteGpu,
    VirtualGpu,
    Cpu,
}

#[cfg(feature = "wgpu")]
impl From<wgpu::DeviceType> for DeviceType {
    fn from(value: wgpu::DeviceType) -> Self {
        match value {
            wgpu::DeviceType::Other => DeviceType::Other,
            wgpu::DeviceType::IntegratedGpu => DeviceType::IntegratedGpu,
            wgpu::DeviceType::DiscreteGpu => DeviceType::DiscreteGpu,
            wgpu::DeviceType::VirtualGpu => DeviceType::VirtualGpu,
            wgpu::DeviceType::Cpu => DeviceType::Cpu,
        }
    }
}

#[derive(Clone)]
pub struct VideoAdapterInfo {
    pub name: String,
    pub driver_name: String,
    pub driver_info: String,
    pub device: String,
    pub device_type: DeviceType,
    pub vendor: String,
    pub api_version: String,
    pub supports_decoding: bool,
    pub supports_encoding: bool,
    pub decode_capabilities: DecodeCapabilities,
    pub encode_capabilities: EncodeCapabilities,
}

impl Debug for VideoAdapterInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AdapterInfo")
            .field("name", &self.name)
            .field("device_type", &self.device_type)
            .field("api_version", &self.api_version)
            .field("driver", &self.driver_name)
            .field("driver_info", &self.driver_info)
            .field("vendor", &self.vendor)
            .field("device", &self.device)
            .field("supports_decoding", &self.supports_decoding)
            .field("supports_encoding", &self.supports_encoding)
            .finish()
    }
}
