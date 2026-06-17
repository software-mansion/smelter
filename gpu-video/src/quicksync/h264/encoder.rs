use std::{
    alloc::{Layout, alloc_zeroed, dealloc, handle_alloc_error},
    collections::{HashMap, VecDeque},
    num::{NonZeroU16, NonZeroU64},
    ptr::NonNull,
    sync::Arc,
    time::Duration,
};

use crate::quicksync::sys as vpl;
use bytes::{Bytes, BytesMut};
use tracing::info;

use crate::{
    InputFrame, VideoFramerate, VideoResolution,
    device::CodecColorDescription,
    dmabuf::QuickSyncDmaBufSync,
    parameters::{ColorRange, ColorSpace},
    quicksync::{
        h264::{
            H264Session, H264SessionError, ImportedRgbaSurface, VplSyncQueue,
            init_dmabuf_sync, progressive_frame_info, retry_device_busy,
            vpl_u16_dimension,
        },
        vpl::{Component, FrameSurface, Session, SyncWait, check_status_allow_warnings},
    },
};

const H264_DIMENSION_ALIGNMENT: u32 = 16;
const MIN_H264_BITSTREAM_BUFFER_SIZE: u32 = 1_500_000;
const H264_ENCODER_MAX_DIMENSION: u32 =
    u16::MAX as u32 / H264_DIMENSION_ALIGNMENT * H264_DIMENSION_ALIGNMENT;
const H264_VIDEO_FORMAT_UNSPECIFIED: u16 = 5;
const QUICKSYNC_ENCODER_ASYNC_DEPTH: u16 = 1;

pub struct WgpuTexturesEncoderH264 {
    input_pool: VecDeque<EncodeInputSurface>,
    encoder: QuickSyncH264Encoder,
    sync: QuickSyncDmaBufSync,
    device: Arc<wgpu::Device>,
    resolution: VideoResolution,
    rgba_to_rgb4: RgbaToRgb4Renderer,
}

pub struct H264EncodedOutputChunk<T> {
    pub data: T,
    pub pts: u64,
    pub is_keyframe: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct H264EncoderConfig<'a> {
    pub adapter_info: &'a wgpu::AdapterInfo,
    pub resolution: VideoResolution,
    pub rate_control: H264EncoderRateControl,
    pub gop_size: NonZeroU16,
    pub framerate: VideoFramerate,
    pub max_pending_frames: usize,
    pub preset: H264EncoderPreset,
    pub color_space: ColorSpace,
    pub color_range: ColorRange,
}

#[derive(Debug, Clone, Copy, Default)]
pub enum H264EncoderPreset {
    HighQuality,
    #[default]
    LowLatency,
}

#[derive(Debug, Clone, Copy)]
pub enum H264EncoderRateControl {
    VariableBitrate { bitrate: H264VariableBitrate, virtual_buffer_size: Duration },
    ConstantBitrate { bitrate: NonZeroU64, virtual_buffer_size: Duration },
}

#[derive(Debug, Clone, Copy)]
pub struct H264VariableBitrate {
    average_bitrate: NonZeroU64,
    max_bitrate: NonZeroU64,
}

impl H264VariableBitrate {
    pub fn new(
        average_bitrate: NonZeroU64,
        max_bitrate: NonZeroU64,
    ) -> Result<Self, H264RateControlError> {
        if max_bitrate < average_bitrate {
            return Err(H264RateControlError::MaxBelowAverage {
                average_bitrate,
                max_bitrate,
            });
        }
        Ok(Self { average_bitrate, max_bitrate })
    }
}

#[derive(Debug, Clone, Copy, thiserror::Error)]
pub enum H264RateControlError {
    #[error(
        "Intel Quick Sync H264 max bitrate {max_bitrate} must be at least average bitrate {average_bitrate}"
    )]
    MaxBelowAverage { average_bitrate: NonZeroU64, max_bitrate: NonZeroU64 },
}

#[derive(Debug, thiserror::Error)]
pub enum QuickSyncH264EncoderError {
    #[error("Intel Quick Sync H264 encoder is unavailable: {0}")]
    Unavailable(#[from] H264SessionError),

    #[error("Intel Quick Sync H264 encode error: {0}")]
    Encode(String),

    #[error(
        "Intel Quick Sync H264 encoder retained every input surface without producing output"
    )]
    InputSurfacePoolExhausted,

    #[error("Intel Quick Sync H264 encoder requires input frames to carry PTS")]
    MissingPts,

    #[error("Intel Quick Sync H264 encoder requires COPY_SRC texture usage, got {0:?}")]
    NoCopySrcTextureUsage(wgpu::TextureUsages),

    #[error("Intel Quick Sync H264 encoder requires RGBA textures, got {0:?}")]
    UnsupportedInputTexture(wgpu::TextureFormat),

    #[error(
        "Intel Quick Sync H264 encoder expected texture size {expected:?}, got {provided:?}"
    )]
    InconsistentTextureSize { expected: wgpu::Extent3d, provided: wgpu::Extent3d },

    #[error("Intel Quick Sync H264 encoder requires non-zero resolution, got {0:?}")]
    ZeroResolution(VideoResolution),

    #[error("Intel Quick Sync H264 encoder requires even resolution, got {0:?}")]
    OddResolution(VideoResolution),

    #[error(
        "Intel Quick Sync H264 encoder requires resolution dimensions no larger than {max}, got {resolution:?}"
    )]
    ResolutionTooLarge { resolution: VideoResolution, max: u32 },

    #[error(
        "Intel Quick Sync H264 encoder bitstream buffer for coded resolution {resolution:?} exceeds oneVPL limit {max_bytes} bytes"
    )]
    BitstreamBufferTooLarge { resolution: VideoResolution, max_bytes: u32 },
}

impl WgpuTexturesEncoderH264 {
    pub fn new(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        config: H264EncoderConfig<'_>,
    ) -> Result<Self, QuickSyncH264EncoderError> {
        let layout = h264_encoder_layout(config.resolution)?;
        info!("Initializing Intel Quick Sync H264 encoder");
        let sync = init_dmabuf_sync(&device, &queue)?;

        let resolution = layout.visible;
        let pool_size = (config.max_pending_frames + 2)
            .max(usize::from(QUICKSYNC_ENCODER_ASYNC_DEPTH) + 1);
        let mut encoder = QuickSyncH264Encoder::new(config, layout)?;
        let input_pool = (0..pool_size)
            .map(|_| encoder.create_input_surface(&device))
            .collect::<Result<VecDeque<_>, _>>()
            .map_err(QuickSyncH264EncoderError::Encode)?;
        let rgba_to_rgb4 = RgbaToRgb4Renderer::new(&device);

        info!(
            width = resolution.width,
            height = resolution.height,
            coded_width = layout.coded.width,
            coded_height = layout.coded.height,
            pool_size,
            "Initialized Intel Quick Sync H264 encoder"
        );

        Ok(Self { input_pool, encoder, sync, device, resolution, rgba_to_rgb4 })
    }

    pub fn parameter_sets(&self) -> &Bytes {
        &self.encoder.parameter_sets
    }

    pub fn encode(
        &mut self,
        frame: InputFrame<wgpu::Texture>,
        force_keyframe: bool,
    ) -> Result<Vec<H264EncodedOutputChunk<Bytes>>, QuickSyncH264EncoderError> {
        self.validate_input_texture(&frame.data)?;
        let pts = frame.pts.ok_or(QuickSyncH264EncoderError::MissingPts)?;

        let mut chunks = self.retire_completed(SyncWait::Poll)?;
        if self.encoder.is_full() || self.input_pool.is_empty() {
            chunks.extend(self.retire_completed(SyncWait::Block)?);
        }

        let input = self
            .input_pool
            .pop_front()
            .ok_or(QuickSyncH264EncoderError::InputSurfacePoolExhausted)?;
        self.copy_input_to_surface(&frame.data, &input)?;
        self.encoder
            .encode(input, pts, force_keyframe)
            .map_err(QuickSyncH264EncoderError::Encode)?;
        chunks.extend(self.retire_completed(SyncWait::Poll)?);
        Ok(chunks)
    }

    pub fn flush(
        &mut self,
    ) -> Result<Vec<H264EncodedOutputChunk<Bytes>>, QuickSyncH264EncoderError> {
        let completed =
            self.encoder.flush().map_err(QuickSyncH264EncoderError::Encode)?;
        Ok(self.collect_completed(completed))
    }

    pub fn poll_output(
        &mut self,
    ) -> Result<Vec<H264EncodedOutputChunk<Bytes>>, QuickSyncH264EncoderError> {
        self.retire_completed(SyncWait::Poll)
    }

    fn validate_input_texture(
        &self,
        texture: &wgpu::Texture,
    ) -> Result<(), QuickSyncH264EncoderError> {
        let expected = self.resolution.extent_2d();
        if !texture.usage().contains(wgpu::TextureUsages::COPY_SRC) {
            return Err(QuickSyncH264EncoderError::NoCopySrcTextureUsage(
                texture.usage(),
            ));
        }
        if texture.format() != wgpu::TextureFormat::Rgba8Unorm {
            return Err(QuickSyncH264EncoderError::UnsupportedInputTexture(
                texture.format(),
            ));
        }
        if texture.size() != expected {
            return Err(QuickSyncH264EncoderError::InconsistentTextureSize {
                expected,
                provided: texture.size(),
            });
        }
        Ok(())
    }

    fn copy_input_to_surface(
        &self,
        texture: &wgpu::Texture,
        input: &EncodeInputSurface,
    ) -> Result<(), QuickSyncH264EncoderError> {
        let mut encoder =
            self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Intel Quick Sync H264 input copy"),
            });
        self.rgba_to_rgb4.render(&mut encoder, texture, input.imported.frame.texture());
        self.sync
            .submit_frame_write(
                input.imported.frame.sync(),
                encoder,
                "Intel Quick Sync H264 RGBA to RGB4 input render",
            )
            .map_err(|err| QuickSyncH264EncoderError::Encode(err.to_string()))?;
        Ok(())
    }

    fn retire_completed(
        &mut self,
        wait: SyncWait,
    ) -> Result<Vec<H264EncodedOutputChunk<Bytes>>, QuickSyncH264EncoderError> {
        let completed = self
            .encoder
            .drain_completed(wait)
            .map_err(QuickSyncH264EncoderError::Encode)?;
        Ok(self.collect_completed(completed))
    }

    fn collect_completed(
        &mut self,
        completed: Vec<EncodeCompletion>,
    ) -> Vec<H264EncodedOutputChunk<Bytes>> {
        let mut chunks = Vec::with_capacity(completed.len());
        for completion in completed {
            self.input_pool.push_back(completion.input);
            chunks.push(completion.chunk);
        }
        chunks
    }
}

impl Drop for WgpuTexturesEncoderH264 {
    fn drop(&mut self) {
        self.input_pool.clear();
        let _ = self.encoder.flush();
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
    }
}

struct QuickSyncH264Encoder {
    quicksync: H264Session,
    parameter_sets: Bytes,
    frame_index: u64,
    bitstream_buffer_size: u32,
    pending_bitstreams: VplSyncQueue<Box<OutputBitstream>>,
    pending_frames: HashMap<u64, PendingFrame>,
}

impl QuickSyncH264Encoder {
    fn new(
        config: H264EncoderConfig<'_>,
        layout: H264EncoderLayout,
    ) -> Result<Self, QuickSyncH264EncoderError> {
        let quicksync = H264Session::new(config.adapter_info, Component::Encode)?;
        let mut video_param = encoder_video_param(&config, layout)
            .map_err(QuickSyncH264EncoderError::Encode)?;
        video_param
            .with_ext_params(|video_param| {
                check_status_allow_warnings("MFXVideoENCODE_Init", unsafe {
                    vpl::MFXVideoENCODE_Init(quicksync.session.raw(), video_param)
                })
            })
            .map_err(|err| QuickSyncH264EncoderError::Encode(err.to_string()))?;
        let parameter_sets = get_parameter_sets(&quicksync.session, &mut video_param)
            .map_err(QuickSyncH264EncoderError::Encode)?;

        Ok(Self {
            quicksync,
            parameter_sets,
            frame_index: 0,
            bitstream_buffer_size: layout.bitstream_buffer_size,
            pending_bitstreams: VplSyncQueue::new(usize::from(
                QUICKSYNC_ENCODER_ASYNC_DEPTH,
            )),
            pending_frames: HashMap::new(),
        })
    }

    fn create_input_surface(
        &mut self,
        device: &wgpu::Device,
    ) -> Result<EncodeInputSurface, String> {
        let surface = self
            .quicksync
            .session
            .get_surface_for_encode()
            .map_err(|err| err.to_string())?;
        let imported = self
            .quicksync
            .import_rgb4_surface(
                device,
                &surface,
                wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                wgpu::TextureUses::COLOR_TARGET,
            )
            .map_err(|err| err.to_string())?;
        Ok(EncodeInputSurface { imported, surface })
    }

    fn encode(
        &mut self,
        mut input: EncodeInputSurface,
        pts: u64,
        force_keyframe: bool,
    ) -> Result<(), String> {
        let frame_index = self.frame_index;
        input.surface.set_timestamp(frame_index);

        self.submit_bitstream(EncodeSubmit::Input {
            force_keyframe,
            surface: &input.surface,
        })?;
        self.pending_frames.insert(
            frame_index,
            PendingFrame { pts, forced_keyframe: force_keyframe, input },
        );
        self.frame_index += 1;
        Ok(())
    }

    fn flush(&mut self) -> Result<Vec<EncodeCompletion>, String> {
        let mut completed = self.drain_completed(SyncWait::Poll)?;
        loop {
            if self.is_full() {
                completed.extend(self.drain_completed(SyncWait::Block)?);
            }
            if self.submit_bitstream(EncodeSubmit::Drain)?
                == EncodeAsyncStatus::NeedsMoreData
            {
                break;
            }
            completed.extend(self.drain_completed(SyncWait::Poll)?);
        }
        completed.extend(self.drain_all_completed()?);
        if !self.pending_frames.is_empty() {
            return Err(format!(
                "Intel Quick Sync H264 encoder left {} frames pending after flush",
                self.pending_frames.len()
            ));
        }
        Ok(completed)
    }

    fn is_full(&self) -> bool {
        self.pending_bitstreams.is_full()
    }

    fn submit_bitstream(
        &mut self,
        submit: EncodeSubmit<'_>,
    ) -> Result<EncodeAsyncStatus, String> {
        let mut output = Box::new(OutputBitstream::new(self.bitstream_buffer_size));
        let status =
            encode_frame_async(&self.quicksync.session, submit, &mut output.bitstream)?;
        if let EncodeAsyncStatus::Submitted(syncp) = status {
            self.pending_bitstreams.push(syncp, output);
        }
        Ok(status)
    }

    fn drain_completed(
        &mut self,
        wait: SyncWait,
    ) -> Result<Vec<EncodeCompletion>, String> {
        let quicksync = &self.quicksync;
        let parameter_sets = &self.parameter_sets;
        let pending_frames = &mut self.pending_frames;
        self.pending_bitstreams.drain_completed(quicksync, wait, |output| {
            complete_bitstream(output, parameter_sets, pending_frames)
        })
    }

    fn drain_all_completed(&mut self) -> Result<Vec<EncodeCompletion>, String> {
        let quicksync = &self.quicksync;
        let parameter_sets = &self.parameter_sets;
        let pending_frames = &mut self.pending_frames;
        self.pending_bitstreams.drain_all_completed(quicksync, |output| {
            complete_bitstream(output, parameter_sets, pending_frames)
        })
    }
}

impl Drop for QuickSyncH264Encoder {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

struct EncodeInputSurface {
    imported: ImportedRgbaSurface,
    surface: FrameSurface,
}

struct OutputBitstream {
    bitstream: vpl::mfxBitstream,
    buffer: BitstreamBuffer,
}

impl OutputBitstream {
    fn new(size: u32) -> Self {
        let mut buffer = BitstreamBuffer::new(size);
        let mut bitstream = unsafe { std::mem::zeroed::<vpl::mfxBitstream>() };
        bitstream.Data = buffer.as_mut_ptr();
        bitstream.MaxLength = size;
        Self { bitstream, buffer }
    }
}

struct BitstreamBuffer {
    ptr: NonNull<u8>,
    layout: Layout,
    len: usize,
}

impl BitstreamBuffer {
    fn new(len: u32) -> Self {
        let len = len as usize;
        let layout =
            Layout::from_size_align(len.max(1), 64).expect("valid bitstream layout");
        let ptr = NonNull::new(unsafe { alloc_zeroed(layout) }).unwrap_or_else(|| {
            handle_alloc_error(layout);
        });
        Self { ptr, layout, len }
    }

    fn as_mut_ptr(&mut self) -> *mut u8 {
        self.ptr.as_ptr()
    }

    fn as_slice(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }
}

impl Drop for BitstreamBuffer {
    fn drop(&mut self) {
        unsafe {
            dealloc(self.ptr.as_ptr(), self.layout);
        }
    }
}

struct PendingFrame {
    pts: u64,
    forced_keyframe: bool,
    input: EncodeInputSurface,
}

struct EncodeCompletion {
    chunk: H264EncodedOutputChunk<Bytes>,
    input: EncodeInputSurface,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct H264EncoderLayout {
    visible: VideoResolution,
    coded: VideoResolution,
    bitstream_buffer_size: u32,
}

enum EncodeSubmit<'a> {
    Input { force_keyframe: bool, surface: &'a FrameSurface },
    Drain,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EncodeAsyncStatus {
    Submitted(vpl::mfxSyncPoint),
    NeedsMoreData,
}

fn complete_bitstream(
    output: Box<OutputBitstream>,
    parameter_sets: &Bytes,
    pending_frames: &mut HashMap<u64, PendingFrame>,
) -> Result<EncodeCompletion, String> {
    let frame_index = output.bitstream.TimeStamp;
    let pending_frame = pending_frames.remove(&frame_index).ok_or_else(|| {
        format!(
            "Intel Quick Sync returned H264 bitstream for unknown frame {frame_index}"
        )
    })?;
    Ok(EncodeCompletion {
        chunk: output_chunk(
            parameter_sets,
            &output,
            pending_frame.pts,
            pending_frame.forced_keyframe,
        )?,
        input: pending_frame.input,
    })
}

struct RgbaToRgb4Renderer {
    label: &'static str,
    device: wgpu::Device,
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
}

impl RgbaToRgb4Renderer {
    fn new(device: &wgpu::Device) -> Self {
        let label = "Intel Quick Sync RGBA to RGB4 input render";
        let shader =
            device.create_shader_module(wgpu::include_wgsl!("../shaders/rgba_to_rgb4.wgsl"));
        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some(label),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                }],
            });
        let pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: &[Some(&bind_group_layout)],
                immediate_size: 0,
            });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(&pipeline_layout),
            cache: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::TextureFormat::Bgra8Unorm.into())],
            }),
            primitive: wgpu::PrimitiveState::default(),
            multiview_mask: None,
            multisample: wgpu::MultisampleState::default(),
            depth_stencil: None,
        });
        Self { label, device: device.clone(), pipeline, bind_group_layout }
    }

    fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        input: &wgpu::Texture,
        output: &wgpu::Texture,
    ) {
        let input_view = input.create_view(&Default::default());
        let output_view = output.create_view(&Default::default());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(self.label),
            layout: &self.bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&input_view),
            }],
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(self.label),
            timestamp_writes: None,
            occlusion_query_set: None,
            depth_stencil_attachment: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &output_view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

fn encoder_video_param(
    config: &H264EncoderConfig<'_>,
    layout: H264EncoderLayout,
) -> Result<H264EncoderVideoParam, String> {
    let mut frame_info =
        progressive_frame_info(vpl::MFX_FOURCC_RGB4, vpl::MFX_CHROMAFORMAT_YUV444 as u16);
    frame_info.BitDepthLuma = 8;
    frame_info.BitDepthChroma = 8;
    frame_info.FrameRateExtN = config.framerate.num.get();
    frame_info.FrameRateExtD = config.framerate.den.get();
    unsafe {
        let dims = &mut frame_info.__bindgen_anon_1.__bindgen_anon_1;
        dims.Width = vpl_u16_dimension("coded width", layout.coded.width)?;
        dims.Height = vpl_u16_dimension("coded height", layout.coded.height)?;
        dims.CropW = vpl_u16_dimension("crop width", layout.visible.width)?;
        dims.CropH = vpl_u16_dimension("crop height", layout.visible.height)?;
    }

    let mut inner = unsafe { std::mem::zeroed::<vpl::mfxVideoParam>() };
    inner.IOPattern = vpl::MFX_IOPATTERN_IN_VIDEO_MEMORY as u16;
    inner.AsyncDepth = QUICKSYNC_ENCODER_ASYNC_DEPTH;
    unsafe {
        let mfx = &mut inner.__bindgen_anon_1.mfx;
        mfx.FrameInfo = frame_info;
        mfx.CodecId = vpl::MFX_CODEC_AVC;
        mfx.CodecProfile = vpl::MFX_PROFILE_AVC_MAIN as u16;
        mfx.CodecLevel = vpl::MFX_LEVEL_AVC_4 as u16;
        mfx.LowPower = vpl::MFX_CODINGOPTION_ON as u16;
        mfx.__bindgen_anon_1.__bindgen_anon_2.MaxDecFrameBuffering = 1;

        let enc = &mut mfx.__bindgen_anon_1.__bindgen_anon_1;
        enc.TargetUsage = config.preset.target_usage();
        enc.GopPicSize = config.gop_size.get();
        enc.GopRefDist = 1;
        enc.GopOptFlag = vpl::MFX_GOP_STRICT as u16;
        enc.IdrInterval = 0;
        enc.NumRefFrame = 1;
        enc.EncodedOrder = 0;

        match config.rate_control {
            H264EncoderRateControl::VariableBitrate { bitrate, virtual_buffer_size } => {
                let average_bitrate = bitrate.average_bitrate.get();
                let max_bitrate = bitrate.max_bitrate.get();
                let buffer_size = virtual_buffer_kb(max_bitrate, virtual_buffer_size);
                enc.RateControlMethod = vpl::MFX_RATECONTROL_VBR as u16;
                enc.__bindgen_anon_1.InitialDelayInKB = buffer_size;
                enc.BufferSizeInKB = buffer_size;
                enc.__bindgen_anon_2.TargetKbps = bitrate_kbps(average_bitrate);
                enc.__bindgen_anon_3.MaxKbps = bitrate_kbps(max_bitrate);
            }
            H264EncoderRateControl::ConstantBitrate { bitrate, virtual_buffer_size } => {
                let bitrate = bitrate.get();
                let buffer_size = virtual_buffer_kb(bitrate, virtual_buffer_size);
                enc.RateControlMethod = vpl::MFX_RATECONTROL_CBR as u16;
                enc.__bindgen_anon_1.InitialDelayInKB = buffer_size;
                enc.BufferSizeInKB = buffer_size;
                enc.__bindgen_anon_2.TargetKbps = bitrate_kbps(bitrate);
            }
        }
    }
    Ok(H264EncoderVideoParam {
        inner,
        coding_option: coding_option(),
        coding_option2: coding_option2(),
        coding_option3: coding_option3(),
        video_signal_info: video_signal_info(config.color_space, config.color_range),
    })
}

struct H264EncoderVideoParam {
    inner: vpl::mfxVideoParam,
    coding_option: vpl::mfxExtCodingOption,
    coding_option2: vpl::mfxExtCodingOption2,
    coding_option3: vpl::mfxExtCodingOption3,
    video_signal_info: vpl::mfxExtVideoSignalInfo,
}

impl H264EncoderVideoParam {
    fn with_ext_params<R>(
        &mut self,
        call: impl FnOnce(*mut vpl::mfxVideoParam) -> R,
    ) -> R {
        let mut ext = [
            &mut self.coding_option.Header as *mut vpl::mfxExtBuffer,
            &mut self.coding_option2.Header as *mut vpl::mfxExtBuffer,
            &mut self.coding_option3.Header as *mut vpl::mfxExtBuffer,
            &mut self.video_signal_info.Header as *mut vpl::mfxExtBuffer,
        ];
        self.with_ext_buffers(&mut ext, call)
    }

    fn with_ext_buffers<R>(
        &mut self,
        ext: &mut [*mut vpl::mfxExtBuffer],
        call: impl FnOnce(*mut vpl::mfxVideoParam) -> R,
    ) -> R {
        self.inner.ExtParam = ext.as_mut_ptr();
        self.inner.NumExtParam = ext.len() as u16;
        call(&mut self.inner)
    }
}

impl H264EncoderPreset {
    fn target_usage(self) -> u16 {
        match self {
            Self::HighQuality => vpl::MFX_TARGETUSAGE_BEST_QUALITY as u16,
            Self::LowLatency => vpl::MFX_TARGETUSAGE_BEST_SPEED as u16,
        }
    }
}

fn coding_option() -> vpl::mfxExtCodingOption {
    let mut option = unsafe { std::mem::zeroed::<vpl::mfxExtCodingOption>() };
    option.Header.BufferId = vpl::MFX_EXTBUFF_CODING_OPTION;
    option.Header.BufferSz = std::mem::size_of::<vpl::mfxExtCodingOption>() as u32;
    option.MaxDecFrameBuffering = 1;
    option.NalHrdConformance = vpl::MFX_CODINGOPTION_ON as u16;
    option.VuiNalHrdParameters = vpl::MFX_CODINGOPTION_ON as u16;
    option
}

fn coding_option2() -> vpl::mfxExtCodingOption2 {
    let mut option = unsafe { std::mem::zeroed::<vpl::mfxExtCodingOption2>() };
    option.Header.BufferId = vpl::MFX_EXTBUFF_CODING_OPTION2;
    option.Header.BufferSz = std::mem::size_of::<vpl::mfxExtCodingOption2>() as u32;
    option.BRefType = vpl::MFX_B_REF_OFF as u16;
    option.AdaptiveI = vpl::MFX_CODINGOPTION_OFF as u16;
    option.AdaptiveB = vpl::MFX_CODINGOPTION_OFF as u16;
    option.LookAheadDepth = 0;
    option
}

fn coding_option3() -> vpl::mfxExtCodingOption3 {
    let mut option = unsafe { std::mem::zeroed::<vpl::mfxExtCodingOption3>() };
    option.Header.BufferId = vpl::MFX_EXTBUFF_CODING_OPTION3;
    option.Header.BufferSz = std::mem::size_of::<vpl::mfxExtCodingOption3>() as u32;
    option.LowDelayHrd = vpl::MFX_CODINGOPTION_ON as u16;
    option.LowDelayBRC = vpl::MFX_CODINGOPTION_ON as u16;
    option.PRefType = vpl::MFX_P_REF_SIMPLE as u16;
    option.ScenarioInfo = vpl::MFX_SCENARIO_DISPLAY_REMOTING as u16;
    option.ContentInfo = vpl::MFX_CONTENT_NON_VIDEO_SCREEN as u16;
    option.NumRefActiveP[0] = 1;
    option
}

fn video_signal_info(
    color_space: ColorSpace,
    color_range: ColorRange,
) -> vpl::mfxExtVideoSignalInfo {
    let mut info = unsafe { std::mem::zeroed::<vpl::mfxExtVideoSignalInfo>() };
    let description = CodecColorDescription::from(color_space);
    info.Header.BufferId = vpl::MFX_EXTBUFF_VIDEO_SIGNAL_INFO;
    info.Header.BufferSz = std::mem::size_of::<vpl::mfxExtVideoSignalInfo>() as u32;
    info.VideoFormat = H264_VIDEO_FORMAT_UNSPECIFIED;
    info.VideoFullRange = u16::from(color_range == ColorRange::Full);
    info.ColourDescriptionPresent = u16::from(color_space != ColorSpace::Unspecified);
    info.ColourPrimaries = u16::from(description.colour_primaries);
    info.TransferCharacteristics = u16::from(description.transfer_characteristics);
    info.MatrixCoefficients = u16::from(description.matrix_coefficients);
    info
}

fn get_parameter_sets(
    session: &Session,
    video_param: &mut H264EncoderVideoParam,
) -> Result<Bytes, String> {
    let mut sps = vec![0; 512];
    let mut pps = vec![0; 256];
    let mut sps_pps = unsafe { std::mem::zeroed::<vpl::mfxExtCodingOptionSPSPPS>() };
    sps_pps.Header.BufferId = vpl::MFX_EXTBUFF_CODING_OPTION_SPSPPS;
    sps_pps.Header.BufferSz = std::mem::size_of::<vpl::mfxExtCodingOptionSPSPPS>() as u32;
    sps_pps.SPSBuffer = sps.as_mut_ptr();
    sps_pps.PPSBuffer = pps.as_mut_ptr();
    sps_pps.SPSBufSize = sps.len() as u16;
    sps_pps.PPSBufSize = pps.len() as u16;
    let mut ext = [
        &mut sps_pps.Header as *mut vpl::mfxExtBuffer,
        &mut video_param.video_signal_info.Header as *mut vpl::mfxExtBuffer,
    ];
    video_param
        .with_ext_buffers(&mut ext, |video_param| {
            check_status_allow_warnings("MFXVideoENCODE_GetVideoParam", unsafe {
                vpl::MFXVideoENCODE_GetVideoParam(session.raw(), video_param)
            })
        })
        .map_err(|err| err.to_string())?;
    let mut out = BytesMut::with_capacity(
        sps_pps.SPSBufSize as usize + sps_pps.PPSBufSize as usize + 8,
    );
    out.extend_from_slice(&[0, 0, 0, 1]);
    out.extend_from_slice(&sps[..sps_pps.SPSBufSize as usize]);
    out.extend_from_slice(&[0, 0, 0, 1]);
    out.extend_from_slice(&pps[..sps_pps.PPSBufSize as usize]);
    Ok(out.freeze())
}

fn encode_frame_async(
    session: &Session,
    submit: EncodeSubmit<'_>,
    bitstream: &mut vpl::mfxBitstream,
) -> Result<EncodeAsyncStatus, String> {
    let mut ctrl = unsafe { std::mem::zeroed::<vpl::mfxEncodeCtrl>() };
    let (ctrl, surface) = match submit {
        EncodeSubmit::Input { force_keyframe, surface } => {
            let ctrl = if force_keyframe {
                ctrl.FrameType = (vpl::MFX_FRAMETYPE_IDR
                    | vpl::MFX_FRAMETYPE_I
                    | vpl::MFX_FRAMETYPE_REF) as u16;
                &mut ctrl as *mut _
            } else {
                std::ptr::null_mut()
            };
            (ctrl, surface.raw())
        }
        EncodeSubmit::Drain => (std::ptr::null_mut(), std::ptr::null_mut()),
    };
    let mut syncp = std::ptr::null_mut();
    let status = retry_device_busy("MFXVideoENCODE_EncodeFrameAsync", || unsafe {
        vpl::MFXVideoENCODE_EncodeFrameAsync(
            session.raw(),
            ctrl,
            surface,
            bitstream,
            &mut syncp,
        )
    })?;
    match status {
        vpl::mfxStatus_MFX_ERR_NONE => Ok(EncodeAsyncStatus::Submitted(syncp)),
        vpl::mfxStatus_MFX_ERR_MORE_DATA => Ok(EncodeAsyncStatus::NeedsMoreData),
        status if status > 0 => Ok(EncodeAsyncStatus::Submitted(syncp)),
        status => Err(format!(
            "MFXVideoENCODE_EncodeFrameAsync failed with oneVPL status {status}"
        )),
    }
}

fn output_chunk(
    parameter_sets: &Bytes,
    output: &OutputBitstream,
    pts: u64,
    forced_keyframe: bool,
) -> Result<H264EncodedOutputChunk<Bytes>, String> {
    let bitstream = &output.bitstream;
    let buffer = &output.buffer;
    let start = bitstream.DataOffset as usize;
    let end = start.checked_add(bitstream.DataLength as usize).ok_or_else(|| {
        "Intel Quick Sync returned an overflowing bitstream range".to_string()
    })?;
    let encoded = buffer.as_slice().get(start..end).ok_or_else(|| {
        "Intel Quick Sync returned an invalid bitstream range".to_string()
    })?;
    let is_keyframe =
        forced_keyframe || bitstream.FrameType & vpl::MFX_FRAMETYPE_IDR as u16 != 0;
    let data = if is_keyframe && !annexb_has_sps_pps(encoded) {
        let mut out = BytesMut::with_capacity(parameter_sets.len() + encoded.len());
        out.extend_from_slice(parameter_sets);
        out.extend_from_slice(encoded);
        out.freeze()
    } else {
        Bytes::copy_from_slice(encoded)
    };
    Ok(H264EncodedOutputChunk { data, pts, is_keyframe })
}

fn annexb_has_sps_pps(data: &[u8]) -> bool {
    let mut has_sps = false;
    let mut has_pps = false;
    let mut index = 0;
    while let Some((start, code_len)) = next_start_code(&data[index..]) {
        let nal_start = index + start + code_len;
        if let Some(header) = data.get(nal_start) {
            match header & 0x1f {
                7 => has_sps = true,
                8 => has_pps = true,
                _ => {}
            }
        }
        index = nal_start + 1;
    }
    has_sps && has_pps
}

fn next_start_code(data: &[u8]) -> Option<(usize, usize)> {
    for index in 0..data.len().saturating_sub(2) {
        let tail = &data[index..];
        if tail.starts_with(&[0, 0, 0, 1]) {
            return Some((index, 4));
        }
        if tail.starts_with(&[0, 0, 1]) {
            return Some((index, 3));
        }
    }
    None
}

fn virtual_buffer_kb(bits_per_second: u64, duration: Duration) -> u16 {
    let bytes = u128::from(bits_per_second)
        .saturating_mul(duration.as_millis())
        .saturating_div(8_000);
    ((bytes.saturating_add(1023) / 1024).clamp(1, u128::from(u16::MAX))) as u16
}

fn bitrate_kbps(bits_per_second: u64) -> u16 {
    bits_per_second.div_ceil(1000).clamp(1, u64::from(u16::MAX)) as u16
}

fn h264_coded_resolution(resolution: VideoResolution) -> VideoResolution {
    VideoResolution {
        width: align_to(resolution.width, H264_DIMENSION_ALIGNMENT),
        height: align_to(resolution.height, H264_DIMENSION_ALIGNMENT),
    }
}

fn h264_encoder_layout(
    resolution: VideoResolution,
) -> Result<H264EncoderLayout, QuickSyncH264EncoderError> {
    if resolution.width == 0 || resolution.height == 0 {
        return Err(QuickSyncH264EncoderError::ZeroResolution(resolution));
    }
    if resolution.width % 2 != 0 || resolution.height % 2 != 0 {
        return Err(QuickSyncH264EncoderError::OddResolution(resolution));
    }
    if resolution.width > H264_ENCODER_MAX_DIMENSION
        || resolution.height > H264_ENCODER_MAX_DIMENSION
    {
        return Err(QuickSyncH264EncoderError::ResolutionTooLarge {
            resolution,
            max: H264_ENCODER_MAX_DIMENSION,
        });
    }
    let coded = h264_coded_resolution(resolution);
    Ok(H264EncoderLayout {
        visible: resolution,
        coded,
        bitstream_buffer_size: h264_bitstream_buffer_size(coded)?,
    })
}

fn h264_bitstream_buffer_size(
    coded_resolution: VideoResolution,
) -> Result<u32, QuickSyncH264EncoderError> {
    let raw_size =
        u64::from(coded_resolution.width) * u64::from(coded_resolution.height) * 4;
    let size = raw_size.max(u64::from(MIN_H264_BITSTREAM_BUFFER_SIZE));
    u32::try_from(size).map_err(|_| QuickSyncH264EncoderError::BitstreamBufferTooLarge {
        resolution: coded_resolution,
        max_bytes: u32::MAX,
    })
}

fn align_to(value: u32, alignment: u32) -> u32 {
    value.div_ceil(alignment) * alignment
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolution(width: u32, height: u32) -> VideoResolution {
        VideoResolution { width, height }
    }

    #[test]
    fn annexb_detects_parameter_sets_with_mixed_start_code_lengths() {
        assert!(annexb_has_sps_pps(&[0, 0, 1, 0x67, 1, 2, 3, 0, 0, 0, 1, 0x68,]));
        assert!(annexb_has_sps_pps(&[0, 0, 0, 1, 0x67, 1, 2, 3, 0, 0, 1, 0x68,]));
    }

    #[test]
    fn annexb_rejects_missing_parameter_sets() {
        assert!(!annexb_has_sps_pps(&[0, 0, 1, 0x67, 1, 2, 3]));
        assert!(!annexb_has_sps_pps(&[0, 0, 0, 1, 0x68, 1, 2, 3]));
    }

    #[test]
    fn h264_encoder_resolution_rejects_zero_dimensions() {
        let err = h264_encoder_layout(resolution(0, 1080)).unwrap_err();

        assert!(matches!(err, QuickSyncH264EncoderError::ZeroResolution(_)));
    }

    #[test]
    fn h264_encoder_resolution_rejects_odd_nv12_dimensions() {
        let err = h264_encoder_layout(resolution(1919, 1080)).unwrap_err();

        assert!(matches!(err, QuickSyncH264EncoderError::OddResolution(_)));
    }

    #[test]
    fn h264_encoder_resolution_rejects_dimensions_above_onevpl_limit() {
        let err = h264_encoder_layout(resolution(H264_ENCODER_MAX_DIMENSION + 2, 1080))
            .unwrap_err();

        assert!(matches!(err, QuickSyncH264EncoderError::ResolutionTooLarge { .. }));
    }

    #[test]
    fn h264_encoder_resolution_rejects_oversized_bitstream_buffer() {
        let err = h264_encoder_layout(resolution(H264_ENCODER_MAX_DIMENSION, 32768))
            .unwrap_err();

        assert!(matches!(err, QuickSyncH264EncoderError::BitstreamBufferTooLarge { .. }));
    }

    #[test]
    fn h264_variable_bitrate_rejects_max_below_average() {
        let err = H264VariableBitrate::new(
            NonZeroU64::new(5_000_000).unwrap(),
            NonZeroU64::new(4_000_000).unwrap(),
        )
        .unwrap_err();
        assert!(matches!(err, H264RateControlError::MaxBelowAverage { .. }));
    }
}
