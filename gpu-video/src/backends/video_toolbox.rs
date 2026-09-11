use std::{
    ptr::{NonNull, null, null_mut},
    sync::Arc,
};

use crate::{
    adapter::{DeviceType, VideoAdapterBackend, VideoAdapterInfo},
    backends::{
        CoreBackend,
        video_toolbox::{
            decoder::VTDecoder,
            encoder::{H264Codec, H265Codec, VTEncoder},
            error::{OSStatusError, OSStatusExt},
        },
    },
    device::CoreVideoDeviceBackend,
    frame_sorter::FrameSorter,
    instance::VideoInstanceBackend,
    parser::{h264::H264Parser, reference_manager::ReferenceContext},
};

use objc2_core_foundation as cf;
use objc2_core_video as cv;

mod caps;
mod decoder;
mod encoder;
mod error;
#[cfg(feature = "wgpu")]
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
        let name = sysctlbyname_string(c"machdep.cpu.brand_string").unwrap_or_else(|_| "".into());
        let device_type = if cfg!(target_arch = "aarch64") {
            DeviceType::IntegratedGpu
        } else {
            DeviceType::Other
        };
        let api_version = query_api_version();

        let decode_capabilities = caps::query_decode_capabilities();
        let encode_capabilities = caps::query_encode_capabilities();

        let supports_decoding = decode_capabilities.h264.is_some();
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
    sysctlbyname_string(c"kern.osproductversion").unwrap_or_else(|_| "???".into())
}

struct VTDevice {}

impl CoreVideoDeviceBackend for VTDevice {
    fn create_bytes_decoder_h264(
        self: Arc<Self>,
        parameters: crate::device::DecoderParameters,
    ) -> Result<crate::BytesDecoder, crate::VideoDecoderError> {
        #[cfg(feature = "wgpu")]
        let decoder = VTDecoder::new(None, parameters.usage_flags)?;
        #[cfg(not(feature = "wgpu"))]
        let decoder = VTDecoder::new(parameters.usage_flags)?;

        Ok(crate::BytesDecoder {
            decoder: Box::new(decoder),
            parser: H264Parser::new_avcc_output(),
            reference_ctx: ReferenceContext::new(parameters.missed_frame_handling),
            frame_sorter: FrameSorter::default(),
        })
    }

    fn create_bytes_encoder_h264(
        self: Arc<Self>,
        parameters: crate::device::EncoderParametersH264,
    ) -> Result<crate::BytesEncoderH264, crate::VideoEncoderError> {
        let encoder =
            VTEncoder::<H264Codec>::new(parameters.input_parameters, parameters.output_parameters)?;

        Ok(crate::BytesEncoderH264 {
            encoder: Box::new(encoder),
        })
    }

    fn create_bytes_encoder_h265(
        self: Arc<Self>,
        parameters: crate::device::EncoderParametersH265,
    ) -> Result<crate::BytesEncoderH265, crate::VideoEncoderError> {
        let encoder =
            VTEncoder::<H265Codec>::new(parameters.input_parameters, parameters.output_parameters)?;

        Ok(crate::BytesEncoderH265 {
            encoder: Box::new(encoder),
        })
    }

    #[cfg(feature = "transcoder")]
    fn create_transcoder(
        self: Arc<Self>,
        _parameters: crate::parameters::TranscoderParameters,
    ) -> Result<crate::transcoder::VideoTranscoder, crate::transcoder::VideoTranscoderError> {
        Err(crate::transcoder::VideoTranscoderError::TranscoderUnsupported)
    }

    fn decode_capabilities(&self) -> crate::capabilities::DecodeCapabilities {
        caps::query_decode_capabilities()
    }

    fn encode_capabilities(&self) -> crate::capabilities::EncodeCapabilities {
        caps::query_encode_capabilities()
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

pub(crate) trait CVBufferExt {
    unsafe fn lock(
        &self,
        flags: cv::CVPixelBufferLockFlags,
    ) -> Result<LockedCVBuffer, OSStatusError>;
}

impl CVBufferExt for cf::CFRetained<cv::CVBuffer> {
    unsafe fn lock(
        &self,
        flags: cv::CVPixelBufferLockFlags,
    ) -> Result<LockedCVBuffer, OSStatusError> {
        unsafe {
            cv::CVPixelBufferLockBaseAddress(self, flags).osstatus()?;
        }

        Ok(LockedCVBuffer {
            buffer: self.clone(),
            flags,
        })
    }
}

pub(crate) struct LockedCVBuffer {
    buffer: cf::CFRetained<cv::CVBuffer>,
    flags: cv::CVPixelBufferLockFlags,
}

impl LockedCVBuffer {
    pub(crate) fn download_nv12(&self) -> Vec<u8> {
        let planes = self.nv12_planes();
        let mut packed = Vec::with_capacity(planes.iter().map(Nv12Plane::packed_byte_size).sum());

        for plane in planes {
            for line in 0..plane.height {
                // SAFETY: the buffer stays locked for as long as `self` lives, and a row holds
                // `packed_row_bytes` initialized bytes inside the plane's allocation.
                packed.extend_from_slice(unsafe {
                    std::slice::from_raw_parts(plane.row_ptr(line), plane.packed_row_bytes)
                });
            }
        }

        packed
    }

    pub(crate) fn upload_nv12(&mut self, packed: &[u8]) {
        let mut consumed = 0;

        for plane in self.nv12_planes() {
            for line in 0..plane.height {
                let Some(row) = packed.get(consumed..consumed + plane.packed_row_bytes) else {
                    panic!("packed NV12 is smaller than the pixel buffer planes");
                };
                // SAFETY: as in `download_nv12`. `packed` is borrowed from caller memory, which
                // cannot overlap the locked planes.
                unsafe {
                    std::ptr::copy_nonoverlapping(row.as_ptr(), plane.row_ptr(line), row.len())
                };
                consumed += row.len();
            }
        }
    }

    fn nv12_planes(&self) -> [Nv12Plane; 2] {
        std::array::from_fn(|plane| Nv12Plane {
            base: cv::CVPixelBufferGetBaseAddressOfPlane(&self.buffer, plane) as *mut u8,
            packed_row_bytes: cv::CVPixelBufferGetWidthOfPlane(&self.buffer, plane)
                * if plane == 0 { 1 } else { 2 },
            stride: cv::CVPixelBufferGetBytesPerRowOfPlane(&self.buffer, plane),
            height: cv::CVPixelBufferGetHeightOfPlane(&self.buffer, plane),
        })
    }
}

struct Nv12Plane {
    base: *mut u8,
    packed_row_bytes: usize,
    stride: usize,
    height: usize,
}

impl Nv12Plane {
    fn packed_byte_size(&self) -> usize {
        self.packed_row_bytes * self.height
    }

    fn row_ptr(&self, line: usize) -> *mut u8 {
        self.base.wrapping_add(line * self.stride)
    }
}

impl Drop for LockedCVBuffer {
    fn drop(&mut self) {
        unsafe {
            if let Err(e) = cv::CVPixelBufferUnlockBaseAddress(&self.buffer, self.flags).osstatus()
            {
                tracing::warn!("error {e} while unlocking a CVBuffer");
            }
        }
    }
}

fn sysctlbyname_string(name: &std::ffi::CStr) -> Result<String, std::io::Error> {
    let mut len = 0;
    let name = name.as_ptr();
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
