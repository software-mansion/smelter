use std::{
    ptr::{NonNull, null, null_mut},
    sync::Arc,
};

#[cfg(feature = "wgpu")]
use crate::{
    WgpuTexturesDecoder,
    backends::{WgpuBackend, video_toolbox::error::VTInitError},
    device::WgpuVideoDeviceBackend,
    global_registry::{GlobalRegistry, VideoDeviceKey},
};
#[cfg(feature = "wgpu")]
use objc2_metal::MTLDevice;

use crate::{
    VideoEncoderError, adapter::{DeviceType, VideoAdapterBackend, VideoAdapterInfo}, backends::{
        CoreBackend,
        video_toolbox::{
            decoder::VTDecoder,
            error::{OSStatusError, OSStatusExt},
        },
    }, device::CoreVideoDeviceBackend, frame_sorter::FrameSorter, instance::VideoInstanceBackend, parser::{h264::H264Parser, reference_manager::ReferenceContext},
};

use objc2_core_foundation as cf;

mod caps;
mod decoder;
mod error;
mod wgpu_api;

pub struct VTBackend;

impl CoreBackend for VTBackend {
    fn new_instance(
        &self,
        _desc: &crate::instance::VideoInstanceDescriptor,
    ) -> Result<
        std::sync::Arc<dyn crate::instance::VideoInstanceBackend>,
        crate::VideoInstanceInitError,
    > {
        Ok(Arc::new(VTInstance {}))
    }
}

#[cfg(feature = "wgpu")]
impl WgpuBackend for VTBackend {
    fn device_key_from_wgpu_device(
        &self,
        device: &wgpu::Device,
    ) -> crate::global_registry::VideoDeviceKey {
        let hal = unsafe { device.as_hal::<wgpu::hal::metal::Api>().unwrap() };
        let registry_id = hal.raw_device().registryID();
        VideoDeviceKey::Metal { registry_id }
    }

    fn retrieve_adapter_info(
        &self,
        wgpu_adapter: &wgpu::Adapter,
    ) -> Option<crate::capabilities::VideoAdapterInfo> {
        let info = wgpu_adapter.get_info();
        let decode_capabilities = caps::query_decode_capabilities();
        let encode_capabilities = caps::query_encode_capabilities();

        Some(VideoAdapterInfo {
            name: info.name,
            driver_name: info.driver,
            driver_info: info.driver_info,
            device: info.device.to_string(),
            device_type: info.device_type.into(),
            vendor: info.vendor.to_string(),
            api_version: query_api_version(),
            supports_decoding: decode_capabilities.h264.is_some()
                || decode_capabilities.h265.is_some(),
            supports_encoding: encode_capabilities.h264.is_some()
                || encode_capabilities.h265.is_some(),
            decode_capabilities,
            encode_capabilities,
        })
    }

    fn create_and_register_device(
        &self,
        wgpu_adapter: &wgpu::Adapter,
        desc: &crate::parameters::VideoDeviceDescriptor,
    ) -> Result<(wgpu::Device, wgpu::Queue), crate::VideoDeviceInitError> {
        let (device, queue) =
            pollster::block_on(wgpu_adapter.request_device(&wgpu::DeviceDescriptor {
                label: Some("wgpu device created by the videotoolbox decoder"),
                required_features: desc.wgpu_features | wgpu::Features::TEXTURE_FORMAT_NV12,
                required_limits: desc.wgpu_limits.clone(),
                experimental_features: desc.wgpu_experimental_features,
                ..Default::default()
            }))
            .map_err(crate::WgpuInitError::WgpuRequestDeviceError)
            .map_err(VTInitError::from)?;

        let id = VTBackend.device_key_from_wgpu_device(&device);
        // VTDevice is empty, and MTLDevices actually only get destroyed at process exit.
        // Because of this, we never remove from the registry.
        GlobalRegistry::register_device(id, Arc::new(VTDevice {}));
        Ok((device, queue))
    }
}

pub struct VTInstance {}

impl VideoInstanceBackend for VTInstance {
    fn iter_adapters<'a>(
        &'a self,
    ) -> Result<Box<dyn Iterator<Item = crate::VideoAdapter<'a>> + 'a>, crate::VideoInstanceInitError>
    {
        Ok(Box::new(std::iter::once(
            crate::VideoAdapter::from_backend(VTAdapter),
        )))
    }
}

pub struct VTAdapter;

impl VideoAdapterBackend for VTAdapter {
    fn build_info(&self) -> VideoAdapterInfo {
        let name = sysctlbyname_string("machdep.cpu.brand_string").unwrap_or_else(|_| "".into());
        let device_type = if cfg!(target_arch = "aarch64") {
            DeviceType::IntegratedGpu
        } else {
            DeviceType::Other
        };
        let api_version = query_api_version();

        let decode_capabilities = caps::query_decode_capabilities();
        let encode_capabilities = caps::query_encode_capabilities();

        let supports_decoding =
            decode_capabilities.h264.is_some() || decode_capabilities.h265.is_some();
        let supports_encoding =
            encode_capabilities.h264.is_some() || encode_capabilities.h265.is_some();

        VideoAdapterInfo {
            name,
            driver_name: "".into(),
            driver_info: "".into(),
            device: "0".into(),
            device_type,
            vendor: "0".into(),
            api_version,
            supports_decoding,
            supports_encoding,
            decode_capabilities,
            encode_capabilities,
        }
    }

    fn create_device(
        self: Box<Self>,
        _: &crate::device::VideoDeviceDescriptor,
    ) -> Result<crate::VideoDevice, crate::VideoDeviceInitError> {
        Ok(crate::VideoDevice {
            inner: Arc::new(VTDevice {}),
            #[cfg(feature = "wgpu")]
            wgpu_device: None,
        })
    }
}

fn query_api_version() -> String {
    sysctlbyname_string("kern.osproductversion").unwrap_or_else(|_| "???".into())
}

struct VTDevice {}

impl CoreVideoDeviceBackend for VTDevice {
    fn create_bytes_decoder_h264(
        self: Arc<Self>,
        parameters: crate::device::DecoderParameters,
    ) -> Result<crate::BytesDecoder, crate::VideoDecoderError> {
        #[cfg(feature = "wgpu")]
        let decoder = VTDecoder::new(None)?;
        #[cfg(not(feature = "wgpu"))]
        let decoder = VTDecoder::new()?;

        // TODO: decoder usage
        Ok(crate::BytesDecoder {
            decoder: Box::new(decoder),
            parser: H264Parser::new_avcc_output(),
            reference_ctx: ReferenceContext::new(parameters.missed_frame_handling),
            frame_sorter: FrameSorter::default(),
        })
    }

    fn create_bytes_encoder_h264(
        self: Arc<Self>,
        _parameters: crate::device::EncoderParametersH264,
    ) -> Result<crate::BytesEncoderH264, crate::VideoEncoderError> {
        Err(VideoEncoderError::EncoderUnsupported)
    }

    fn create_bytes_encoder_h265(
        self: Arc<Self>,
        _parameters: crate::device::EncoderParametersH265,
    ) -> Result<crate::BytesEncoderH265, crate::VideoEncoderError> {
        Err(VideoEncoderError::EncoderUnsupported)
    }

    fn decode_capabilities(&self) -> crate::capabilities::DecodeCapabilities {
        caps::query_decode_capabilities()
    }

    fn encode_capabilities(&self) -> crate::capabilities::EncodeCapabilities {
        caps::query_encode_capabilities()
    }
}

#[cfg(feature = "wgpu")]
impl WgpuVideoDeviceBackend for VTDevice {
    fn create_wgpu_textures_decoder_h264(
        self: Arc<Self>,
        wgpu_device: wgpu::Device,
        parameters: crate::device::DecoderParameters,
    ) -> Result<crate::WgpuTexturesDecoder, crate::VideoDecoderError> {
        let decoder = VTDecoder::new(Some(&wgpu_device))?;

        // TODO: decoder usage
        Ok(WgpuTexturesDecoder {
            wgpu_device,
            decoder: Box::new(decoder),
            parser: H264Parser::new_avcc_output(),
            reference_ctx: ReferenceContext::new(parameters.missed_frame_handling),
            frame_sorter: FrameSorter::default(),
        })
    }

    fn create_wgpu_textures_encoder_h264(
        self: Arc<Self>,
        _wgpu_device: wgpu::Device,
        _wgpu_queue: wgpu::Queue,
        _parameters: crate::device::EncoderParametersH264,
    ) -> Result<crate::WgpuTexturesEncoderH264, crate::VideoEncoderError> {
        Err(VideoEncoderError::EncoderUnsupported)
    }

    fn create_wgpu_textures_encoder_h265(
        self: Arc<Self>,
        _wgpu_device: wgpu::Device,
        _wgpu_queue: wgpu::Queue,
        _parameters: crate::device::EncoderParametersH265,
    ) -> Result<crate::WgpuTexturesEncoderH265, crate::VideoEncoderError> {
        Err(VideoEncoderError::EncoderUnsupported)
    }
}

trait OutPtr<R> {
    fn null() -> Self;
    fn nonnull(self) -> Option<NonNull<R>>;
}

impl<R> OutPtr<R> for *const R {
    fn null() -> Self {
        null()
    }

    fn nonnull(self) -> Option<NonNull<R>> {
        NonNull::new(self as *mut R)
    }
}

impl<R> OutPtr<R> for *mut R {
    fn null() -> Self {
        null_mut()
    }

    fn nonnull(self) -> Option<NonNull<R>> {
        NonNull::new(self)
    }
}

fn allocate_retained<R: objc2_core_foundation::Type, P: OutPtr<R>, F: FnOnce(NonNull<P>) -> i32>(
    f: F,
) -> Result<cf::CFRetained<R>, OSStatusError> {
    let mut p = P::null();
    f(NonNull::from(&mut p)).osstatus()?;
    Ok(unsafe {
        cf::CFRetained::from_raw(
            p.nonnull()
                .expect("Apple API returned success but wrote a null pointer"),
        )
    })
}

fn sysctlbyname_string(name: &str) -> Result<String, std::io::Error> {
    let mut len = 0;
    let name = name.as_ptr() as *const i8;
    let result = unsafe { libc::sysctlbyname(name, null_mut(), &mut len, null_mut(), 0) };
    if result != 0 {
        return Err(std::io::Error::last_os_error());
    }

    let mut bytes = vec![0u8; len];
    let result =
        unsafe { libc::sysctlbyname(name, bytes.as_mut_ptr() as *mut _, &mut len, null_mut(), 0) };
    if result != 0 {
        return Err(std::io::Error::last_os_error());
    }

    // unwrapping, cause if the kernel gives us a bad string i think its ok to die
    Ok(std::ffi::CString::from_vec_with_nul(bytes)
        .unwrap()
        .into_string()
        .unwrap())
}
