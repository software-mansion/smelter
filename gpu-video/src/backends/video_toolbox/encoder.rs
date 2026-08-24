use std::{
    cell::OnceCell,
    ffi::{c_int, c_void},
    ops::Deref,
    ptr::{NonNull, null, null_mut},
    sync::mpsc,
};

use objc2_core_foundation as cf;
use objc2_core_media as cm;
use objc2_core_video as cv;
use objc2_video_toolbox as vt;

use crate::{
    EncodedOutputChunk, InputFrame, RawFrameData, VideoEncoderError,
    backends::video_toolbox::{
        CVBufferExt, OSStatusError, allocate_retained, caps,
        error::{OSStatusExt, VTEncoderError},
    },
    device::{ColorRange, ColorSpace, EncoderOutputParameters, VideoParameters},
    encoders::{
        VideoEncoderBackend, VideoEncoderParametersInfoH264, VideoEncoderParametersInfoH265,
    },
    parameters::{EncoderPreset, EncoderUsage, H264Profile, H265Profile, RateControl},
};

#[cfg(feature = "wgpu")]
pub(crate) mod wgpu_api;

const ANNEX_B_START_CODE: [u8; 4] = [0, 0, 0, 1];

pub(crate) trait EncodeCodec: Send {
    type Profile: Copy + std::fmt::Debug + Send;

    /// parsed out of the session's current `CMFormatDescription`
    type StreamParameters: Send;

    const CODEC_TYPE: cm::CMVideoCodecType;

    fn profile_level(profile: Self::Profile) -> &'static cf::CFString;

    fn stream_parameters(
        format_description: &cm::CMFormatDescription,
    ) -> Result<Self::StreamParameters, OSStatusError>;

    fn keyframe_prefix(parameters: &Self::StreamParameters) -> &[u8];

    fn write_sample(
        sample: &cm::CMBlockBuffer,
        parameters: &Self::StreamParameters,
        output: &mut Vec<u8>,
    ) -> Result<(), VTEncoderError>;
}

pub(crate) struct ParameterSetInfo {
    count: usize,
    nal_length_size: usize,
}

pub(crate) trait H26xCodec: Send {
    type Profile: Copy + std::fmt::Debug + Send;
    type ParameterSetKind: Copy + PartialEq + Send + std::fmt::Display;

    const CODEC_TYPE: cm::CMVideoCodecType;

    fn profile_level(profile: Self::Profile) -> &'static cf::CFString;

    fn parameter_set_info(
        format_description: &cm::CMFormatDescription,
    ) -> Result<ParameterSetInfo, OSStatusError>;

    fn parameter_set(
        format_description: &cm::CMFormatDescription,
        index: usize,
    ) -> Result<&[u8], OSStatusError>;

    fn parameter_set_kind(nal: &[u8]) -> Option<Self::ParameterSetKind>;
}

impl<C: H26xCodec> EncodeCodec for C {
    type Profile = C::Profile;
    type StreamParameters = H26xStreamParameters<C::ParameterSetKind>;

    const CODEC_TYPE: cm::CMVideoCodecType = C::CODEC_TYPE;

    fn profile_level(profile: Self::Profile) -> &'static cf::CFString {
        C::profile_level(profile)
    }

    fn stream_parameters(
        format_description: &cm::CMFormatDescription,
    ) -> Result<Self::StreamParameters, OSStatusError> {
        H26xStreamParameters::from_description::<C>(format_description)
    }

    fn keyframe_prefix(parameters: &Self::StreamParameters) -> &[u8] {
        &parameters.configuration
    }

    fn write_sample(
        sample: &cm::CMBlockBuffer,
        parameters: &Self::StreamParameters,
        output: &mut Vec<u8>,
    ) -> Result<(), VTEncoderError> {
        write_sample_as_annex_b(sample, parameters.sample_nal_length_size, output)
    }
}

struct ParameterSetRange<Kind> {
    kind: Kind,
    start: usize,
    end: usize,
}

pub(crate) struct H26xStreamParameters<ParameterSetKind> {
    /// All parameter sets in Annex B, in the order the `CMFormatDescription` lists them.
    configuration: Vec<u8>,
    parameter_sets: Vec<ParameterSetRange<ParameterSetKind>>,
    sample_nal_length_size: usize,
}

impl<ParameterSetKind: Copy + PartialEq + std::fmt::Display>
    H26xStreamParameters<ParameterSetKind>
{
    fn from_description<C: H26xCodec<ParameterSetKind = ParameterSetKind>>(
        format_description: &cm::CMFormatDescription,
    ) -> Result<Self, OSStatusError> {
        let info = C::parameter_set_info(format_description)?;
        let mut state = Self {
            configuration: Vec::new(),
            parameter_sets: Vec::new(),
            sample_nal_length_size: info.nal_length_size,
        };

        for index in 0..info.count {
            state.push_parameter_set::<C>(C::parameter_set(format_description, index)?);
        }

        Ok(state)
    }

    fn push_parameter_set<C: H26xCodec<ParameterSetKind = ParameterSetKind>>(
        &mut self,
        nal: &[u8],
    ) {
        if let Some(kind) = C::parameter_set_kind(nal) {
            self.configuration.extend_from_slice(&ANNEX_B_START_CODE);
            self.configuration.extend_from_slice(nal);

            let start = self.configuration.len() - nal.len() - 4;
            let end = self.configuration.len();
            self.parameter_sets
                .push(ParameterSetRange { kind, start, end });
        }
    }

    fn parameter_sets_of(&self, wanted: ParameterSetKind) -> Result<Vec<u8>, VTEncoderError> {
        let mut output = Vec::new();
        for range in &self.parameter_sets {
            if range.kind == wanted {
                output.extend_from_slice(&self.configuration[range.start..range.end]);
            }
        }

        if output.is_empty() {
            return Err(VTEncoderError::MissingParameterSet(wanted.to_string()));
        }

        Ok(output)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum H264ParameterSet {
    Sps,
    Pps,
}

impl std::fmt::Display for H264ParameterSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sps => f.write_str("SPS"),
            Self::Pps => f.write_str("PPS"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum H265ParameterSet {
    Vps,
    Sps,
    Pps,
}

impl std::fmt::Display for H265ParameterSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Vps => f.write_str("VPS"),
            Self::Sps => f.write_str("SPS"),
            Self::Pps => f.write_str("PPS"),
        }
    }
}

pub(crate) struct H264Codec;

impl H26xCodec for H264Codec {
    type Profile = H264Profile;
    type ParameterSetKind = H264ParameterSet;

    const CODEC_TYPE: cm::CMVideoCodecType = cm::kCMVideoCodecType_H264;

    fn profile_level(profile: H264Profile) -> &'static cf::CFString {
        unsafe {
            match profile {
                H264Profile::Baseline => vt::kVTProfileLevel_H264_Baseline_AutoLevel,
                H264Profile::Main => vt::kVTProfileLevel_H264_Main_AutoLevel,
                H264Profile::High => vt::kVTProfileLevel_H264_High_AutoLevel,
            }
        }
    }

    fn parameter_set_info(
        format_description: &cm::CMFormatDescription,
    ) -> Result<ParameterSetInfo, OSStatusError> {
        query_parameter_set_info(
            cm::CMVideoFormatDescriptionGetH264ParameterSetAtIndex,
            format_description,
        )
    }

    fn parameter_set(
        format_description: &cm::CMFormatDescription,
        index: usize,
    ) -> Result<&[u8], OSStatusError> {
        query_parameter_set(
            cm::CMVideoFormatDescriptionGetH264ParameterSetAtIndex,
            format_description,
            index,
        )
    }

    fn parameter_set_kind(nal: &[u8]) -> Option<H264ParameterSet> {
        match nal.first()? & 0x1f {
            7 => Some(H264ParameterSet::Sps),
            8 => Some(H264ParameterSet::Pps),
            _ => None,
        }
    }
}

pub(crate) struct H265Codec;

impl H26xCodec for H265Codec {
    type Profile = H265Profile;
    type ParameterSetKind = H265ParameterSet;

    const CODEC_TYPE: cm::CMVideoCodecType = cm::kCMVideoCodecType_HEVC;

    fn profile_level(profile: H265Profile) -> &'static cf::CFString {
        unsafe {
            match profile {
                H265Profile::Main => vt::kVTProfileLevel_HEVC_Main_AutoLevel,
            }
        }
    }

    fn parameter_set_info(
        format_description: &cm::CMFormatDescription,
    ) -> Result<ParameterSetInfo, OSStatusError> {
        query_parameter_set_info(
            cm::CMVideoFormatDescriptionGetHEVCParameterSetAtIndex,
            format_description,
        )
    }

    fn parameter_set(
        format_description: &cm::CMFormatDescription,
        index: usize,
    ) -> Result<&[u8], OSStatusError> {
        query_parameter_set(
            cm::CMVideoFormatDescriptionGetHEVCParameterSetAtIndex,
            format_description,
            index,
        )
    }

    fn parameter_set_kind(nal: &[u8]) -> Option<H265ParameterSet> {
        match (nal.first()? >> 1) & 0x3f {
            32 => Some(H265ParameterSet::Vps),
            33 => Some(H265ParameterSet::Sps),
            34 => Some(H265ParameterSet::Pps),
            _ => None,
        }
    }
}

type CMGetParameterSetAtIndexFn = unsafe extern "C-unwind" fn(
    &cm::CMFormatDescription,
    usize,
    *mut *const u8,
    *mut usize,
    *mut usize,
    *mut c_int,
) -> i32;

fn query_parameter_set_info(
    getter: CMGetParameterSetAtIndexFn,
    format_description: &cm::CMFormatDescription,
) -> Result<ParameterSetInfo, OSStatusError> {
    let mut count = 0;
    let mut nal_length_size: c_int = 0;

    unsafe {
        getter(
            format_description,
            0,
            null_mut(),
            null_mut(),
            &mut count,
            &mut nal_length_size,
        )
        .osstatus()?;
    }

    Ok(ParameterSetInfo {
        count,
        nal_length_size: nal_length_size as usize,
    })
}

fn query_parameter_set(
    getter: CMGetParameterSetAtIndexFn,
    format_description: &cm::CMFormatDescription,
    index: usize,
) -> Result<&[u8], OSStatusError> {
    let mut nal = null();
    let mut size = 0;

    unsafe {
        getter(
            format_description,
            index,
            &mut nal,
            &mut size,
            null_mut(),
            null_mut(),
        )
        .osstatus()?;

        // SAFETY: the parameter set bytes are owned by the format description, which the returned
        // slice borrows from.
        Ok(std::slice::from_raw_parts(nal, size))
    }
}

pub(crate) struct VTEncoder<C: EncodeCodec> {
    session: Session,
    session_generation: u64,
    stream_format: Option<StreamFormat<C>>,
    prefetched_stream_format: OnceCell<StreamFormat<C>>,
    frame_index: i64,
    pts_step: i64,
    time_scale: i32,
    picture_byte_size: usize,
    inline_stream_params: bool,
    /// Fatal with `!inline_stream_params`.
    parameters_changed_mid_stream: bool,
    // Retained so a session invalidated mid-stream can be rebuilt identically.
    input_parameters: VideoParameters,
    output_parameters: EncoderOutputParameters<C::Profile>,
    #[cfg(feature = "wgpu")]
    wgpu: Option<wgpu_api::VTWgpuEncodeState>,
}

impl<C: EncodeCodec> VTEncoder<C> {
    pub(crate) fn new(
        input_parameters: VideoParameters,
        output_parameters: EncoderOutputParameters<C::Profile>,
    ) -> Result<Self, VideoEncoderError> {
        Ok(Self::create(input_parameters, output_parameters, false)?)
    }

    pub(crate) fn create(
        input_parameters: VideoParameters,
        output_parameters: EncoderOutputParameters<C::Profile>,
        metal_compatible_input: bool,
    ) -> Result<Self, VTEncoderError> {
        let time_scale = i32::try_from(input_parameters.target_framerate.numerator)
            .ok()
            .filter(|numerator| *numerator > 0)
            .ok_or_else(|| VTEncoderError::Parameters {
                field: "input_parameters.target_framerate",
                problem: format!(
                    "the numerator ({}) has to fit in 1..=i32::MAX",
                    input_parameters.target_framerate.numerator
                ),
            })?;
        let pts_step = i64::from(input_parameters.target_framerate.denominator.get());

        let session =
            Self::build_session(input_parameters, &output_parameters, metal_compatible_input)?;

        let width = input_parameters.width.get() as usize;
        let height = input_parameters.height.get() as usize;

        Ok(Self {
            session,
            session_generation: 0,
            stream_format: None,
            prefetched_stream_format: OnceCell::new(),
            frame_index: 0,
            pts_step,
            time_scale,
            picture_byte_size: width * height * 3 / 2,
            inline_stream_params: output_parameters.inline_stream_params.unwrap_or(true),
            parameters_changed_mid_stream: false,
            input_parameters,
            output_parameters,
            #[cfg(feature = "wgpu")]
            wgpu: None,
        })
    }

    fn build_session(
        input_parameters: VideoParameters,
        output_parameters: &EncoderOutputParameters<C::Profile>,
        metal_compatible_input: bool,
    ) -> Result<Session, VTEncoderError> {
        let width = dimension_to_i32(input_parameters.width.get(), "input_parameters.width")?;
        let height = dimension_to_i32(input_parameters.height.get(), "input_parameters.height")?;

        fn require_even_for_420_subsampling(
            value: i32,
            field: &'static str,
        ) -> Result<(), VTEncoderError> {
            if value % 2 != 0 {
                return Err(VTEncoderError::Parameters {
                    field,
                    problem: format!("{value} has to be even for 4:2:0 chroma subsampling"),
                });
            }
            Ok(())
        }

        require_even_for_420_subsampling(width, "input_parameters.width")?;
        require_even_for_420_subsampling(height, "input_parameters.height")?;

        let pixel_format = match output_parameters.color_range.unwrap_or(ColorRange::Limited) {
            ColorRange::Full => cv::kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
            ColorRange::Limited => cv::kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
        };

        let encoder_specification = unsafe {
            cf::CFDictionary::<cf::CFString, cf::CFType>::from_slices(
                &[vt::kVTVideoEncoderSpecification_RequireHardwareAcceleratedVideoEncoder],
                &[cf::CFBoolean::new(true).as_ref()],
            )
        };

        let pixel_format_number = cf::CFNumber::new_i32(pixel_format as i32);
        let io_surface_properties =
            cf::CFDictionary::<cf::CFType, cf::CFType>::from_slices(&[], &[]);
        let source_image_buffer_attributes = if metal_compatible_input {
            unsafe {
                cf::CFDictionary::<cf::CFString, cf::CFType>::from_slices(
                    &[
                        cv::kCVPixelBufferPixelFormatTypeKey,
                        cv::kCVPixelBufferMetalCompatibilityKey,
                        cv::kCVPixelBufferIOSurfacePropertiesKey,
                    ],
                    &[
                        pixel_format_number.as_ref(),
                        cf::CFBoolean::new(true).as_ref(),
                        io_surface_properties.as_ref(),
                    ],
                )
            }
        } else {
            unsafe {
                cf::CFDictionary::<cf::CFString, cf::CFType>::from_slices(
                    &[cv::kCVPixelBufferPixelFormatTypeKey],
                    &[pixel_format_number.as_ref()],
                )
            }
        };

        let attempt = |apply_preset_base: bool| -> Result<Session, VTEncoderError> {
            let session = SessionGuard(
                unsafe {
                    allocate_retained(|ptr| {
                        vt::VTCompressionSession::create(
                            None,
                            width,
                            height,
                            C::CODEC_TYPE,
                            Some(encoder_specification.as_ref()),
                            Some(source_image_buffer_attributes.as_ref()),
                            None,
                            None,
                            null_mut(),
                            ptr,
                        )
                    })
                }
                .map_err(|err| match err {
                    OSStatusError::VTCouldNotFindVideoEncoder => VTEncoderError::NoHardwareEncoder,
                    err => err.into(),
                })?,
            );

            configure_session::<C>(
                &session,
                input_parameters,
                output_parameters,
                apply_preset_base,
            )?;

            unsafe { session.prepare_to_encode_frames().osstatus()? };

            let input_pool =
                unsafe { session.pixel_buffer_pool() }.ok_or(OSStatusError::VTAllocationFailed)?;

            Ok(Session {
                session,
                input_pool,
            })
        };

        // Presets set private rate-control keys that clash with explicit rate control, so we only
        // try them with `EncoderDefault` rate control and fall back to a preset-less build.
        if matches!(output_parameters.rate_control, RateControl::EncoderDefault) {
            match attempt(true) {
                Ok(session) => Ok(session),
                Err(VTEncoderError::NoHardwareEncoder) => Err(VTEncoderError::NoHardwareEncoder),
                Err(with_preset) => {
                    tracing::debug!(
                        "encoder setup failed with the preset base configuration ({with_preset}); rebuilding without it"
                    );
                    attempt(false)
                }
            }
        } else {
            attempt(false)
        }
    }

    fn next_frame_timing(&mut self) -> (cm::CMTime, cm::CMTime) {
        let presentation_time_stamp =
            unsafe { cm::CMTime::new(self.frame_index * self.pts_step, self.time_scale) };
        self.frame_index += 1;
        (presentation_time_stamp, self.frame_duration())
    }

    fn frame_duration(&self) -> cm::CMTime {
        unsafe { cm::CMTime::new(self.pts_step, self.time_scale) }
    }

    fn frame_properties(
        force_idr: bool,
    ) -> Option<cf::CFRetained<cf::CFDictionary<cf::CFString, cf::CFType>>> {
        force_idr.then(|| unsafe {
            cf::CFDictionary::<cf::CFString, cf::CFType>::from_slices(
                &[vt::kVTEncodeFrameOptionKey_ForceKeyFrame],
                &[cf::CFBoolean::new(true).as_ref()],
            )
        })
    }

    pub(crate) fn encode_pixel_buffer(
        &mut self,
        buffer: &cv::CVBuffer,
        pts: Option<u64>,
        force_idr: bool,
    ) -> Result<EncodedOutputChunk<Vec<u8>>, VTEncoderError> {
        if self.parameters_changed_mid_stream && !self.inline_stream_params {
            return Err(VTEncoderError::ParametersDiverged);
        }

        let (presentation_time_stamp, duration) = self.next_frame_timing();

        let frame_properties = Self::frame_properties(force_idr);
        let frame_properties = frame_properties
            .as_ref()
            .map(|properties| properties.as_ref());

        let generation = self.session_generation;
        match self.try_encode(
            buffer,
            presentation_time_stamp,
            duration,
            frame_properties,
            pts,
        ) {
            Err(VTEncoderError::OSStatus(OSStatusError::VTInvalidSession)) => self
                .recover_from_invalidated_session(
                    generation,
                    buffer,
                    presentation_time_stamp,
                    duration,
                    frame_properties,
                    pts,
                ),
            result => result,
        }
    }

    fn try_encode(
        &mut self,
        buffer: &cv::CVBuffer,
        presentation_time_stamp: cm::CMTime,
        duration: cm::CMTime,
        frame_properties: Option<&cf::CFDictionary>,
        pts: Option<u64>,
    ) -> Result<EncodedOutputChunk<Vec<u8>>, VTEncoderError> {
        let sample = self.session.encode_blocking(
            buffer,
            presentation_time_stamp,
            duration,
            frame_properties,
        )?;
        self.collect_output(&sample, pts)
    }

    #[cfg(feature = "wgpu")]
    fn metal_compatible_input(&self) -> bool {
        self.wgpu.is_some()
    }

    #[cfg(not(feature = "wgpu"))]
    fn metal_compatible_input(&self) -> bool {
        false
    }

    fn recover_from_invalidated_session(
        &mut self,
        generation: u64,
        buffer: &cv::CVBuffer,
        presentation_time_stamp: cm::CMTime,
        duration: cm::CMTime,
        frame_properties: Option<&cf::CFDictionary>,
        pts: Option<u64>,
    ) -> Result<EncodedOutputChunk<Vec<u8>>, VTEncoderError> {
        let previous_configuration = if generation == self.session_generation {
            // Happens on media services resets, sleep/wake or GPU changes.
            tracing::warn!(
                "VideoToolbox invalidated the compression session; rebuilding it and retrying the frame"
            );

            let previous = self
                .stream_format
                .as_ref()
                .map(|stream_format| C::keyframe_prefix(&stream_format.parameters).to_vec());

            self.session = Self::build_session(
                self.input_parameters,
                &self.output_parameters,
                self.metal_compatible_input(),
            )
            .map_err(|source| VTEncoderError::SessionInvalidated(Box::new(source)))?;
            self.session_generation += 1;

            previous
        } else {
            None
        };

        let chunk = self
            .try_encode(
                buffer,
                presentation_time_stamp,
                duration,
                frame_properties,
                pts,
            )
            .map_err(|source| VTEncoderError::SessionInvalidated(Box::new(source)))?;

        if let Some(previous) = previous_configuration {
            if C::keyframe_prefix(&self.stream_format()?.parameters) != previous.as_slice() {
                self.parameters_changed_mid_stream = true;
                if self.inline_stream_params {
                    tracing::warn!(
                        "stream parameters changed after rebuilding the compression session"
                    );
                } else {
                    return Err(VTEncoderError::ParametersDiverged);
                }
            }
        }

        Ok(chunk)
    }

    fn collect_output(
        &mut self,
        sample: &cm::CMSampleBuffer,
        pts: Option<u64>,
    ) -> Result<EncodedOutputChunk<Vec<u8>>, VTEncoderError> {
        if let Some(format_description) = unsafe { sample.format_description() } {
            self.update_stream_format(format_description)?;
        }

        let is_keyframe = is_keyframe(sample);
        let stream_format = self.stream_format()?;

        let mut data = if is_keyframe && self.inline_stream_params {
            C::keyframe_prefix(&stream_format.parameters).to_vec()
        } else {
            Vec::new()
        };

        let block_buffer = unsafe { sample.data_buffer() }.ok_or(VTEncoderError::NoDataBuffer)?;

        C::write_sample(&block_buffer, &stream_format.parameters, &mut data)?;

        Ok(EncodedOutputChunk {
            data,
            pts,
            is_keyframe,
        })
    }

    fn update_stream_format(
        &mut self,
        description: cf::CFRetained<cm::CMFormatDescription>,
    ) -> Result<(), VTEncoderError> {
        let current = self
            .stream_format
            .as_ref()
            .map(|stream_format| &*stream_format.description);
        if unsafe { cm::CMFormatDescription::equal(current, Some(&description)) } {
            return Ok(());
        }

        let stream_format = StreamFormat {
            parameters: C::stream_parameters(&description)?,
            description,
        };

        let diverged_from_prefetched = self.stream_format.is_none()
            && self
                .prefetched_stream_format
                .get()
                .is_some_and(|prefetched| {
                    C::keyframe_prefix(&prefetched.parameters)
                        != C::keyframe_prefix(&stream_format.parameters)
                });

        self.stream_format = Some(stream_format);

        if diverged_from_prefetched {
            self.parameters_changed_mid_stream = true;
            if self.inline_stream_params {
                tracing::warn!("stream parameters differ from the prefetched ones");
            } else {
                return Err(VTEncoderError::ParametersDiverged);
            }
        }

        Ok(())
    }

    fn stream_format(&self) -> Result<&StreamFormat<C>, VTEncoderError> {
        if let Some(stream_format) = &self.stream_format {
            return Ok(stream_format);
        }
        if let Some(stream_format) = self.prefetched_stream_format.get() {
            return Ok(stream_format);
        }

        let stream_format = self.prefetch_stream_format()?;
        Ok(self.prefetched_stream_format.get_or_init(|| stream_format))
    }

    fn prefetch_stream_format(&self) -> Result<StreamFormat<C>, VTEncoderError> {
        tracing::debug!(
            "encoding a dummy frame on a throwaway session to extract the stream parameters"
        );

        let session = Self::build_session(
            self.input_parameters,
            &self.output_parameters,
            self.metal_compatible_input(),
        )?;

        let buffer = self.session.acquire_input_buffer()?;

        let sample = session.encode_blocking(
            &buffer,
            unsafe { cm::CMTime::new(0, self.time_scale) },
            self.frame_duration(),
            None,
        )?;
        let description =
            unsafe { sample.format_description() }.ok_or(VTEncoderError::NoFormatDescription)?;

        Ok(StreamFormat {
            parameters: C::stream_parameters(&description)?,
            description,
        })
    }
}

impl<C: EncodeCodec> VideoEncoderBackend for VTEncoder<C> {
    fn encode_bytes(
        &mut self,
        frame: &InputFrame<RawFrameData>,
        force_idr: bool,
    ) -> Result<EncodedOutputChunk<Vec<u8>>, VideoEncoderError> {
        if frame.data.frame.len() != self.picture_byte_size {
            return Err(VideoEncoderError::InconsistentPictureByteSize {
                bytes: frame.data.frame.len(),
                size_from_resolution: self.picture_byte_size,
            });
        }

        let buffer = self.session.input_buffer_from_nv12(&frame.data.frame)?;

        Ok(self.encode_pixel_buffer(&buffer, frame.pts, force_idr)?)
    }
}

impl<C: H26xCodec> VTEncoder<C> {
    fn parameter_set(&self, wanted: C::ParameterSetKind) -> Result<Vec<u8>, VideoEncoderError> {
        Ok(self.stream_format()?.parameters.parameter_sets_of(wanted)?)
    }
}

impl VideoEncoderParametersInfoH264 for VTEncoder<H264Codec> {
    fn sps(&self) -> Result<Vec<u8>, VideoEncoderError> {
        self.parameter_set(H264ParameterSet::Sps)
    }

    fn pps(&self) -> Result<Vec<u8>, VideoEncoderError> {
        self.parameter_set(H264ParameterSet::Pps)
    }
}

impl VideoEncoderParametersInfoH265 for VTEncoder<H265Codec> {
    fn vps(&self) -> Result<Vec<u8>, VideoEncoderError> {
        self.parameter_set(H265ParameterSet::Vps)
    }

    fn sps(&self) -> Result<Vec<u8>, VideoEncoderError> {
        self.parameter_set(H265ParameterSet::Sps)
    }

    fn pps(&self) -> Result<Vec<u8>, VideoEncoderError> {
        self.parameter_set(H265ParameterSet::Pps)
    }
}

struct CallbackOutput {
    status: i32,
    flags: vt::VTEncodeInfoFlags,
    sample: Option<cf::CFRetained<cm::CMSampleBuffer>>,
}

// Safety: the sample buffer only carries encoded bytes, there is no GPU resource left to
// synchronize by the time the output handler runs.
unsafe impl Send for CallbackOutput {}

/// VT calls the returned block from an internal thread once the frame is emitted.
fn output_handler(
    sender: mpsc::Sender<CallbackOutput>,
) -> block2::RcBlock<dyn Fn(i32, vt::VTEncodeInfoFlags, *mut cm::CMSampleBuffer)> {
    block2::RcBlock::new(
        move |status: i32, flags: vt::VTEncodeInfoFlags, sample: *mut cm::CMSampleBuffer| {
            let sample =
                NonNull::new(sample).map(|sample| unsafe { cf::CFRetained::retain(sample) });
            let _ = sender.send(CallbackOutput {
                status,
                flags,
                sample,
            });
        },
    )
}

fn check_output_status(
    output: CallbackOutput,
) -> Result<cf::CFRetained<cm::CMSampleBuffer>, VTEncoderError> {
    output.status.osstatus()?;

    if output.flags.contains(vt::VTEncodeInfoFlags::FrameDropped) {
        return Err(VTEncoderError::FrameDropped);
    }

    output.sample.ok_or(VTEncoderError::NoEncoderOutput)
}

struct SessionGuard(cf::CFRetained<vt::VTCompressionSession>);

impl Deref for SessionGuard {
    type Target = vt::VTCompressionSession;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Drop for SessionGuard {
    fn drop(&mut self) {
        unsafe { self.0.invalidate() };
    }
}

struct Session {
    session: SessionGuard,
    input_pool: cf::CFRetained<cv::CVPixelBufferPool>,
}

struct StreamFormat<C: EncodeCodec> {
    description: cf::CFRetained<cm::CMFormatDescription>,
    parameters: C::StreamParameters,
}

// Safety: Sessions are not marked in docs as thread-affine
unsafe impl Send for Session {}

// Safety: CMFormatDescriptions are immutable and CF retain counts are thread-safe.
unsafe impl<C: EncodeCodec> Send for StreamFormat<C> {}

impl Session {
    fn acquire_input_buffer(&self) -> Result<cf::CFRetained<cv::CVBuffer>, VTEncoderError> {
        Ok(unsafe {
            allocate_retained(|ptr| {
                cv::CVPixelBufferPool::create_pixel_buffer(None, &self.input_pool, ptr)
            })?
        })
    }

    fn input_buffer_from_nv12(
        &self,
        packed: &[u8],
    ) -> Result<cf::CFRetained<cv::CVBuffer>, VTEncoderError> {
        let buffer = self.acquire_input_buffer()?;

        let mut locked = unsafe { buffer.lock(cv::CVPixelBufferLockFlags::empty())? };
        locked.upload_nv12(packed);
        drop(locked);

        Ok(buffer)
    }

    fn encode_blocking(
        &self,
        buffer: &cv::CVBuffer,
        presentation_time_stamp: cm::CMTime,
        duration: cm::CMTime,
        frame_properties: Option<&cf::CFDictionary>,
    ) -> Result<cf::CFRetained<cm::CMSampleBuffer>, VTEncoderError> {
        let (sender, receiver) = mpsc::channel();
        let block = output_handler(sender);

        unsafe {
            self.session
                .encode_frame_with_output_handler(
                    buffer,
                    presentation_time_stamp,
                    duration,
                    frame_properties,
                    null_mut(),
                    block2::RcBlock::as_ptr(&block),
                )
                .osstatus()?;

            // for blocking encoding
            self.session
                .complete_frames(cm::kCMTimeInvalid)
                .osstatus()?;
        }

        let output = receiver
            .try_recv()
            .map_err(|_| VTEncoderError::NoEncoderOutput)?;
        check_output_status(output)
    }
}

fn configure_session<C: EncodeCodec>(
    session: &vt::VTCompressionSession,
    input_parameters: VideoParameters,
    output_parameters: &EncoderOutputParameters<C::Profile>,
    apply_preset_base: bool,
) -> Result<(), VTEncoderError> {
    // the explicit sets below override it
    if apply_preset_base {
        apply_preset(session, output_parameters.preset);
    }

    let set_required = |property: &'static str, key: &cf::CFString, value: &cf::CFType| {
        unsafe { vt::VTSessionSetProperty(session.as_ref(), key, Some(value)) }
            .osstatus()
            .map_err(|error| VTEncoderError::PropertySet { property, error })
    };

    let set_optional = |property: &'static str, key: &cf::CFString, value: &cf::CFType| {
        if let Err(error) =
            unsafe { vt::VTSessionSetProperty(session.as_ref(), key, Some(value)) }.osstatus()
        {
            tracing::warn!("VideoToolbox rejected optional encoder property {property}: {error}");
        }
    };

    set_required(
        "ProfileLevel",
        unsafe { vt::kVTCompressionPropertyKey_ProfileLevel },
        C::profile_level(output_parameters.profile).as_ref(),
    )?;

    // for blocking encoding
    set_optional(
        "AllowFrameReordering",
        unsafe { vt::kVTCompressionPropertyKey_AllowFrameReordering },
        cf::CFBoolean::new(false).as_ref(),
    );

    if let Some(realtime) = realtime_hint(output_parameters.preset, output_parameters.usage_flags) {
        set_optional(
            "RealTime",
            unsafe { vt::kVTCompressionPropertyKey_RealTime },
            cf::CFBoolean::new(realtime).as_ref(),
        );
    }

    if output_parameters.max_references.is_some() {
        tracing::debug!(
            "VideoToolbox manages reference frames internally; ignoring max_references"
        );
    }

    if let Some(idr_period) = output_parameters.idr_period {
        set_required(
            "MaxKeyFrameInterval",
            unsafe { vt::kVTCompressionPropertyKey_MaxKeyFrameInterval },
            cf::CFNumber::new_i64(i64::from(idr_period.get())).as_ref(),
        )?;
    }

    let framerate = f64::from(input_parameters.target_framerate.numerator)
        / f64::from(input_parameters.target_framerate.denominator.get());
    set_optional(
        "ExpectedFrameRate",
        unsafe { vt::kVTCompressionPropertyKey_ExpectedFrameRate },
        cf::CFNumber::new_f64(framerate).as_ref(),
    );

    configure_rate_control(session, &set_required, &set_optional, output_parameters)?;
    configure_color(
        &set_optional,
        output_parameters.color_space.unwrap_or_default(),
    );

    Ok(())
}

fn configure_rate_control<P>(
    session: &vt::VTCompressionSession,
    set_required: &impl Fn(&'static str, &cf::CFString, &cf::CFType) -> Result<(), VTEncoderError>,
    set_optional: &impl Fn(&'static str, &cf::CFString, &cf::CFType),
    output_parameters: &EncoderOutputParameters<P>,
) -> Result<(), VTEncoderError> {
    match output_parameters.rate_control {
        RateControl::EncoderDefault => {}

        RateControl::VariableBitrate {
            average_bitrate,
            max_bitrate,
            virtual_buffer_size,
        } => {
            if virtual_buffer_size.is_zero() {
                return Err(VTEncoderError::Parameters {
                    field: "rate_control.virtual_buffer_size",
                    problem: "must be non-zero for variable bitrate rate control".to_string(),
                });
            }

            set_required(
                "AverageBitRate",
                unsafe { vt::kVTCompressionPropertyKey_AverageBitRate },
                cf::CFNumber::new_i64(average_bitrate as i64).as_ref(),
            )
            .map_err(|error| VTEncoderError::Parameters {
                field: "rate_control.average_bitrate",
                problem: error.to_string(),
            })?;

            // DataRateLimits is expressed as "at most N bytes per W seconds".
            let window_seconds = virtual_buffer_size.as_secs_f64();
            let max_bytes = (max_bitrate as f64 / 8.0) * window_seconds;
            let limits = cf::CFArray::<cf::CFType>::from_objects(&[
                cf::CFNumber::new_i64(max_bytes as i64).as_ref(),
                cf::CFNumber::new_f64(window_seconds).as_ref(),
            ]);
            // only a refinement of the average bitrate target
            set_optional(
                "DataRateLimits",
                unsafe { vt::kVTCompressionPropertyKey_DataRateLimits },
                limits.as_ref(),
            );
        }

        RateControl::ConstantBitrate { bitrate, .. } => {
            let supported = supported_property_dictionary(session)?;
            if !caps::supports_property(&supported, unsafe {
                vt::kVTCompressionPropertyKey_ConstantBitRate
            }) {
                return Err(VTEncoderError::ConstantBitrateUnsupported);
            }

            // VideoToolbox exposes no window length for CBR, it always picks its own.
            set_required(
                "ConstantBitRate",
                unsafe { vt::kVTCompressionPropertyKey_ConstantBitRate },
                cf::CFNumber::new_i64(bitrate as i64).as_ref(),
            )
            .map_err(|error| VTEncoderError::Parameters {
                field: "rate_control.bitrate",
                problem: error.to_string(),
            })?;
        }

        RateControl::Disabled => {
            let supported = supported_property_dictionary(session)?;
            if caps::supports_property(&supported, unsafe { vt::kVTCompressionPropertyKey_Quality })
            {
                set_required(
                    "Quality",
                    unsafe { vt::kVTCompressionPropertyKey_Quality },
                    cf::CFNumber::new_f64(quality_for(output_parameters.preset)).as_ref(),
                )?;
            } else {
                tracing::warn!(
                    "This encoder does not support the Quality property; leaving rate control at the encoder default"
                );
            }
        }
    }

    Ok(())
}

fn configure_color(
    set_optional: &impl Fn(&'static str, &cf::CFString, &cf::CFType),
    color_space: ColorSpace,
) {
    let (primaries, transfer, matrix) = unsafe {
        match color_space {
            ColorSpace::Unspecified => return,
            ColorSpace::BT709 => (
                cv::kCVImageBufferColorPrimaries_ITU_R_709_2,
                cv::kCVImageBufferTransferFunction_ITU_R_709_2,
                cv::kCVImageBufferYCbCrMatrix_ITU_R_709_2,
            ),
            ColorSpace::BT601Ntsc => (
                cv::kCVImageBufferColorPrimaries_SMPTE_C,
                cv::kCVImageBufferTransferFunction_ITU_R_709_2,
                cv::kCVImageBufferYCbCrMatrix_ITU_R_601_4,
            ),
            ColorSpace::BT601Pal => (
                cv::kCVImageBufferColorPrimaries_EBU_3213,
                cv::kCVImageBufferTransferFunction_ITU_R_709_2,
                cv::kCVImageBufferYCbCrMatrix_ITU_R_601_4,
            ),
        }
    };

    set_optional(
        "ColorPrimaries",
        unsafe { vt::kVTCompressionPropertyKey_ColorPrimaries },
        primaries.as_ref(),
    );
    set_optional(
        "TransferFunction",
        unsafe { vt::kVTCompressionPropertyKey_TransferFunction },
        transfer.as_ref(),
    );
    set_optional(
        "YCbCrMatrix",
        unsafe { vt::kVTCompressionPropertyKey_YCbCrMatrix },
        matrix.as_ref(),
    );
}

fn supported_property_dictionary(
    session: &vt::VTCompressionSession,
) -> Result<cf::CFRetained<cf::CFDictionary>, OSStatusError> {
    allocate_retained(|ptr| unsafe {
        vt::VTSessionCopySupportedPropertyDictionary(session.as_ref(), ptr)
    })
}

/// Looks up an extern CFString in runtime instead of failing to compile if it's not present
fn cfstring_constant(symbol: &std::ffi::CStr) -> Option<&'static cf::CFString> {
    // SAFETY: RTLD_DEFAULT searches loaded images; VideoToolbox is strongly linked by this
    // module, so it is always loaded. The symbol, when present, is a global holding a valid,
    // immortal CFStringRef.
    unsafe {
        let slot = libc::dlsym(libc::RTLD_DEFAULT, symbol.as_ptr());
        (slot as *const *const cf::CFString)
            .as_ref()
            .and_then(|&ptr| ptr.as_ref())
    }
}

fn apply_preset(session: &vt::VTCompressionSession, preset: EncoderPreset) {
    let (name, symbol) = match preset {
        EncoderPreset::HighQuality => ("HighQuality", c"kVTCompressionPreset_HighQuality"),
        EncoderPreset::Balanced => ("Balanced", c"kVTCompressionPreset_Balanced"),
        EncoderPreset::LowLatency => ("HighSpeed", c"kVTCompressionPreset_HighSpeed"),
    };

    let Some(key) = cfstring_constant(symbol) else {
        // compression presets were added in macOS 26
        tracing::debug!(
            "VideoToolbox has no compression presets on this macOS version; skipping the {name} preset base configuration"
        );
        return;
    };

    let supported = match preset_dictionaries(session) {
        Ok(dictionaries) => dictionaries,
        Err(error) => {
            tracing::debug!(
                "VideoToolbox does not expose preset dictionaries ({error}); skipping the {name} preset base configuration"
            );
            return;
        }
    };

    // SAFETY: `key` is a valid `CFString` pointer for the lifetime of the call.
    let value = unsafe { supported.value((key as *const cf::CFString).cast()) };
    let Some(value) = NonNull::new(value.cast_mut()) else {
        tracing::debug!(
            "VideoToolbox has no {name} preset dictionary; skipping its base configuration"
        );
        return;
    };

    // SAFETY: `SupportedPresetDictionaries` maps preset-name `CFString`s to `CFDictionary`s of
    // session properties.
    let preset_properties = unsafe { value.cast::<cf::CFDictionary>().as_ref() };

    if let Err(error) =
        unsafe { vt::VTSessionSetProperties(session.as_ref(), preset_properties) }.osstatus()
    {
        tracing::warn!("VideoToolbox rejected the {name} preset base configuration: {error}");
    }
}

fn preset_dictionaries(
    session: &vt::VTCompressionSession,
) -> Result<cf::CFRetained<cf::CFDictionary>, OSStatusError> {
    let property_key = cfstring_constant(c"kVTCompressionPropertyKey_SupportedPresetDictionaries")
        .ok_or(OSStatusError::VTPropertyNotSupported)?;

    allocate_retained(|ptr: NonNull<*const cf::CFDictionary>| unsafe {
        vt::VTSessionCopyProperty(session.as_ref(), property_key, None, ptr.as_ptr().cast())
    })
}

fn realtime_hint(preset: EncoderPreset, usage: Option<EncoderUsage>) -> Option<bool> {
    match usage {
        Some(EncoderUsage::Transcoding) => Some(false),
        // Recording implies a live source, so falling behind is data loss, not just latency.
        Some(EncoderUsage::Streaming | EncoderUsage::Conferencing | EncoderUsage::Recording) => {
            Some(true)
        }
        Some(EncoderUsage::Default) | None => {
            matches!(preset, EncoderPreset::LowLatency).then_some(true)
        }
    }
}

fn quality_for(preset: EncoderPreset) -> f64 {
    // values from the `Quality` property docs
    match preset {
        EncoderPreset::HighQuality => 0.75,
        EncoderPreset::Balanced => 0.5,
        EncoderPreset::LowLatency => 0.25,
    }
}

fn dimension_to_i32(value: u32, field: &'static str) -> Result<i32, VTEncoderError> {
    i32::try_from(value).map_err(|_| VTEncoderError::Parameters {
        field,
        problem: format!("{value} has to fit in i32"),
    })
}

fn is_keyframe(sample: &cm::CMSampleBuffer) -> bool {
    let Some(attachments) = (unsafe { sample.sample_attachments_array(false) }) else {
        return true;
    };

    // SAFETY: the attachments array of a sample buffer always holds attachment dictionaries.
    let attachments = unsafe {
        cf::CFRetained::cast_unchecked::<cf::CFArray<cf::CFDictionary<cf::CFString, cf::CFType>>>(
            attachments,
        )
    };

    let Some(attachment) = attachments.get(0) else {
        return true;
    };

    match attachment.get(unsafe { cm::kCMSampleAttachmentKey_NotSync }) {
        Some(val) => {
            let val = val.downcast::<cf::CFBoolean>().unwrap();
            !val.as_bool()
        }
        None => true,
    }
}

fn write_sample_as_annex_b(
    data: &cm::CMBlockBuffer,
    nal_length_size: usize,
    output: &mut Vec<u8>,
) -> Result<(), VTEncoderError> {
    let full_len = unsafe { data.data_length() };
    output.reserve(full_len);

    let mut offset = 0;
    while offset < full_len {
        let mut nal_length: [u8; 4] = [0; 4];
        unsafe {
            data.copy_data_bytes(
                offset,
                nal_length_size,
                NonNull::from(&mut nal_length[4 - nal_length_size..]).cast::<c_void>(),
            )
            .osstatus()?
        };

        let nal_length = u32::from_be_bytes(nal_length);
        let nal_start = offset + nal_length_size;

        output.extend_from_slice(&ANNEX_B_START_CODE);
        let start_in_output = output.len();
        output.resize(output.len() + nal_length as usize, 0);

        unsafe {
            data.copy_data_bytes(
                nal_start,
                nal_length as usize,
                NonNull::from(&mut output[start_in_output..]).cast(),
            )
            .osstatus()?;
        }

        offset = nal_start + nal_length as usize;
    }

    Ok(())
}
