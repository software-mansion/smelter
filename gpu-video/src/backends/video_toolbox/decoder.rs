use std::{
    ffi::c_void,
    ptr::{NonNull, null, null_mut},
};

use h264_reader::nal::{pps::PicParameterSet, sps::SeqParameterSet};
use objc2_core_foundation as cf;
use objc2_core_media as cm;
use objc2_core_video as cv;
use objc2_video_toolbox as vt;
use rustc_hash::FxHashMap;
use tracing::{debug, warn};

use crate::{
    RawFrameData,
    backends::video_toolbox::{
        OSStatusError, allocate_retained,
        error::{OSStatusExt, VTDecoderError},
    },
    device::{ColorRange, ColorSpace},
    frame_sorter::{DecodeResult, DecodeResultMetadata},
    parameters::DecoderUsage,
    parser::reference_manager::DecodeInformation,
};

#[cfg(feature = "wgpu")]
pub(crate) mod wgpu_api;

pub(crate) struct VTDecoder {
    session: Option<Session>,
    sps: FxHashMap<u8, Sps>,
    pps: FxHashMap<(u8, u8), Box<[u8]>>,
    needs_session_update: bool,
    session_color_range: Option<ColorRange>,
    usage: DecoderUsage,
    #[cfg_attr(not(feature = "wgpu"), expect(dead_code))]
    metal_compatible_output: bool,
}

/// What VideoToolbox reported for one submission through its output handler.
pub(super) enum Completion {
    Frame(DecodeResult<cf::CFRetained<cv::CVBuffer>>),
    Dropped,
    Failed,
}

impl VTDecoder {
    pub(super) fn new(usage: DecoderUsage, metal_compatible_output: bool) -> Self {
        Self {
            session: None,
            sps: Default::default(),
            pps: Default::default(),
            needs_session_update: false,
            session_color_range: None,
            usage,
            metal_compatible_output,
        }
    }

    pub(super) fn process_sps(&mut self, sps: SeqParameterSet, raw: Box<[u8]>) {
        let id = sps.id().id();
        self.sps.insert(id, Sps { raw, sps });
        self.needs_session_update = true;
    }

    pub(super) fn process_pps(&mut self, pps: PicParameterSet, raw: Box<[u8]>) {
        let sps_id = pps.seq_parameter_set_id.id();
        let pps_id = pps.pic_parameter_set_id.id();
        self.pps.insert((sps_id, pps_id), raw);
        self.needs_session_update = true;
    }

    /// Submits one access unit. VideoToolbox invokes `on_completion` exactly once per successful
    /// submission, possibly on another thread. If this returns an error, it is never invoked.
    pub(super) fn submit(
        &mut self,
        decode_info: DecodeInformation,
        is_idr: bool,
        flags: vt::VTDecodeFrameFlags,
        on_completion: impl Fn(Completion) + Send + 'static,
    ) -> Result<(), VTDecoderError> {
        if is_idr {
            self.ensure_session(decode_info.sps_id)?;
        }

        let sps = self.sps.get(&decode_info.sps_id).ok_or_else(|| {
            VTDecoderError::InvalidInputData(format!("Unknown SPS id {}", decode_info.sps_id))
        })?;
        let (cropped_width, cropped_height) = sps
            .sps
            .pixel_dimensions()
            .map_err(|err| VTDecoderError::InvalidInputData(format!("Invalid SPS: {err:?}")))?;

        let metadata = DecodeResultMetadata {
            pts: decode_info.pts,
            pic_order_cnt: decode_info.picture_info.PicOrderCnt_for_decoding[0],
            max_num_reorder_frames: decode_info.max_num_reorder_frames,
            is_idr,
            color_space: ColorSpace::from(&sps.sps),
            color_range: ColorRange::from(&sps.sps),
            cropped_width,
            cropped_height,
        };

        let Some(session) = self.session.as_ref() else {
            return Err(VTDecoderError::NoSession);
        };

        let buffer = session.begin_au()?;
        session.append_slice(decode_info.rbsp_bytes.into_boxed_slice(), &buffer)?;
        session.submit(buffer, metadata, flags, on_completion)
    }

    pub(super) fn wait_for_pending_frames(&self) -> Result<(), VTDecoderError> {
        let Some(session) = self.session.as_ref() else {
            return Ok(());
        };

        unsafe { session.session.wait_for_asynchronous_frames().osstatus()? };

        Ok(())
    }

    fn ensure_session(&mut self, sps_id: u8) -> Result<(), OSStatusError> {
        let color_range = self
            .sps
            .get(&sps_id)
            .map(|sps| ColorRange::from(&sps.sps))
            .unwrap_or(ColorRange::Limited);

        let range_changed = self.session_color_range != Some(color_range);

        if !self.needs_session_update && !range_changed && self.session.is_some() {
            return Ok(());
        }

        let count = self.sps.len() + self.pps.len();
        let mut parameters = Vec::with_capacity(count);
        let mut counts = Vec::with_capacity(count);

        for sps in self.sps.values() {
            let ptr = NonNull::from(&sps.raw[4]);
            parameters.push(ptr);
            counts.push(sps.raw.len() - 4);
        }

        for pps in self.pps.values() {
            let ptr = NonNull::from(&pps[4]);
            parameters.push(ptr);
            counts.push(pps.len() - 4);
        }

        let format_description = unsafe {
            allocate_retained(|ptr| {
                cm::CMVideoFormatDescriptionCreateFromH264ParameterSets(
                    None,
                    count,
                    NonNull::from(parameters.first().unwrap()),
                    NonNull::from(counts.first().unwrap()),
                    4,
                    ptr,
                )
            })?
        };

        let can_reuse_session = !range_changed
            && self.session.as_ref().is_some_and(|session| unsafe {
                session
                    .session
                    .can_accept_format_description(&format_description)
            });

        if can_reuse_session {
            self.session.as_mut().unwrap().format_description = format_description;
            self.needs_session_update = false;
            return Ok(());
        }

        let video_decoder_specification = unsafe {
            cf::CFDictionary::<cf::CFString, cf::CFType>::from_slices(
                &[vt::kVTVideoDecoderSpecification_EnableHardwareAcceleratedVideoDecoder],
                &[cf::kCFBooleanTrue.unwrap().as_ref()],
            )
        };

        let pixel_format = match color_range {
            ColorRange::Full => cv::kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
            ColorRange::Limited => cv::kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
        };

        #[cfg(feature = "wgpu")]
        let destination_image_buffer_attributes = if self.metal_compatible_output {
            unsafe {
                cf::CFDictionary::<cf::CFString, cf::CFType>::from_slices(
                    &[
                        cv::kCVPixelBufferMetalCompatibilityKey,
                        cv::kCVPixelBufferIOSurfacePropertiesKey,
                        cv::kCVPixelBufferPixelFormatTypeKey,
                    ],
                    &[
                        cf::kCFBooleanTrue.unwrap().as_ref(),
                        cf::CFDictionary::<cf::CFType, cf::CFType>::from_slices(&[], &[]).as_ref(),
                        cf::CFNumber::new_i32(pixel_format as i32).as_ref(),
                    ],
                )
            }
        } else {
            unsafe {
                cf::CFDictionary::<cf::CFString, cf::CFType>::from_slices(
                    &[cv::kCVPixelBufferPixelFormatTypeKey],
                    &[cf::CFNumber::new_i32(pixel_format as i32).as_ref()],
                )
            }
        };

        #[cfg(not(feature = "wgpu"))]
        let destination_image_buffer_attributes = unsafe {
            cf::CFDictionary::<cf::CFString, cf::CFType>::from_slices(
                &[cv::kCVPixelBufferPixelFormatTypeKey],
                &[cf::CFNumber::new_i32(pixel_format as i32).as_ref()],
            )
        };

        let session = unsafe {
            allocate_retained(|ptr| {
                vt::VTDecompressionSession::create(
                    None,
                    &format_description,
                    Some(video_decoder_specification.as_ref()),
                    Some(destination_image_buffer_attributes.as_ref()),
                    null(),
                    ptr,
                )
            })?
        };

        self.configure_session_usage(&session)?;

        self.session = Some(Session {
            session,
            format_description,
        });
        self.needs_session_update = false;
        self.session_color_range = Some(color_range);

        Ok(())
    }

    fn configure_session_usage(
        &self,
        session: &vt::VTDecompressionSession,
    ) -> Result<(), OSStatusError> {
        match self.usage {
            DecoderUsage::Default | DecoderUsage::Streaming => return Ok(()),
            DecoderUsage::Transcoding | DecoderUsage::Offline => {}
        }

        unsafe {
            let realtime = cf::kCFBooleanFalse.unwrap();
            vt::VTSessionSetProperty(
                session.as_ref(),
                vt::kVTDecompressionPropertyKey_RealTime,
                Some(realtime.as_ref()),
            )
            .osstatus()
        }
    }
}

pub(super) fn download_to_bytes(
    buffer: &cf::CFRetained<cv::CVBuffer>,
) -> Result<RawFrameData, OSStatusError> {
    let width = cv::CVPixelBufferGetWidth(buffer);
    let height = cv::CVPixelBufferGetHeight(buffer);
    let locked = unsafe { buffer.lock(cv::CVPixelBufferLockFlags::ReadOnly)? };
    let mut result = Vec::with_capacity(width * height * 3 / 2);

    for plane in 0..2usize {
        let plane_width = cv::CVPixelBufferGetWidthOfPlane(buffer, plane);
        let plane_height = cv::CVPixelBufferGetHeightOfPlane(buffer, plane);
        let stride = cv::CVPixelBufferGetBytesPerRowOfPlane(buffer, plane) as isize;
        let base_address = locked.plane_address(plane);
        let row_data_bytes = plane_width * if plane == 0 { 1 } else { 2 };
        for line in 0..plane_height as isize {
            let data = unsafe {
                std::slice::from_raw_parts(base_address.offset(line * stride), row_data_bytes)
            };
            result.extend_from_slice(data);
        }
    }

    drop(locked);

    Ok(RawFrameData {
        frame: result,
        width: width as u32,
        height: height as u32,
    })
}

struct Session {
    session: cf::CFRetained<vt::VTDecompressionSession>,
    format_description: cf::CFRetained<cm::CMFormatDescription>,
}

// Safety: Sessions are not marked in docs as thread-affine (required to be run on a specific
// thread)
unsafe impl Send for Session {}

impl Session {
    fn begin_au(&self) -> Result<cf::CFRetained<cm::CMBlockBuffer>, OSStatusError> {
        unsafe { allocate_retained(|ptr| cm::CMBlockBuffer::create_empty(None, 0, 0, ptr)) }
    }

    fn append_slice(
        &self,
        slice: Box<[u8]>,
        buffer: &cm::CMBlockBuffer,
    ) -> Result<(), OSStatusError> {
        let ptr = slice.as_ptr();
        let len = slice.len();
        let raw = Box::leak(slice);

        unsafe extern "C-unwind" fn free(_refcon: *mut c_void, ptr: NonNull<c_void>, len: usize) {
            unsafe {
                let slice = std::slice::from_raw_parts_mut(ptr.as_ptr() as *mut u8, len);
                drop(Box::from_raw(slice as *mut [u8]));
            }
        }

        let custom_source = cm::CMBlockBufferCustomBlockSource {
            version: cm::kCMBlockBufferCustomBlockSourceVersion,
            AllocateBlock: None,
            FreeBlock: Some(free),
            refCon: null_mut(),
        };

        let result = unsafe {
            buffer
                .append_memory_block(ptr as *mut _, len, None, &custom_source, 0, len, 0)
                .osstatus()
        };

        if result.is_err() {
            unsafe { drop(Box::from_raw(raw)) };
        }

        result
    }

    fn submit(
        &self,
        buffer: cf::CFRetained<cm::CMBlockBuffer>,
        metadata: DecodeResultMetadata,
        flags: vt::VTDecodeFrameFlags,
        on_completion: impl Fn(Completion) + Send + 'static,
    ) -> Result<(), VTDecoderError> {
        let len = unsafe { buffer.data_length() };
        let sample_buffer = unsafe {
            allocate_retained(|ptr| {
                cm::CMSampleBuffer::create_ready(
                    None,
                    Some(&buffer),
                    Some(&self.format_description),
                    1,
                    0,
                    null(),
                    1,
                    &len,
                    ptr,
                )
            })?
        };

        let block = block2::RcBlock::new(
            move |status: i32,
                  info_flags: vt::VTDecodeInfoFlags,
                  image: *mut cv::CVImageBuffer,
                  _pts: cm::CMTime,
                  _dur: cm::CMTime| {
                let image =
                    NonNull::new(image).map(|image| unsafe { cf::CFRetained::retain(image) });

                let completion = match (status.osstatus(), image) {
                    (Err(err), _) => {
                        debug!("VideoToolbox failed to decode a frame: {err}");
                        Completion::Failed
                    }
                    (Ok(()), Some(frame)) => Completion::Frame(DecodeResult { frame, metadata }),
                    (Ok(()), None) if info_flags.contains(vt::VTDecodeInfoFlags::FrameDropped) => {
                        Completion::Dropped
                    }
                    (Ok(()), None) => {
                        debug!(
                            "VideoToolbox returned success with no image and no FrameDropped flag"
                        );
                        Completion::Failed
                    }
                };

                on_completion(completion);
            },
        );

        unsafe {
            self.session
                .decode_frame_with_output_handler(
                    &sample_buffer,
                    flags,
                    null_mut(),
                    block2::RcBlock::as_ptr(&block),
                )
                .osstatus()?
        };

        Ok(())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        unsafe {
            if let Err(err) = self.session.wait_for_asynchronous_frames().osstatus() {
                warn!("error {err} while waiting for asynchronous frames");
            }
            self.session.invalidate()
        };
    }
}

struct Sps {
    raw: Box<[u8]>,
    sps: SeqParameterSet,
}

trait CVBufferExt {
    unsafe fn lock(
        &self,
        flags: cv::CVPixelBufferLockFlags,
    ) -> Result<LockedCvBuffer, OSStatusError>;
}

impl CVBufferExt for cf::CFRetained<cv::CVBuffer> {
    unsafe fn lock(
        &self,
        flags: cv::CVPixelBufferLockFlags,
    ) -> Result<LockedCvBuffer, OSStatusError> {
        unsafe {
            cv::CVPixelBufferLockBaseAddress(self, flags).osstatus()?;
        }

        Ok(LockedCvBuffer {
            buffer: self.clone(),
            flags,
        })
    }
}

struct LockedCvBuffer {
    buffer: cf::CFRetained<cv::CVBuffer>,
    flags: cv::CVPixelBufferLockFlags,
}

impl LockedCvBuffer {
    fn plane_address(&self, plane: usize) -> *const u8 {
        cv::CVPixelBufferGetBaseAddressOfPlane(&self.buffer, plane) as *const u8
    }
}

impl Drop for LockedCvBuffer {
    fn drop(&mut self) {
        unsafe {
            if let Err(e) = cv::CVPixelBufferUnlockBaseAddress(&self.buffer, self.flags).osstatus()
            {
                warn!("error {e} while unlocking a CVBuffer");
            }
        }
    }
}
