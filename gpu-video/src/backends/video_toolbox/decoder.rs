use std::{
    ffi::c_void,
    ptr::{NonNull, null, null_mut},
    sync::{Arc, Mutex},
};

use h264_reader::nal::{pps::PicParameterSet, sps::SeqParameterSet};
use objc2_core_foundation as cf;
use objc2_core_media as cm;
use objc2_core_video as cv;
use objc2_video_toolbox as vt;
use rustc_hash::FxHashMap;
use tracing::warn;

use crate::{
    RawFrameData,
    backends::video_toolbox::{
        OSStatusError, allocate_retained,
        error::{OSStatusExt, VTDecoderError},
    },
    decoders::VideoDecoderBackend,
    device::{ColorRange, ColorSpace},
    frame_sorter::{DecodeResult, DecodeResultMetadata},
    parameters::DecoderUsage,
    parser::{decoder_instructions::DecoderInstruction, reference_manager::DecodeInformation},
};

#[cfg(feature = "wgpu")]
pub(crate) mod wgpu_api;

pub(crate) struct VTDecoder {
    session: Option<Session>,
    sps: FxHashMap<u8, Sps>,
    pps: FxHashMap<(u8, u8), Pps>,
    needs_session_update: bool,
    #[cfg(feature = "wgpu")]
    texture_cache: Option<wgpu_api::SyncCache>,
    session_color_range: Option<ColorRange>,
    usage: DecoderUsage,
}

impl VideoDecoderBackend for VTDecoder {
    fn decode_to_bytes(
        &mut self,
        decoder_instructions: &[DecoderInstruction],
    ) -> Result<Vec<DecodeResult<RawFrameData>>, crate::VideoDecoderError> {
        let buffers = self.decode_to_cvbuffers(decoder_instructions)?;
        Ok(self.download_outputs(buffers)?)
    }
}

impl VTDecoder {
    #[cfg(not(feature = "wgpu"))]
    pub(crate) fn new(usage: DecoderUsage) -> Result<Self, VTDecoderError> {
        Ok(Self {
            session: None,
            sps: Default::default(),
            pps: Default::default(),
            needs_session_update: false,
            session_color_range: None,
            usage,
        })
    }

    fn download_outputs(
        &self,
        outputs: Vec<DecodeResult<cf::CFRetained<cv::CVBuffer>>>,
    ) -> Result<Vec<DecodeResult<RawFrameData>>, VTDecoderError> {
        outputs
            .into_iter()
            .map(|output_frame| {
                let frame = self.download_output(&output_frame.frame)?;
                Ok(DecodeResult {
                    frame,
                    metadata: output_frame.metadata,
                })
            })
            .collect()
    }

    fn decode_to_cvbuffers(
        &mut self,
        instructions: &[DecoderInstruction],
    ) -> Result<Vec<DecodeResult<cf::CFRetained<cv::CVBuffer>>>, VTDecoderError> {
        let mut results = Vec::new();

        for instruction in instructions {
            match instruction {
                DecoderInstruction::Sps { sps, raw_bytes } => self.process_sps(Sps {
                    raw: raw_bytes.clone(),
                    sps: sps.clone(),
                })?,
                DecoderInstruction::Pps { pps, raw_bytes } => self.process_pps(Pps {
                    raw: raw_bytes.clone(),
                    pps: pps.clone(),
                })?,
                DecoderInstruction::Decode { decode_info, .. } => {
                    results.extend(self.do_decode(decode_info.clone(), false)?)
                }
                DecoderInstruction::Idr { decode_info, .. } => {
                    results.extend(self.do_decode(decode_info.clone(), true)?)
                }
                DecoderInstruction::Drop { .. } => {}
            }
        }

        Ok(results)
    }

    fn do_decode(
        &mut self,
        decode_info: DecodeInformation,
        is_idr: bool,
    ) -> Result<Option<DecodeResult<cf::CFRetained<cv::CVBuffer>>>, VTDecoderError> {
        if is_idr {
            self.ensure_session(decode_info.sps_id)?;
        }

        let sps = self.sps.get(&decode_info.sps_id).ok_or_else(|| {
            VTDecoderError::InvalidInputData(format!("Unknown SPS id {}", decode_info.sps_id))
        })?;

        let metadata = DecodeResultMetadata {
            pts: decode_info.pts,
            pic_order_cnt: decode_info.picture_info.PicOrderCnt_for_decoding[0],
            max_num_reorder_frames: decode_info.max_num_reorder_frames,
            is_idr,
            color_space: ColorSpace::from(&sps.sps),
            color_range: ColorRange::from(&sps.sps),
        };

        self.upload_and_decode_au(decode_info.rbsp_bytes.into_boxed_slice(), metadata)
    }

    fn download_output(
        &self,
        buffer: &cf::CFRetained<cv::CVBuffer>,
    ) -> Result<RawFrameData, OSStatusError> {
        let width = cv::CVPixelBufferGetWidth(buffer);
        let height = cv::CVPixelBufferGetHeight(buffer);
        let locked = unsafe { buffer.lock(cv::CVPixelBufferLockFlags::ReadOnly)? };
        let mut result = Vec::with_capacity(width * height * 3 / 2);

        // NV12: plane 0 is Y (1 byte/pixel), plane 1 is CbCr (2 bytes/pixel)
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

        let data = RawFrameData {
            frame: result,
            width: width as u32,
            height: height as u32,
        };

        Ok(data)
    }

    fn upload_and_decode_au(
        &mut self,
        decode_data: Box<[u8]>,
        metadata: DecodeResultMetadata,
    ) -> Result<Option<DecodeResult<cf::CFRetained<cv::CVBuffer>>>, VTDecoderError> {
        let Some(session) = self.session.as_ref() else {
            return Err(VTDecoderError::NoSession);
        };

        let buffer = session.begin_au()?;
        session.append_slice(decode_data, &buffer)?;
        session.decode_block_buffer(buffer, metadata)
    }

    fn process_sps(&mut self, sps: Sps) -> Result<(), OSStatusError> {
        let id = sps.sps.id().id();
        self.sps.insert(id, sps);
        self.needs_session_update = true;
        Ok(())
    }

    fn process_pps(&mut self, pps: Pps) -> Result<(), OSStatusError> {
        let sps_id = pps.pps.seq_parameter_set_id.id();
        let pps_id = pps.pps.pic_parameter_set_id.id();
        self.pps.insert((sps_id, pps_id), pps);
        self.needs_session_update = true;
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
            let ptr = NonNull::from(&pps.raw[4]);
            parameters.push(ptr);
            counts.push(pps.raw.len() - 4);
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
        let destination_image_buffer_attributes = if self.output_to_wgpu_textures() {
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
            // Leave VideoToolbox on its default realtime-playback path.
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

struct CallbackOutput {
    status: i32,
    #[allow(dead_code)]
    flags: vt::VTDecodeInfoFlags,
    image: Option<cf::CFRetained<cv::CVBuffer>>,
    metadata: DecodeResultMetadata,
}

// Safety: CVBuffers are unsafe to transfer if you dont lock them or force the OS to sync them in a
// different way. If we access them on a CPU, we always lock, and if we transfer it to metal the GPU
// does the sync.
unsafe impl Send for CallbackOutput {}

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

    fn decode_block_buffer(
        &self,
        buffer: cf::CFRetained<cm::CMBlockBuffer>,
        metadata: DecodeResultMetadata,
    ) -> Result<Option<DecodeResult<cf::CFRetained<cv::CVBuffer>>>, VTDecoderError> {
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

        let slot = Arc::new(Mutex::new(None));
        let slot_clone = slot.clone();

        let block = block2::RcBlock::new(
            move |status: i32,
                  flags: vt::VTDecodeInfoFlags,
                  image: *mut cv::CVImageBuffer,
                  _pts: cm::CMTime,
                  _dur: cm::CMTime| {
                let image =
                    NonNull::new(image).map(|image| unsafe { cf::CFRetained::retain(image) });
                *slot_clone.lock().unwrap() = Some(CallbackOutput {
                    status,
                    flags,
                    image,
                    metadata,
                });
            },
        );

        unsafe {
            self.session
                .decode_frame_with_output_handler(
                    &sample_buffer,
                    vt::VTDecodeFrameFlags::empty(),
                    null_mut(),
                    block2::RcBlock::as_ptr(&block),
                )
                .osstatus()?
        };

        let Some(output) = slot.lock().unwrap().take() else {
            return Err(VTDecoderError::NoDecoderOutput);
        };

        output.status.osstatus()?;

        match output.image {
            Some(image) => Ok(Some(DecodeResult {
                frame: image,
                metadata: output.metadata,
            })),
            None if output.flags.contains(vt::VTDecodeInfoFlags::FrameDropped) => Ok(None),
            None => Err(VTDecoderError::UnexpectedNullImage),
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        unsafe { self.session.invalidate() };
    }
}

struct Sps {
    raw: Box<[u8]>,
    sps: SeqParameterSet,
}

struct Pps {
    raw: Box<[u8]>,
    pps: PicParameterSet,
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
