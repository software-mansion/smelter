pub mod h264;

#[cfg(feature = "wgpu")]
use wgpu::hal::api::Vulkan as VkApi;

mod display;
mod sys;
mod va;
mod vpl;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Nv12Plane {
    Y,
    Uv,
}

impl Nv12Plane {
    fn aspect(self) -> wgpu::TextureAspect {
        match self {
            Self::Y => wgpu::TextureAspect::Plane0,
            Self::Uv => wgpu::TextureAspect::Plane1,
        }
    }

    fn bytes_per_texel(self) -> u32 {
        match self {
            Self::Y => 1,
            Self::Uv => 2,
        }
    }
}

#[cfg(feature = "wgpu")]
fn required_wgpu_features() -> wgpu::Features {
    crate::dmabuf::required_wgpu_features()
}

#[cfg(feature = "wgpu")]
pub fn supported_wgpu_features(adapter: &wgpu::Adapter) -> wgpu::Features {
    supported_wgpu_features_from(
        adapter.features(),
        supports_required_vulkan_device_extensions(adapter),
    )
}

#[cfg(feature = "wgpu")]
pub fn supports_wgpu_device(device: &wgpu::Device) -> bool {
    crate::dmabuf::DmaBufInterop::new(device).is_ok()
}

#[cfg(feature = "wgpu")]
pub(crate) fn supported_wgpu_features_from(
    adapter_features: wgpu::Features,
    supports_required_vulkan_device_extensions: bool,
) -> wgpu::Features {
    let required = required_wgpu_features();
    if adapter_features.contains(required) && supports_required_vulkan_device_extensions {
        required
    } else {
        wgpu::Features::empty()
    }
}

#[cfg(feature = "wgpu")]
pub fn create_wgpu_device(
    adapter: &wgpu::Adapter,
    descriptor: &wgpu::DeviceDescriptor<'_>,
) -> Result<(wgpu::Device, wgpu::Queue), String> {
    let hal_adapter = unsafe {
        adapter.as_hal::<VkApi>().ok_or_else(|| {
            "Intel Quick Sync requires a Vulkan wgpu adapter".to_string()
        })?
    };
    let capabilities = hal_adapter.physical_device_capabilities();
    if let Some(extension) =
        crate::dmabuf::missing_required_vulkan_device_extension(|extension| {
            capabilities.supports_extension(extension)
        })
    {
        return Err(format!(
            "Intel Quick Sync DMA-BUF interop requires Vulkan device extension {}",
            extension.to_string_lossy()
        ));
    }

    let hal_device = unsafe {
        hal_adapter.open_with_callback(
            descriptor.required_features,
            &descriptor.required_limits,
            &descriptor.memory_hints,
            Some(Box::new(move |args| {
                for extension in crate::dmabuf::REQUIRED_VULKAN_DEVICE_EXTENSIONS {
                    if !args.extensions.contains(&extension) {
                        args.extensions.push(extension);
                    }
                }
            })),
        )
    }
    .map_err(|err| format!("failed to create Vulkan wgpu-hal device: {err}"))?;
    unsafe { adapter.create_device_from_hal::<VkApi>(hal_device, descriptor) }
        .map_err(|err| format!("failed to create wgpu device from Vulkan HAL: {err}"))
}

#[cfg(feature = "wgpu")]
fn supports_required_vulkan_device_extensions(adapter: &wgpu::Adapter) -> bool {
    let hal_adapter = unsafe { adapter.as_hal::<VkApi>() };
    let Some(hal_adapter) = hal_adapter else {
        return false;
    };

    let capabilities = hal_adapter.physical_device_capabilities();
    crate::dmabuf::missing_required_vulkan_device_extension(|extension| {
        capabilities.supports_extension(extension)
    })
    .is_none()
}

#[cfg(all(test, feature = "wgpu"))]
mod tests {
    use super::*;

    #[test]
    fn wgpu_features_require_dma_buf_import_and_vulkan_extensions() {
        let required = required_wgpu_features();

        assert_eq!(supported_wgpu_features_from(required, true), required);
        assert!(supported_wgpu_features_from(required, false).is_empty());
        assert!(supported_wgpu_features_from(wgpu::Features::empty(), true).is_empty());
    }
}
