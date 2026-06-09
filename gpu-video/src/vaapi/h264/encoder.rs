#[cfg(target_os = "linux")]
mod imp {
    use std::{
        borrow::Borrow,
        collections::VecDeque,
        os::fd::{AsFd, AsRawFd, OwnedFd},
        rc::Rc,
        sync::Arc,
        time::{Duration, Instant},
    };

    use crate::{
        dmabuf::{
            export_nv12_dmabuf_texture, validate_nv12_dmabuf_frame, DmaBufFrame,
        },
        EncodedOutputChunk, InputFrame, VideoFramerate, VideoResolution,
    };
    use bytes::Bytes;
    use libva::{
        BufferType, Config, Context, Display, EncCodedBuffer, EncMiscParameter,
        EncMiscParameterFrameRate, EncMiscParameterRateControl, EncPictureParameter,
        EncPictureParameterBufferH264, EncSequenceParameter,
        EncSequenceParameterBufferH264, EncSliceParameter, EncSliceParameterBufferH264,
        ExternalBufferDescriptor, H264EncFrameCropOffsets, H264EncPicFields,
        H264EncSeqFields, H264VuiFields, MappedCodedBuffer, MemoryType, Picture,
        PictureH264, PictureNew, RcFlags, Surface, UsageHint, VAConfigAttrib,
        VAConfigAttribType, VADRMPRIMESurfaceDescriptor, VAEntrypoint, VAProfile,
        VASurfaceAttribType, VASurfaceStatus, VA_ATTRIB_NOT_SUPPORTED, VA_INVALID_ID,
        VA_PICTURE_H264_SHORT_TERM_REFERENCE, VA_RC_CBR, VA_RC_VBR, VA_RT_FORMAT_YUV420,
    };
    use tracing::{info, warn};

    use crate::vaapi::display::{
        invalid_h264_pictures, open_display, take_nv12_surface,
    };

    use super::super::parameter_sets::{
        main_parameter_sets, H264_LEVEL_4_0, LOG2_MAX_FRAME_NUM_MINUS4,
        LOG2_MAX_PIC_ORDER_CNT_LSB_MINUS4,
    };

    const RECONSTRUCTED_SURFACE_ALLOCATION_BATCH: usize = 4;
    const DEFAULT_CODED_BUFFER_SIZE: usize = 1_500_000;
    const CBR_RATE_CONTROL: VaapiRateControlConfig = VaapiRateControlConfig {
        mode: VA_RC_CBR,
        bits_per_second: 0,
        target_percentage: 100,
        window_size: 1_500,
        initial_qp: 26,
        min_qp: 10,
        basic_unit_size: 0,
        disable_bit_stuffing: false,
        max_qp: 51,
    };

    #[derive(Clone, Copy)]
    struct VaapiRateControlConfig {
        mode: u32,
        bits_per_second: u32,
        target_percentage: u32,
        window_size: u32,
        initial_qp: u32,
        min_qp: u32,
        basic_unit_size: u32,
        disable_bit_stuffing: bool,
        max_qp: u32,
    }

    pub struct H264Encoder {
        encoder: IntelVaapiH264Encoder,
        parameter_sets: Bytes,
    }

    /// H.264 encoder that accepts NV12 [`wgpu::Texture`] inputs.
    ///
    /// VA-API consumes DMA-BUF-backed surfaces, so each input texture is copied into
    /// an internal DMA-BUF pool before encode. The copy is synchronously waited on
    /// before submitting the frame to VA-API.
    pub struct WgpuTexturesEncoderH264 {
        encoder: H264Encoder,
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        input_pool: Vec<Arc<DmaBufFrame>>,
        next_input_index: usize,
        resolution: VideoResolution,
    }

    #[derive(Debug, Clone)]
    pub struct H264EncoderConfig {
        pub adapter_info: Option<wgpu::AdapterInfo>,
        pub resolution: VideoResolution,
        pub rate_control: H264EncoderRateControl,
        pub gop_size: u16,
        pub framerate: VideoFramerate,
        pub max_pending_frames: usize,
    }

    #[derive(Debug, Clone, Copy)]
    pub enum H264EncoderRateControl {
        VariableBitrate {
            average_bitrate: u32,
            max_bitrate: u32,
            virtual_buffer_size: Duration,
        },
        ConstantBitrate {
            bitrate: u32,
            virtual_buffer_size: Duration,
        },
    }

    impl H264EncoderRateControl {
        fn config(self) -> VaapiRateControlConfig {
            match self {
                Self::VariableBitrate {
                    average_bitrate,
                    max_bitrate,
                    virtual_buffer_size,
                } => {
                    let max_bitrate = max_bitrate.max(average_bitrate).max(1);
                    let target_percentage = ((u64::from(average_bitrate) * 100)
                        / u64::from(max_bitrate))
                    .clamp(1, 100) as u32;
                    VaapiRateControlConfig {
                        mode: VA_RC_VBR,
                        bits_per_second: max_bitrate,
                        target_percentage,
                        window_size: duration_millis_u32(virtual_buffer_size),
                        disable_bit_stuffing: true,
                        ..CBR_RATE_CONTROL
                    }
                }
                Self::ConstantBitrate { bitrate, virtual_buffer_size } => {
                    VaapiRateControlConfig {
                        bits_per_second: bitrate.max(1),
                        window_size: duration_millis_u32(virtual_buffer_size),
                        ..CBR_RATE_CONTROL
                    }
                }
            }
        }
    }

    #[derive(Debug, thiserror::Error)]
    pub enum VaapiH264EncoderError {
        #[error("VA-API H264 encoder is unavailable: {0}")]
        Unavailable(String),

        #[error("VA-API H264 encode error: {0}")]
        Encode(String),

        #[error("VA-API H264 encoder requires COPY_SRC texture usage, got {0:?}")]
        NoCopySrcTextureUsage(wgpu::TextureUsages),

        #[error("VA-API H264 encoder requires NV12 textures, got {0:?}")]
        NotNv12Texture(wgpu::TextureFormat),

        #[error(
            "VA-API H264 encoder expected texture size {expected:?}, got {provided:?}"
        )]
        InconsistentTextureSize { expected: wgpu::Extent3d, provided: wgpu::Extent3d },
    }

    impl H264Encoder {
        pub fn new(config: H264EncoderConfig) -> Result<Self, VaapiH264EncoderError> {
            info!("Initializing VA-API H264 encoder");

            let display = open_display(config.adapter_info.as_ref())
                .map_err(VaapiH264EncoderError::Unavailable)?;
            let parameter_sets = main_parameter_sets(config.resolution, config.framerate);

            let encoder = IntelVaapiH264Encoder::new(
                display,
                config.resolution,
                config.rate_control,
                config.gop_size.max(1),
                config.framerate,
                config.max_pending_frames,
                parameter_sets.clone(),
            )
            .map_err(VaapiH264EncoderError::Unavailable)?;

            info!(
                width = config.resolution.width,
                height = config.resolution.height,
                rate_control = ?config.rate_control,
                max_pending_frames = config.max_pending_frames,
                "Initialized VA-API H264 encoder with direct NV12 DMA-BUF input"
            );

            Ok(Self { encoder, parameter_sets })
        }

        pub fn parameter_sets(&self) -> &Bytes {
            &self.parameter_sets
        }

        pub fn encode(
            &mut self,
            frame: Arc<DmaBufFrame>,
            pts: Option<u64>,
            force_keyframe: bool,
        ) -> Result<Vec<EncodedOutputChunk<Bytes>>, VaapiH264EncoderError> {
            self.encoder
                .encode(frame, pts, force_keyframe)
                .map_err(VaapiH264EncoderError::Encode)
        }

        pub fn flush(&mut self) -> Result<Vec<EncodedOutputChunk<Bytes>>, VaapiH264EncoderError> {
            self.encoder.flush().map_err(VaapiH264EncoderError::Encode)
        }
    }

    impl WgpuTexturesEncoderH264 {
        pub fn new(
            device: Arc<wgpu::Device>,
            queue: Arc<wgpu::Queue>,
            config: H264EncoderConfig,
        ) -> Result<Self, VaapiH264EncoderError> {
            let resolution = config.resolution;
            let pool_size = config.max_pending_frames + 2;
            let encoder = H264Encoder::new(config)?;
            let input_pool = (0..pool_size)
                .map(|_| export_nv12_dmabuf_texture(&device, resolution))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|err| VaapiH264EncoderError::Unavailable(err.to_string()))?;
            Ok(Self {
                encoder,
                device,
                queue,
                input_pool,
                next_input_index: 0,
                resolution,
            })
        }

        pub fn parameter_sets(&self) -> &Bytes {
            self.encoder.parameter_sets()
        }

        pub fn encode(
            &mut self,
            frame: InputFrame<wgpu::Texture>,
            force_keyframe: bool,
        ) -> Result<Vec<EncodedOutputChunk<Bytes>>, VaapiH264EncoderError> {
            let pts = frame.pts;
            let frame = self.copy_input_to_dmabuf(frame.data)?;
            self.encoder.encode(frame, pts, force_keyframe)
        }

        pub fn flush(&mut self) -> Result<Vec<EncodedOutputChunk<Bytes>>, VaapiH264EncoderError> {
            self.encoder.flush()
        }

        fn copy_input_to_dmabuf(
            &mut self,
            texture: wgpu::Texture,
        ) -> Result<Arc<DmaBufFrame>, VaapiH264EncoderError> {
            let expected_size = wgpu::Extent3d {
                width: self.resolution.width,
                height: self.resolution.height,
                depth_or_array_layers: 1,
            };
            if !texture.usage().contains(wgpu::TextureUsages::COPY_SRC) {
                return Err(VaapiH264EncoderError::NoCopySrcTextureUsage(
                    texture.usage(),
                ));
            }
            if texture.format() != wgpu::TextureFormat::NV12 {
                return Err(VaapiH264EncoderError::NotNv12Texture(texture.format()));
            }
            if texture.size() != expected_size {
                return Err(VaapiH264EncoderError::InconsistentTextureSize {
                    expected: expected_size,
                    provided: texture.size(),
                });
            }

            let input = self.next_input_frame()?;
            let mut encoder =
                self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("VA-API H264 encoder input copy"),
                });
            encoder.copy_texture_to_texture(
                texture.as_image_copy(),
                input.texture().as_image_copy(),
                expected_size,
            );
            self.queue.submit([encoder.finish()]);
            self.device
                .poll(wgpu::PollType::wait_indefinitely())
                .map_err(|err| VaapiH264EncoderError::Encode(err.to_string()))?;
            Ok(input)
        }

        fn next_input_frame(
            &mut self,
        ) -> Result<Arc<DmaBufFrame>, VaapiH264EncoderError> {
            for _ in 0..self.input_pool.len() {
                let index = self.next_input_index;
                self.next_input_index =
                    (self.next_input_index + 1) % self.input_pool.len();
                if Arc::strong_count(&self.input_pool[index]) == 1 {
                    return Ok(Arc::clone(&self.input_pool[index]));
                }
            }

            let frame = export_nv12_dmabuf_texture(&self.device, self.resolution)
                .map_err(|err| VaapiH264EncoderError::Encode(err.to_string()))?;
            self.input_pool.push(Arc::clone(&frame));
            Ok(frame)
        }
    }

    struct IntelVaapiH264Encoder {
        _config: Config,
        context: Rc<Context>,
        display: Rc<Display>,
        free_input_surfaces: Vec<VaapiInputSurface>,
        free_reconstructed_surfaces: Vec<Surface<()>>,
        pending: VecDeque<PendingEncode>,
        retired_after_producer: Vec<Surface<()>>,
        reference: Option<EncodedReference>,
        resolution: VideoResolution,
        rate_control: VaapiRateControlConfig,
        gop_size: u16,
        max_pending_frames: usize,
        frames_since_keyframe: u16,
        frame_num: u16,
        idr_pic_id: u16,
        framerate: VideoFramerate,
        parameter_sets: Bytes,
    }

    impl IntelVaapiH264Encoder {
        fn new(
            display: Rc<Display>,
            resolution: VideoResolution,
            rate_control: H264EncoderRateControl,
            gop_size: u16,
            framerate: VideoFramerate,
            max_pending_frames: usize,
            parameter_sets: Bytes,
        ) -> Result<Self, String> {
            let profile = VAProfile::VAProfileH264Main;
            let entrypoint = h264_encode_entrypoint(&display, profile)?;
            let rate_control = rate_control.config();
            validate_h264_rate_control_support(
                &display,
                profile,
                entrypoint,
                rate_control.mode,
            )?;
            let mut config = display
                .create_config(
                    vec![
                        VAConfigAttrib {
                            type_: VAConfigAttribType::VAConfigAttribRTFormat,
                            value: VA_RT_FORMAT_YUV420,
                        },
                        VAConfigAttrib {
                            type_: VAConfigAttribType::VAConfigAttribRateControl,
                            value: rate_control.mode,
                        },
                    ],
                    profile,
                    entrypoint,
                )
                .map_err(|err| format!("failed to create VA-API H264 config: {err}"))?;
            validate_h264_encode_surface_support(&mut config)?;
            let context = display
                .create_context::<()>(
                    &config,
                    resolution.width,
                    resolution.height,
                    None,
                    true,
                )
                .map_err(|err| format!("failed to create VA-API H264 context: {err}"))?;

            Ok(Self {
                _config: config,
                context,
                display,
                free_input_surfaces: Vec::new(),
                free_reconstructed_surfaces: Vec::new(),
                pending: VecDeque::new(),
                retired_after_producer: Vec::new(),
                reference: None,
                resolution,
                rate_control,
                gop_size,
                max_pending_frames,
                frames_since_keyframe: 0,
                frame_num: 0,
                idr_pic_id: 0,
                framerate,
                parameter_sets,
            })
        }

        fn encode(
            &mut self,
            frame: Arc<DmaBufFrame>,
            pts: Option<u64>,
            force_keyframe: bool,
        ) -> Result<Vec<EncodedOutputChunk<Bytes>>, String> {
            let mut completed = self.collect_ready()?;
            let pending = self.submit(frame, pts, force_keyframe)?;
            self.pending.push_back(pending);
            while self.pending.len() > self.max_pending_frames {
                completed.push(self.complete_oldest()?);
            }
            Ok(completed)
        }

        fn flush(&mut self) -> Result<Vec<EncodedOutputChunk<Bytes>>, String> {
            let mut completed = self.collect_ready()?;
            while !self.pending.is_empty() {
                completed.push(self.complete_oldest()?);
            }
            Ok(completed)
        }

        fn submit(
            &mut self,
            frame: Arc<DmaBufFrame>,
            pts: Option<u64>,
            force_keyframe: bool,
        ) -> Result<PendingEncode, String> {
            let started_at = Instant::now();
            let input = self.take_input_surface(frame)?;
            let is_keyframe = force_keyframe
                || self.reference.is_none()
                || self.frames_since_keyframe >= self.gop_size;
            let reconstructed = self.take_reconstructed_surface()?;
            let coded_buffer = self
                .context
                .create_enc_coded(self.coded_buffer_size())
                .map_err(|err| format!("failed to create VA-API coded buffer: {err}"))?;

            let mut picture =
                Picture::new(pts.unwrap_or_default(), Rc::clone(&self.context), input);
            self.add_buffers(&mut picture, &coded_buffer, &reconstructed, is_keyframe)?;

            let picture = picture
                .begin()
                .map_err(|err| format!("failed to begin VA-API picture: {err}"))?
                .render()
                .map_err(|err| format!("failed to render VA-API picture: {err}"))?
                .end()
                .map_err(|err| format!("failed to end VA-API picture: {err}"))?;
            let elapsed = started_at.elapsed();
            if elapsed > Duration::from_millis(25) {
                warn!(
                    submit_ms = elapsed.as_millis(),
                    is_keyframe,
                    pts_us = pts,
                    "slow VA-API H264 encode submit"
                );
            }

            let reconstructed_id = reconstructed.id();
            let retired_reference = self.rotate_reference(reconstructed, is_keyframe);
            let retired_after_sync = match (is_keyframe, retired_reference) {
                (false, Some(reference)) => Some(reference.surface),
                (false, None) => None,
                (true, Some(reference))
                    if self.producer_is_pending(reference.surface.id()) =>
                {
                    self.retired_after_producer.push(reference.surface);
                    None
                }
                (true, Some(reference)) => {
                    self.free_reconstructed_surfaces.push(reference.surface);
                    None
                }
                (true, None) => None,
            };

            Ok(PendingEncode {
                picture,
                coded_buffer,
                reconstructed_id,
                retired_after_sync,
                pts,
                is_keyframe,
                submitted_at: started_at,
            })
        }

        fn take_input_surface(
            &mut self,
            frame: Arc<DmaBufFrame>,
        ) -> Result<VaapiInputSurface, String> {
            validate_nv12_dmabuf_frame(&frame, self.resolution)
                .map_err(|err| err.to_string())?;
            let frame_key = DmaBufFrameKey::from_frame(&frame);
            let mut surface = match self
                .free_input_surfaces
                .iter()
                .position(|surface| surface.frame_key == frame_key)
            {
                Some(index) => self.free_input_surfaces.swap_remove(index),
                None => self.create_input_surface(&frame, frame_key)?,
            };
            surface.frame_lease = Some(frame);
            Ok(surface)
        }

        fn create_input_surface(
            &self,
            frame: &Arc<DmaBufFrame>,
            frame_key: DmaBufFrameKey,
        ) -> Result<VaapiInputSurface, String> {
            let descriptor = VaapiInputSurfaceDescriptor::new(frame.as_ref())?;
            let surfaces = self
                .display
                .create_surfaces(
                    libva::VA_RT_FORMAT_YUV420,
                    Some(libva::VA_FOURCC_NV12),
                    self.resolution.width,
                    self.resolution.height,
                    Some(UsageHint::USAGE_HINT_ENCODER),
                    vec![descriptor],
                )
                .map_err(|err| {
                    format!(
                        "failed to import NV12 DMA-BUF as VA-API input surface: {err}"
                    )
                })?;
            let surface = surfaces
                .into_iter()
                .next()
                .ok_or_else(|| "VA-API returned no imported input surface".to_string())?;
            Ok(VaapiInputSurface {
                frame_key,
                frame_lease: Some(Arc::clone(frame)),
                surface,
            })
        }

        fn release_input_surface(&mut self, mut surface: VaapiInputSurface) {
            surface.frame_lease = None;
            self.free_input_surfaces.push(surface);
        }

        fn add_buffers(
            &self,
            picture: &mut Picture<PictureNew, VaapiInputSurface>,
            coded_buffer: &EncCodedBuffer,
            reconstructed: &Surface<()>,
            is_keyframe: bool,
        ) -> Result<(), String> {
            for buffer in [
                self.sequence_parameter(),
                self.picture_parameter(coded_buffer, reconstructed, is_keyframe),
                self.slice_parameter(is_keyframe),
                self.rate_control_parameter(),
                self.framerate_parameter(),
            ] {
                let buffer = self
                    .context
                    .create_buffer(buffer)
                    .map_err(|err| format!("failed to create VA-API buffer: {err}"))?;
                picture.add_buffer(buffer);
            }
            Ok(())
        }

        fn sequence_parameter(&self) -> BufferType {
            let (width_mbs, height_mbs) = self.macroblocks();
            let (crop_right, crop_bottom) = self.crop_offsets();
            let frame_crop = (crop_right > 0 || crop_bottom > 0)
                .then(|| H264EncFrameCropOffsets::new(0, crop_right, 0, crop_bottom));
            let seq_fields = H264EncSeqFields::new(
                1,
                1,
                0,
                0,
                1,
                LOG2_MAX_FRAME_NUM_MINUS4,
                0,
                LOG2_MAX_PIC_ORDER_CNT_LSB_MINUS4,
                0,
            );
            BufferType::EncSequenceParameter(EncSequenceParameter::H264(
                EncSequenceParameterBufferH264::new(
                    0,
                    H264_LEVEL_4_0,
                    self.gop_size.into(),
                    self.gop_size.into(),
                    1,
                    self.rate_control.bits_per_second,
                    1,
                    width_mbs as u16,
                    height_mbs as u16,
                    &seq_fields,
                    0,
                    0,
                    0,
                    0,
                    0,
                    [0; 256],
                    frame_crop,
                    Some(H264VuiFields::new(1, 1, 1, 16, 16, 1, 0, 1)),
                    1,
                    1,
                    1,
                    self.framerate.den.max(1),
                    self.framerate.num.max(1).saturating_mul(2),
                ),
            ))
        }

        fn picture_parameter(
            &self,
            coded_buffer: &EncCodedBuffer,
            reconstructed: &Surface<()>,
            is_keyframe: bool,
        ) -> BufferType {
            let mut reference_frames = invalid_h264_pictures::<16>();
            if let Some(reference) = self.active_reference(is_keyframe) {
                reference_frames[0] = reference.picture();
            }
            let pic_fields =
                H264EncPicFields::new(is_keyframe as u32, 1, 0, 0, 0, 0, 0, 1, 0, 0, 0);
            BufferType::EncPictureParameter(EncPictureParameter::H264(
                EncPictureParameterBufferH264::new(
                    PictureH264::new(
                        reconstructed.id(),
                        self.frame_num_for(is_keyframe).into(),
                        VA_PICTURE_H264_SHORT_TERM_REFERENCE,
                        self.poc_for(is_keyframe).into(),
                        self.poc_for(is_keyframe).into(),
                    ),
                    reference_frames,
                    coded_buffer.id(),
                    0,
                    0,
                    0,
                    self.frame_num_for(is_keyframe),
                    26,
                    0,
                    0,
                    0,
                    0,
                    &pic_fields,
                ),
            ))
        }

        fn slice_parameter(&self, is_keyframe: bool) -> BufferType {
            let mut ref_pic_list_0 = invalid_h264_pictures::<32>();
            if let Some(reference) = self.active_reference(is_keyframe) {
                ref_pic_list_0[0] = reference.picture();
            }
            BufferType::EncSliceParameter(EncSliceParameter::H264(
                EncSliceParameterBufferH264::new(
                    0,
                    self.macroblock_count(),
                    VA_INVALID_ID,
                    if is_keyframe { 2 } else { 0 },
                    0,
                    self.idr_pic_id,
                    self.poc_for(is_keyframe),
                    0,
                    [0, 0],
                    1,
                    (!is_keyframe) as u8,
                    0,
                    0,
                    ref_pic_list_0,
                    invalid_h264_pictures::<32>(),
                    0,
                    0,
                    0,
                    [0; 32],
                    [0; 32],
                    0,
                    [[0; 2]; 32],
                    [[0; 2]; 32],
                    0,
                    [0; 32],
                    [0; 32],
                    0,
                    [[0; 2]; 32],
                    [[0; 2]; 32],
                    0,
                    0,
                    0,
                    2,
                    2,
                ),
            ))
        }

        fn rate_control_parameter(&self) -> BufferType {
            let rc = self.rate_control;
            BufferType::EncMiscParameter(EncMiscParameter::RateControl(
                EncMiscParameterRateControl::new(
                    rc.bits_per_second,
                    rc.target_percentage,
                    rc.window_size,
                    rc.initial_qp,
                    rc.min_qp,
                    rc.basic_unit_size,
                    RcFlags::new(
                        0,
                        1,
                        if rc.disable_bit_stuffing { 1 } else { 0 },
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                    ),
                    0,
                    rc.max_qp,
                    0,
                    0,
                ),
            ))
        }

        fn framerate_parameter(&self) -> BufferType {
            BufferType::EncMiscParameter(EncMiscParameter::FrameRate(
                EncMiscParameterFrameRate::new(rounded_framerate(self.framerate), 0),
            ))
        }

        fn take_reconstructed_surface(&mut self) -> Result<Surface<()>, String> {
            take_nv12_surface(
                &self.display,
                &mut self.free_reconstructed_surfaces,
                self.resolution,
                UsageHint::USAGE_HINT_ENCODER,
                RECONSTRUCTED_SURFACE_ALLOCATION_BATCH,
                "reconstructed",
            )
        }

        fn rotate_reference(
            &mut self,
            surface: Surface<()>,
            encoded_keyframe: bool,
        ) -> Option<EncodedReference> {
            let retired = self.reference.take();
            self.reference = Some(EncodedReference {
                surface,
                frame_num: self.frame_num_for(encoded_keyframe),
                poc: self.poc_for(encoded_keyframe),
            });
            self.frame_num = if encoded_keyframe {
                self.idr_pic_id = self.idr_pic_id.wrapping_add(1);
                self.frames_since_keyframe = 1;
                1
            } else {
                self.frames_since_keyframe = self.frames_since_keyframe.saturating_add(1);
                self.frame_num.wrapping_add(1)
            };
            retired
        }

        fn producer_is_pending(&self, surface_id: libva::VASurfaceID) -> bool {
            self.pending.iter().any(|pending| pending.reconstructed_id == surface_id)
        }

        fn collect_ready(&mut self) -> Result<Vec<EncodedOutputChunk<Bytes>>, String> {
            let mut completed = Vec::new();
            while self.pending.front().is_some_and(PendingEncode::is_ready) {
                completed.push(self.complete_oldest()?);
            }
            Ok(completed)
        }

        fn complete_oldest(&mut self) -> Result<EncodedOutputChunk<Bytes>, String> {
            let pending = self
                .pending
                .pop_front()
                .ok_or_else(|| "VA-API encoder has no pending frame".to_string())?;
            self.complete_pending(pending)
        }

        fn complete_pending(
            &mut self,
            pending: PendingEncode,
        ) -> Result<EncodedOutputChunk<Bytes>, String> {
            let sync_started_at = Instant::now();
            let picture = pending
                .picture
                .sync()
                .map_err(|(err, _)| format!("failed to sync VA-API picture: {err}"))?;
            let sync_elapsed = sync_started_at.elapsed();
            let input = picture
                .take_surface()
                .map_err(|_| "VA-API picture kept a shared input surface".to_string())?;
            self.release_input_surface(input);

            let map_started_at = Instant::now();
            let data =
                self.collect_coded_data(&pending.coded_buffer, pending.is_keyframe)?;
            let map_elapsed = map_started_at.elapsed();
            let elapsed = pending.submitted_at.elapsed();
            if sync_elapsed > Duration::from_millis(10)
                || map_elapsed > Duration::from_millis(10)
            {
                warn!(
                    elapsed_ms = elapsed.as_millis(),
                    sync_ms = sync_elapsed.as_millis(),
                    map_ms = map_elapsed.as_millis(),
                    is_keyframe = pending.is_keyframe,
                    bytes = data.len(),
                    pts_us = pending.pts,
                    "completed VA-API H264 encode frame"
                );
            }

            if let Some(surface) = pending.retired_after_sync {
                self.free_reconstructed_surfaces.push(surface);
            }
            self.release_retired_producer(pending.reconstructed_id);

            Ok(EncodedOutputChunk {
                data,
                pts: pending.pts,
                is_keyframe: pending.is_keyframe,
            })
        }

        fn release_retired_producer(&mut self, surface_id: libva::VASurfaceID) {
            if let Some(index) = self
                .retired_after_producer
                .iter()
                .position(|surface| surface.id() == surface_id)
            {
                let surface = self.retired_after_producer.swap_remove(index);
                self.free_reconstructed_surfaces.push(surface);
            }
        }

        fn collect_coded_data(
            &self,
            coded_buffer: &EncCodedBuffer,
            is_keyframe: bool,
        ) -> Result<Bytes, String> {
            let mapped = MappedCodedBuffer::new(coded_buffer)
                .map_err(|err| format!("failed to map VA-API coded buffer: {err}"))?;
            let slice_len = mapped.iter().map(|segment| segment.buf.len()).sum::<usize>();
            if slice_len == 0 {
                return Err("VA-API encoder returned empty coded data".into());
            }
            let starts_with_three_byte_code = mapped
                .iter()
                .flat_map(|segment| segment.buf.iter().copied())
                .take(3)
                .eq([0, 0, 1]);
            let parameter_sets_len =
                is_keyframe.then_some(self.parameter_sets.len()).unwrap_or_default();
            let mut out = Vec::with_capacity(
                parameter_sets_len + slice_len + starts_with_three_byte_code as usize,
            );
            if is_keyframe {
                out.extend_from_slice(&self.parameter_sets);
            }
            if starts_with_three_byte_code {
                out.push(0);
            }
            for segment in mapped.iter() {
                out.extend_from_slice(segment.buf);
            }
            Ok(out.into())
        }

        fn active_reference(&self, is_keyframe: bool) -> Option<&EncodedReference> {
            (!is_keyframe).then_some(self.reference.as_ref()).flatten()
        }

        fn coded_buffer_size(&self) -> usize {
            let raw_size =
                self.resolution.width as usize * self.resolution.height as usize * 3 / 2;
            ((self.rate_control.bits_per_second as usize / 8) * 2)
                .max(DEFAULT_CODED_BUFFER_SIZE)
                .max(raw_size)
        }

        fn macroblocks(&self) -> (u32, u32) {
            (self.resolution.width.div_ceil(16), self.resolution.height.div_ceil(16))
        }

        fn macroblock_count(&self) -> u32 {
            let (width_mbs, height_mbs) = self.macroblocks();
            width_mbs * height_mbs
        }

        fn crop_offsets(&self) -> (u32, u32) {
            let (width_mbs, height_mbs) = self.macroblocks();
            (
                (width_mbs * 16 - self.resolution.width) / 2,
                (height_mbs * 16 - self.resolution.height) / 2,
            )
        }

        fn frame_num_for(&self, is_keyframe: bool) -> u16 {
            if is_keyframe {
                0
            } else {
                self.frame_num
            }
        }

        fn poc_for(&self, is_keyframe: bool) -> u16 {
            self.frame_num_for(is_keyframe).wrapping_mul(2)
        }
    }

    struct EncodedReference {
        surface: Surface<()>,
        frame_num: u16,
        poc: u16,
    }

    #[derive(Clone, Copy, Eq, PartialEq)]
    struct DmaBufFrameKey(usize);

    impl DmaBufFrameKey {
        fn from_frame(frame: &Arc<DmaBufFrame>) -> Self {
            Self(Arc::as_ptr(frame) as usize)
        }
    }

    struct VaapiInputSurfaceDescriptor {
        fourcc: u32,
        width: u32,
        height: u32,
        objects: Vec<VaapiInputSurfaceObject>,
        layers: Vec<VaapiInputSurfaceLayer>,
    }

    struct VaapiInputSurfaceObject {
        fd: OwnedFd,
        size: u32,
        modifier: u64,
    }

    struct VaapiInputSurfaceLayer {
        drm_format: u32,
        planes: Vec<VaapiInputSurfacePlane>,
    }

    struct VaapiInputSurfacePlane {
        object_index: u8,
        offset: u32,
        pitch: u32,
    }

    impl VaapiInputSurfaceDescriptor {
        fn new(frame: &DmaBufFrame) -> Result<Self, String> {
            let objects = frame
                .objects()
                .iter()
                .map(|object| {
                    Ok(VaapiInputSurfaceObject {
                        fd: object.fd.as_fd().try_clone_to_owned().map_err(|err| {
                            format!("failed to duplicate DMA-BUF fd: {err}")
                        })?,
                        size: object.size,
                        modifier: object.modifier,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            let layers = frame
                .layers()
                .iter()
                .map(|layer| {
                    let planes = layer
                        .planes
                        .iter()
                        .map(|plane| {
                            Ok(VaapiInputSurfacePlane {
                                object_index: plane.object_index.try_into().map_err(|_| {
                                    format!(
                                        "DMA-BUF plane object index {} does not fit VA-API",
                                        plane.object_index
                                    )
                                })?,
                                offset: plane.offset,
                                pitch: plane.pitch,
                            })
                        })
                        .collect::<Result<Vec<_>, String>>()?;
                    Ok(VaapiInputSurfaceLayer { drm_format: layer.drm_format, planes })
                })
                .collect::<Result<Vec<_>, String>>()?;

            Ok(Self {
                fourcc: frame.fourcc(),
                width: frame.width(),
                height: frame.height(),
                objects,
                layers,
            })
        }
    }

    impl ExternalBufferDescriptor for VaapiInputSurfaceDescriptor {
        const MEMORY_TYPE: MemoryType = MemoryType::DrmPrime2;
        type DescriptorAttribute = VADRMPRIMESurfaceDescriptor;

        fn va_surface_attribute(&mut self) -> Self::DescriptorAttribute {
            let mut descriptor = VADRMPRIMESurfaceDescriptor {
                fourcc: self.fourcc,
                width: self.width,
                height: self.height,
                num_objects: self.objects.len().try_into().unwrap(),
                num_layers: self.layers.len().try_into().unwrap(),
                ..Default::default()
            };

            for (index, object) in self.objects.iter().enumerate() {
                descriptor.objects[index].fd = object.fd.as_raw_fd();
                descriptor.objects[index].size = object.size;
                descriptor.objects[index].drm_format_modifier = object.modifier;
            }
            for (layer_index, layer) in self.layers.iter().enumerate() {
                descriptor.layers[layer_index].drm_format = layer.drm_format;
                descriptor.layers[layer_index].num_planes =
                    layer.planes.len().try_into().unwrap();
                for (plane_index, plane) in layer.planes.iter().enumerate() {
                    descriptor.layers[layer_index].object_index[plane_index] =
                        plane.object_index.into();
                    descriptor.layers[layer_index].offset[plane_index] = plane.offset;
                    descriptor.layers[layer_index].pitch[plane_index] = plane.pitch;
                }
            }

            descriptor
        }
    }

    struct VaapiInputSurface {
        frame_key: DmaBufFrameKey,
        frame_lease: Option<Arc<DmaBufFrame>>,
        surface: Surface<VaapiInputSurfaceDescriptor>,
    }

    impl Borrow<Surface<VaapiInputSurfaceDescriptor>> for VaapiInputSurface {
        fn borrow(&self) -> &Surface<VaapiInputSurfaceDescriptor> {
            &self.surface
        }
    }

    struct PendingEncode {
        picture: Picture<libva::PictureEnd, VaapiInputSurface>,
        coded_buffer: EncCodedBuffer,
        reconstructed_id: libva::VASurfaceID,
        retired_after_sync: Option<Surface<()>>,
        pts: Option<u64>,
        is_keyframe: bool,
        submitted_at: Instant,
    }

    impl PendingEncode {
        fn is_ready(&self) -> bool {
            match self.picture.surface().query_status() {
                Ok(status) => status == VASurfaceStatus::VASurfaceReady,
                Err(_) => true,
            }
        }
    }

    impl EncodedReference {
        fn picture(&self) -> PictureH264 {
            PictureH264::new(
                self.surface.id(),
                self.frame_num.into(),
                VA_PICTURE_H264_SHORT_TERM_REFERENCE,
                self.poc.into(),
                self.poc.into(),
            )
        }
    }

    fn h264_encode_entrypoint(
        display: &Display,
        profile: VAProfile::Type,
    ) -> Result<VAEntrypoint::Type, String> {
        let entrypoints = display
            .query_config_entrypoints(profile)
            .map_err(|err| format!("failed to query VA-API H264 entrypoints: {err}"))?;
        if entrypoints.contains(&VAEntrypoint::VAEntrypointEncSliceLP) {
            Ok(VAEntrypoint::VAEntrypointEncSliceLP)
        } else if entrypoints.contains(&VAEntrypoint::VAEntrypointEncSlice) {
            Ok(VAEntrypoint::VAEntrypointEncSlice)
        } else {
            Err("VA-API H264 encode entrypoint is unavailable".into())
        }
    }

    fn validate_h264_rate_control_support(
        display: &Display,
        profile: VAProfile::Type,
        entrypoint: VAEntrypoint::Type,
        rate_control: u32,
    ) -> Result<(), String> {
        let mut attrs = [VAConfigAttrib {
            type_: VAConfigAttribType::VAConfigAttribRateControl,
            value: 0,
        }];
        display.get_config_attributes(profile, entrypoint, &mut attrs).map_err(
            |err| format!("failed to query VA-API H264 rate-control support: {err}"),
        )?;
        let supported = attrs[0].value;
        if supported == VA_ATTRIB_NOT_SUPPORTED || supported & rate_control == 0 {
            Err(format!(
                "VA-API H264 encode config does not support rate-control mode 0x{rate_control:x}; got 0x{supported:x}"
            ))
        } else {
            Ok(())
        }
    }

    fn validate_h264_encode_surface_support(config: &mut Config) -> Result<(), String> {
        validate_config_int_attr(
            config,
            VASurfaceAttribType::VASurfaceAttribPixelFormat,
            libva::VA_FOURCC_NV12 as i32,
            "NV12 surface pixel format",
            false,
        )?;
        Ok(())
    }

    fn validate_config_int_attr(
        config: &mut Config,
        attr_type: VASurfaceAttribType::Type,
        required: i32,
        label: &str,
        bitmask: bool,
    ) -> Result<(), String> {
        let values =
            config.query_surface_attributes_by_type(attr_type).map_err(|err| {
                format!("failed to query VA-API H264 encode support for {label}: {err}")
            })?;
        let integers = values
            .into_iter()
            .map(|value| match value {
                libva::GenericValue::Integer(value) => Ok(value),
                other => Err(format!(
                    "VA-API H264 encode support for {label} returned non-integer {other:?}"
                )),
            })
            .collect::<Result<Vec<_>, _>>()?;

        if integers.is_empty() {
            return Err(format!(
                "VA-API H264 encode config does not advertise support for {label}"
            ));
        }

        let supported = integers.iter().any(|value| {
            if bitmask {
                value & required == required
            } else {
                *value == required
            }
        });
        if supported {
            Ok(())
        } else {
            Err(format!(
                "VA-API H264 encode config does not support {label}; got {integers:?}"
            ))
        }
    }

    fn duration_millis_u32(duration: Duration) -> u32 {
        duration.as_millis().clamp(1, u128::from(u32::MAX)) as u32
    }

    fn rounded_framerate(framerate: VideoFramerate) -> u32 {
        let den = u64::from(framerate.den.max(1));
        let rounded = (u64::from(framerate.num) + den / 2) / den;
        rounded.max(1).min(u64::from(u32::MAX)) as u32
    }

    #[cfg(all(test, target_os = "linux"))]
    mod tests {
        use std::{
            fs,
            io::Write,
            process::{Command, Stdio},
            sync::Mutex,
            thread,
            time::{Duration, Instant},
        };

        use super::*;
        const TEST_RESOLUTION: VideoResolution =
            VideoResolution { width: 64, height: 64 };
        const STRESS_RESOLUTION: VideoResolution =
            VideoResolution { width: 1280, height: 720 };
        const TEST_FRAMERATE: VideoFramerate = VideoFramerate { num: 30, den: 1 };
        const WT_PREVIEW_FRAMERATE: VideoFramerate =
            VideoFramerate { num: 30_000, den: 1001 };
        const MAX_PENDING_ENCODE_FRAMES: usize = 8;
        static VAAPI_TEST_LOCK: Mutex<()> = Mutex::new(());

        #[test]
        fn rounds_vaapi_rate_control_framerate() {
            assert_eq!(rounded_framerate(VideoFramerate { num: 30_000, den: 1001 }), 30);
            assert_eq!(rounded_framerate(VideoFramerate { num: 24_000, den: 1001 }), 24);
            assert_eq!(rounded_framerate(VideoFramerate { num: 25, den: 1 }), 25);
        }

        #[test]
        fn converts_vaapi_vbr_rate_control_to_libva_parameters() {
            let rc = H264EncoderRateControl::VariableBitrate {
                average_bitrate: 6_000_000,
                max_bitrate: 8_000_000,
                virtual_buffer_size: Duration::from_secs(2),
            }
            .config();
            assert_eq!(rc.mode, VA_RC_VBR);
            assert_eq!(rc.bits_per_second, 8_000_000);
            assert_eq!(rc.target_percentage, 75);
            assert_eq!(rc.window_size, 2_000);
            assert!(rc.disable_bit_stuffing);
        }

        #[test]
        #[ignore = "requires a VA-API capable Linux host"]
        fn encodes_exported_nv12_dmabuf_frames_to_h264() {
            let _guard = VAAPI_TEST_LOCK.lock().unwrap();
            let (device, queue, adapter_info) = crate::test_wgpu_device_and_queue();
            let mut encoder = H264Encoder::new(H264EncoderConfig {
                adapter_info: Some(adapter_info),
                resolution: TEST_RESOLUTION,
                rate_control: H264EncoderRateControl::ConstantBitrate {
                    bitrate: 500_000,
                    virtual_buffer_size: Duration::from_millis(1_500),
                },
                gop_size: 30,
                framerate: TEST_FRAMERATE,
                max_pending_frames: MAX_PENDING_ENCODE_FRAMES,
            })
            .expect("failed to create VA-API H264 encoder");
            let mut frames = (0..2)
                .map(|_| {
                    crate::dmabuf::export_nv12_dmabuf_texture(&device, TEST_RESOLUTION)
                        .expect("failed to export NV12 DMA-BUF test texture")
                })
                .collect::<Vec<_>>();

            let mut encoded = Vec::new();
            encoded.extend(
                encoder
                    .encode(frames.remove(0), Duration::ZERO, true)
                    .expect("failed to encode VA-API keyframe"),
            );
            encoded.extend(
                encoder
                    .encode(frames.remove(0), Duration::from_millis(33), false)
                    .expect("failed to encode VA-API delta frame"),
            );
            encoded.extend(encoder.flush().expect("failed to flush VA-API encoder"));
            assert_eq!(encoded.len(), 2);
            let keyframe = &encoded[0];
            let delta = &encoded[1];

            assert!(keyframe.is_keyframe);
            assert!(contains_h264_nal(&keyframe.data, 7));
            assert!(contains_h264_nal(&keyframe.data, 8));
            assert!(contains_h264_nal(&keyframe.data, 5));
            assert!(!delta.is_keyframe);
            assert!(contains_h264_nal(&delta.data, 1));
        }

        #[test]
        #[ignore = "requires ffmpeg and a VA-API capable Linux host"]
        fn encodes_producer_synced_wgpu_writes_to_direct_nv12_dmabuf_input() {
            const FRAME_COUNT: usize = 24;
            const FRAME_POOL_SIZE: usize = 3;
            let _guard = VAAPI_TEST_LOCK.lock().unwrap();
            let (device, queue, adapter_info) = crate::test_wgpu_device_and_queue();
            let queue = Arc::new(queue);
            let mut encoder = H264Encoder::new(H264EncoderConfig {
                adapter_info: Some(adapter_info),
                resolution: TEST_RESOLUTION,
                rate_control: H264EncoderRateControl::ConstantBitrate {
                    bitrate: 2_000_000,
                    virtual_buffer_size: Duration::from_millis(1_500),
                },
                gop_size: 1,
                framerate: TEST_FRAMERATE,
                max_pending_frames: 1,
            })
            .expect("failed to create VA-API H264 encoder");
            let frames = (0..FRAME_POOL_SIZE)
                .map(|_| {
                    crate::dmabuf::export_nv12_dmabuf_texture(&device, TEST_RESOLUTION)
                        .expect("failed to export NV12 DMA-BUF test texture")
                })
                .collect::<Vec<_>>();

            let mut encoded = Vec::new();
            for index in 0..FRAME_COUNT {
                let frame = Arc::clone(&frames[index % frames.len()]);
                let luma = solid_luma_for_frame(index);
                write_solid_nv12_frame(&queue, frame.as_ref(), luma, 128, 128);
                queue.submit([]);
                device
                    .poll(wgpu::PollType::wait_indefinitely())
                    .expect("failed to wait for WGPU producer write");
                encoded.extend(
                    encoder
                        .encode(frame, frame_pts(index, TEST_FRAMERATE), true)
                        .expect("failed to encode VA-API frame after WGPU write"),
                );
            }
            encoded.extend(encoder.flush().expect("failed to flush VA-API encoder"));
            assert_eq!(encoded.len(), FRAME_COUNT);

            let bitstream = encoded
                .iter()
                .flat_map(|frame| frame.data.iter().copied())
                .collect::<Vec<_>>();
            let decoded = ffmpeg_decode_h264_to_nv12(&bitstream);
            let y_plane_len = (TEST_RESOLUTION.width * TEST_RESOLUTION.height) as usize;
            let frame_len = y_plane_len * 3 / 2;
            assert_eq!(decoded.len(), FRAME_COUNT * frame_len);
            for index in 0..FRAME_COUNT {
                let frame = &decoded[index * frame_len..][..frame_len];
                let actual_luma = average_luma(&frame[..y_plane_len]);
                let expected_luma = solid_luma_for_frame(index);
                assert!(
                    actual_luma.abs_diff(expected_luma) <= 12,
                    "decoded frame {index} luma {actual_luma} differs from expected {expected_luma}"
                );
            }
        }

        #[test]
        #[ignore = "requires a VA-API capable Linux host"]
        fn encodes_exported_nv12_dmabuf_frames_at_steady_30fps() {
            const FRAME_COUNT: usize = 120;
            let _guard = VAAPI_TEST_LOCK.lock().unwrap();
            let (device, queue, adapter_info) = crate::test_wgpu_device_and_queue();
            let mut encoder = H264Encoder::new(H264EncoderConfig {
                adapter_info: Some(adapter_info),
                resolution: STRESS_RESOLUTION,
                rate_control: H264EncoderRateControl::ConstantBitrate {
                    bitrate: 4_000_000,
                    virtual_buffer_size: Duration::from_millis(1_500),
                },
                gop_size: 30,
                framerate: TEST_FRAMERATE,
                max_pending_frames: MAX_PENDING_ENCODE_FRAMES,
            })
            .expect("failed to create VA-API H264 encoder");
            let frames = (0..MAX_PENDING_ENCODE_FRAMES + 1)
                .map(|_| {
                    crate::dmabuf::export_nv12_dmabuf_texture(&device, STRESS_RESOLUTION)
                        .expect("failed to export NV12 DMA-BUF stress texture")
                })
                .collect::<Vec<_>>();

            let mut encoded = Vec::new();
            let mut call_times = Vec::new();
            for index in 0..FRAME_COUNT {
                let started_at = Instant::now();
                encoded.extend(
                    encoder
                        .encode(
                            Arc::clone(&frames[index % frames.len()]),
                            Duration::from_micros(index as u64 * 1_000_000 / 30),
                            false,
                        )
                        .expect("failed to encode VA-API frame"),
                );
                let elapsed = started_at.elapsed();
                if index >= frames.len() {
                    call_times.push(elapsed);
                }
                if elapsed < Duration::from_millis(33) {
                    thread::sleep(Duration::from_millis(33) - elapsed);
                }
            }
            encoded.extend(encoder.flush().expect("failed to flush VA-API encoder"));

            let max_call_ms =
                call_times.iter().map(Duration::as_millis).max().unwrap_or_default();
            let keyframes = encoded.iter().filter(|frame| frame.is_keyframe).count();
            eprintln!(
                "encoded={}; keyframes={}; max_call_ms={}",
                encoded.len(),
                keyframes,
                max_call_ms
            );
            assert_eq!(encoded.len(), FRAME_COUNT);
            assert!(keyframes >= 4);
            assert!(max_call_ms < 40);
        }

        #[test]
        #[ignore = "requires a VA-API capable Linux host"]
        fn encodes_wt_preview_low_latency_without_stalls_or_memory_growth() {
            const FRAME_COUNT: usize = 1_800;
            const WARMUP_FRAMES: usize = 60;
            const WT_PREVIEW_BITRATE: u32 = 6_000_000;
            const WT_PREVIEW_GOP_SIZE: u16 = 30;
            const MAX_RSS_GROWTH_KIB: usize = 64 * 1024;
            let _guard = VAAPI_TEST_LOCK.lock().unwrap();
            let (device, queue, adapter_info) = crate::test_wgpu_device_and_queue();
            let mut encoder = H264Encoder::new(H264EncoderConfig {
                adapter_info: Some(adapter_info),
                resolution: STRESS_RESOLUTION,
                rate_control: H264EncoderRateControl::VariableBitrate {
                    average_bitrate: WT_PREVIEW_BITRATE,
                    max_bitrate: WT_PREVIEW_BITRATE * 4 / 3,
                    virtual_buffer_size: Duration::from_secs(2),
                },
                gop_size: WT_PREVIEW_GOP_SIZE,
                framerate: WT_PREVIEW_FRAMERATE,
                max_pending_frames: 1,
            })
            .expect("failed to create WT-preview VA-API H264 encoder");
            let frames = (0..3)
                .map(|_| {
                    crate::dmabuf::export_nv12_dmabuf_texture(&device, STRESS_RESOLUTION)
                        .expect("failed to export NV12 DMA-BUF WT preview texture")
                })
                .collect::<Vec<_>>();

            let interval = WT_PREVIEW_FRAMERATE.get_interval_duration();
            let mut encoded_frames = 0;
            let mut keyframes = 0;
            let mut encoded_bytes = 0usize;
            let mut call_times = Vec::new();
            let mut rss_after_warmup = None;
            let mut peak_rss_kib = 0;
            for index in 0..FRAME_COUNT {
                let started_at = Instant::now();
                let encoded = encoder
                    .encode(
                        Arc::clone(&frames[index % frames.len()]),
                        frame_pts(index, WT_PREVIEW_FRAMERATE),
                        false,
                    )
                    .expect("failed to encode WT-preview VA-API frame");
                observe_encoded_frames(
                    encoded,
                    &mut encoded_frames,
                    &mut keyframes,
                    &mut encoded_bytes,
                );
                let elapsed = started_at.elapsed();
                if index >= WARMUP_FRAMES {
                    call_times.push(elapsed);
                    let rss = current_rss_kib();
                    rss_after_warmup.get_or_insert(rss);
                    peak_rss_kib = peak_rss_kib.max(rss);
                }
                if elapsed < interval {
                    thread::sleep(interval - elapsed);
                }
            }
            observe_encoded_frames(
                encoder.flush().expect("failed to flush WT-preview VA-API encoder"),
                &mut encoded_frames,
                &mut keyframes,
                &mut encoded_bytes,
            );

            call_times.sort_unstable();
            let max_call = call_times.iter().copied().max().unwrap_or_default();
            let p99_call = percentile_duration(&call_times, 99);
            let rss_growth_kib =
                peak_rss_kib.saturating_sub(rss_after_warmup.unwrap_or(peak_rss_kib));
            let padded_cbr_bytes = padded_cbr_budget_bytes(
                WT_PREVIEW_BITRATE,
                WT_PREVIEW_FRAMERATE,
                FRAME_COUNT,
            );
            eprintln!(
                "wt_low_latency frames={encoded_frames}; keyframes={keyframes}; bytes={encoded_bytes}; padded_cbr_bytes={padded_cbr_bytes}; max_call_ms={}; p99_call_ms={}; rss_after_warmup_kib={}; peak_rss_kib={peak_rss_kib}; rss_growth_kib={rss_growth_kib}",
                max_call.as_millis(),
                p99_call.as_millis(),
                rss_after_warmup.unwrap_or(0),
            );

            assert_eq!(encoded_frames, FRAME_COUNT);
            assert!(keyframes >= 55);
            assert!(encoded_bytes < padded_cbr_bytes * 9 / 10);
            assert!(p99_call < Duration::from_millis(15));
            assert!(max_call < Duration::from_millis(33));
            assert!(rss_growth_kib < MAX_RSS_GROWTH_KIB);
        }

        fn contains_h264_nal(data: &[u8], nal_type: u8) -> bool {
            data.windows(5)
                .any(|window| window[..4] == [0, 0, 0, 1] && window[4] & 0x1f == nal_type)
                || data.windows(4).any(|window| {
                    window[..3] == [0, 0, 1] && window[3] & 0x1f == nal_type
                })
        }

        fn observe_encoded_frames(
            frames: Vec<EncodedOutputChunk<Bytes>>,
            encoded_frames: &mut usize,
            keyframes: &mut usize,
            encoded_bytes: &mut usize,
        ) {
            for frame in frames {
                *encoded_frames += 1;
                *keyframes += frame.is_keyframe as usize;
                *encoded_bytes += frame.data.len();
            }
        }

        fn write_solid_nv12_frame(
            queue: &wgpu::Queue,
            frame: &DmaBufFrame,
            y: u8,
            u: u8,
            v: u8,
        ) {
            let width = frame.width();
            let height = frame.height();
            let y_plane = vec![y; (width * height) as usize];
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: frame.texture(),
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::Plane0,
                },
                &y_plane,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(width),
                    rows_per_image: Some(height),
                },
                wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            );

            let uv_width = width / 2;
            let uv_height = height / 2;
            let mut uv_plane = vec![0; (uv_width * uv_height * 2) as usize];
            for pixel in uv_plane.chunks_exact_mut(2) {
                pixel[0] = u;
                pixel[1] = v;
            }
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: frame.texture(),
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::Plane1,
                },
                &uv_plane,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(width),
                    rows_per_image: Some(uv_height),
                },
                wgpu::Extent3d {
                    width: uv_width,
                    height: uv_height,
                    depth_or_array_layers: 1,
                },
            );
        }

        fn solid_luma_for_frame(index: usize) -> u8 {
            32 + ((index * 41) % 192) as u8
        }

        fn average_luma(y_plane: &[u8]) -> u8 {
            (y_plane.iter().map(|value| u64::from(*value)).sum::<u64>()
                / y_plane.len() as u64) as u8
        }

        fn ffmpeg_decode_h264_to_nv12(bitstream: &[u8]) -> Vec<u8> {
            let mut child = Command::new("ffmpeg")
                .args([
                    "-v", "error", "-f", "h264", "-i", "pipe:0", "-f", "rawvideo",
                    "-pix_fmt", "nv12", "pipe:1",
                ])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("failed to spawn ffmpeg");
            child
                .stdin
                .as_mut()
                .expect("missing ffmpeg stdin")
                .write_all(bitstream)
                .expect("failed to write H264 bitstream to ffmpeg");
            let output = child.wait_with_output().expect("failed to wait for ffmpeg");
            assert!(
                output.status.success(),
                "ffmpeg decode failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            output.stdout
        }

        fn frame_pts(index: usize, framerate: VideoFramerate) -> Duration {
            Duration::from_nanos(
                index as u64 * 1_000_000_000u64 * framerate.den as u64
                    / framerate.num as u64,
            )
        }

        fn percentile_duration(values: &[Duration], percentile: usize) -> Duration {
            if values.is_empty() {
                return Duration::ZERO;
            }
            let index = (values.len() - 1) * percentile / 100;
            values[index]
        }

        fn padded_cbr_budget_bytes(
            bitrate: u32,
            framerate: VideoFramerate,
            frame_count: usize,
        ) -> usize {
            let bytes = u128::from(bitrate)
                * u128::from(framerate.den.max(1))
                * frame_count as u128
                / (8 * u128::from(framerate.num.max(1)));
            bytes.min(usize::MAX as u128) as usize
        }

        fn current_rss_kib() -> usize {
            fs::read_to_string("/proc/self/status")
                .ok()
                .and_then(|status| {
                    status.lines().find_map(|line| {
                        let value = line.strip_prefix("VmRSS:")?;
                        value.split_whitespace().next()?.parse().ok()
                    })
                })
                .unwrap_or(0)
        }
    }
}

pub use imp::{
    H264EncoderConfig, H264EncoderRateControl, VaapiH264EncoderError, WgpuTexturesEncoderH264,
};
