use std::{
    alloc::{Layout, alloc, dealloc, handle_alloc_error},
    collections::{HashMap, VecDeque},
    num::{NonZeroU16, NonZeroU64},
    ptr::NonNull,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use crate::quicksync::sys as vpl;
use bytes::{Bytes, BytesMut};
use tracing::{info, warn};

use crate::{
    InputFrame, VideoFramerate, VideoResolution,
    device::CodecColorDescription,
    dmabuf::{
        DmaBufFrame, DmaBufInterop, QuickSyncDmaBufSync, RenderableNv12DmaBuf,
        StagedDmaBufWrite, export_renderable_nv12,
    },
    parameters::{ColorRange, ColorSpace},
    quicksync::{
        Nv12Plane,
        h264::{
            H264Session, H264SessionError, ImportedNv12Surface, VplSyncQueue,
            init_dmabuf_interop, nv12_progressive_frame_info, retry_device_busy,
            vpl_u16_dimension,
        },
        va::{ExternalNv12DmaBuf, ImportedVaSurface},
        vpl::{Component, FrameSurface, Session, SyncWait, check_status_allow_warnings},
    },
};

const H264_PADDING_LUMA: u8 = 16;
const H264_PADDING_CHROMA: u8 = 128;
const H264_DIMENSION_ALIGNMENT: u32 = 16;
const MIN_H264_BITSTREAM_BUFFER_SIZE: u32 = 1_500_000;
const H264_ENCODER_MAX_DIMENSION: u32 =
    u16::MAX as u32 / H264_DIMENSION_ALIGNMENT * H264_DIMENSION_ALIGNMENT;
const H264_VIDEO_FORMAT_UNSPECIFIED: u16 = 5;
const QUICKSYNC_ENCODER_ASYNC_DEPTH: u16 = 8;
/// Extra zero-copy pool slots covering frames in flight on the bounded encoder
/// frame channel (the compositor holds a slot from render until the encoder
/// retires the bitstream). Keeps the pool bounded while avoiding starvation.
const ZERO_COPY_POOL_CHANNEL_MARGIN: usize = 6;

/// Quick Sync H264 encoder over wgpu NV12 textures.
///
/// Two interchangeable input paths share the same oneVPL bitstream pump:
///
/// - [`ZeroCopyEncoderH264`] — "reverse ownership": the compositor renders each
///   NV12 output frame *directly* into one of a bounded pool of dma-buf surfaces
///   that VA + oneVPL encode zero-copy ([`MFX_SURFACE_FLAG_IMPORT_SHARED`]). No
///   per-frame copy.
/// - [`CopyEncoderH264`] — the original path: oneVPL allocates input surfaces and
///   each frame is `copy_texture_to_texture`'d into one before encode. Kept intact
///   as a clean fallback when the zero-copy path is unavailable.
pub enum WgpuTexturesEncoderH264 {
    ZeroCopy(ZeroCopyEncoderH264),
    Copy(CopyEncoderH264),
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

    #[error("Intel Quick Sync H264 encoder requires NV12 textures, got {0:?}")]
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

    #[error(
        "Intel Quick Sync H264 zero-copy frame did not match any encoder pool surface"
    )]
    UnknownZeroCopyFrame,
}

impl WgpuTexturesEncoderH264 {
    pub fn new(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        config: H264EncoderConfig<'_>,
    ) -> Result<Self, QuickSyncH264EncoderError> {
        let layout = h264_encoder_layout(config.resolution)?;
        info!("Initializing Intel Quick Sync H264 encoder");

        // The zero-copy "reverse ownership" path is OPT-IN: measured slower than
        // the copy path on Intel (exported dma-buf surfaces lose driver-managed
        // render-target compression, so render+encode through them costs more than
        // a cheap blit into an internal optimal-tiled surface). Default to the
        // proven copy path; enable the experiment with MIROIR_QUICKSYNC_ZEROCOPY=1.
        let zero_copy_enabled = std::env::var("MIROIR_QUICKSYNC_ZEROCOPY")
            .is_ok_and(|value| matches!(value.as_str(), "1" | "true"));
        if zero_copy_enabled {
            match ZeroCopyEncoderH264::new(&device, &queue, config, layout) {
                Ok(encoder) => {
                    info!(
                        width = layout.visible.width,
                        height = layout.visible.height,
                        coded_width = layout.coded.width,
                        coded_height = layout.coded.height,
                        pool_size = encoder.pool.slots.len(),
                        "Initialized Intel Quick Sync H264 encoder (zero-copy, IMPORT_SHARED)"
                    );
                    return Ok(Self::ZeroCopy(encoder));
                }
                Err(err) => {
                    warn!(
                        "Intel Quick Sync H264 zero-copy path unavailable, falling back to copy path: {err}"
                    );
                }
            }
        }

        let encoder = CopyEncoderH264::new(device, queue, config, layout)?;
        info!(
            width = layout.visible.width,
            height = layout.visible.height,
            coded_width = layout.coded.width,
            coded_height = layout.coded.height,
            pool_size = encoder.input_pool.len(),
            "Initialized Intel Quick Sync H264 encoder (copy path)"
        );
        Ok(Self::Copy(encoder))
    }

    pub fn parameter_sets(&self) -> &Bytes {
        match self {
            Self::ZeroCopy(encoder) => encoder.inner.parameter_sets(),
            Self::Copy(encoder) => encoder.inner.parameter_sets(),
        }
    }

    /// The shared dma-buf NV12 pool the compositor must render into for the
    /// zero-copy path. `None` when running on the copy fallback.
    pub fn external_pool(&self) -> Option<Arc<ZeroCopyNv12Pool>> {
        match self {
            Self::ZeroCopy(encoder) => Some(Arc::clone(&encoder.pool)),
            Self::Copy(_) => None,
        }
    }

    pub fn encode(
        &mut self,
        frame: InputFrame<wgpu::Texture>,
        force_keyframe: bool,
    ) -> Result<Vec<H264EncodedOutputChunk<Bytes>>, QuickSyncH264EncoderError> {
        match self {
            Self::ZeroCopy(encoder) => encoder.encode(frame, force_keyframe),
            Self::Copy(encoder) => encoder.encode(frame, force_keyframe),
        }
    }

    pub fn flush(
        &mut self,
    ) -> Result<Vec<H264EncodedOutputChunk<Bytes>>, QuickSyncH264EncoderError> {
        match self {
            Self::ZeroCopy(encoder) => encoder.flush(),
            Self::Copy(encoder) => encoder.flush(),
        }
    }

    pub fn poll_output(
        &mut self,
    ) -> Result<Vec<H264EncodedOutputChunk<Bytes>>, QuickSyncH264EncoderError> {
        match self {
            Self::ZeroCopy(encoder) => encoder.poll_output(),
            Self::Copy(encoder) => encoder.poll_output(),
        }
    }
}

// =====================================================================
// Copy path (fallback): oneVPL allocates input surfaces, each frame is
// copy_texture_to_texture'd into one before encode.
// =====================================================================

pub struct CopyEncoderH264 {
    input_pool: VecDeque<EncodeInputSurface>,
    inner: QuickSyncH264Encoder<EncodeInputSurface>,
    sync: QuickSyncDmaBufSync,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    resolution: VideoResolution,
    padding_rects: Box<[PaddingRect]>,
}

impl CopyEncoderH264 {
    fn new(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        config: H264EncoderConfig<'_>,
        layout: H264EncoderLayout,
    ) -> Result<Self, QuickSyncH264EncoderError> {
        let (interop, sync) = init_dmabuf_interop(&device, &queue)?;
        let resolution = layout.visible;
        let padding_rects = h264_coded_padding_rects(layout.visible, layout.coded);
        let pool_size = encoder_pool_size(config.max_pending_frames);

        let (quicksync, parameter_sets) = init_h264_encode_session(config, layout)?;
        let mut inner =
            QuickSyncH264Encoder::with_session(quicksync, parameter_sets, layout);
        let input_pool = (0..pool_size)
            .map(|_| inner.create_input_surface(&interop))
            .collect::<Result<VecDeque<_>, _>>()
            .map_err(QuickSyncH264EncoderError::Encode)?;

        Ok(Self {
            input_pool,
            inner,
            sync,
            device,
            queue,
            resolution,
            padding_rects,
        })
    }

    fn encode(
        &mut self,
        frame: InputFrame<wgpu::Texture>,
        force_keyframe: bool,
    ) -> Result<Vec<H264EncodedOutputChunk<Bytes>>, QuickSyncH264EncoderError> {
        validate_input_texture(&frame.data, self.resolution)?;
        let pts = frame.pts.ok_or(QuickSyncH264EncoderError::MissingPts)?;

        let mut chunks = self.retire_completed(SyncWait::Poll)?;

        if self.inner.is_full() || self.input_pool.is_empty() {
            // TEMP: handoff instrumentation (H2 retire block)
            let h2_pending = self.inner.pending_len();
            let h2_pool = self.input_pool.len();
            let h2_start = std::time::Instant::now();
            chunks.extend(self.retire_completed(SyncWait::Block)?);
            let h2_ms = h2_start.elapsed().as_secs_f64() * 1000.0;
            if h2_ms > 3.0 {
                info!(h2_ms, h2_pending, h2_pool, "handoff_h2 retire block");
            }
        }

        let input = self
            .input_pool
            .pop_front()
            .ok_or(QuickSyncH264EncoderError::InputSurfacePoolExhausted)?;
        self.copy_input_to_surface(&frame.data, &input)?;
        self.inner
            .encode(input, pts, force_keyframe)
            .map_err(QuickSyncH264EncoderError::Encode)?;
        chunks.extend(self.retire_completed(SyncWait::Poll)?);
        Ok(chunks)
    }

    fn flush(
        &mut self,
    ) -> Result<Vec<H264EncodedOutputChunk<Bytes>>, QuickSyncH264EncoderError> {
        let completed = self.inner.flush().map_err(QuickSyncH264EncoderError::Encode)?;
        Ok(self.collect_completed(completed))
    }

    fn poll_output(
        &mut self,
    ) -> Result<Vec<H264EncodedOutputChunk<Bytes>>, QuickSyncH264EncoderError> {
        self.retire_completed(SyncWait::Poll)
    }

    fn copy_input_to_surface(
        &self,
        texture: &wgpu::Texture,
        input: &EncodeInputSurface,
    ) -> Result<(), QuickSyncH264EncoderError> {
        let started = Instant::now();
        let mut encoder =
            self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Intel Quick Sync H264 input copy"),
            });
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: input.imported.frame.texture(),
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            self.resolution.extent_2d(),
        );
        self.sync
            .submit_dma_buf_write(
                &input.imported.frame,
                encoder,
                "Intel Quick Sync H264 input copy",
            )
            .map_err(|err| QuickSyncH264EncoderError::Encode(err.to_string()))?;
        let copy_ms = started.elapsed().as_secs_f64() * 1000.0; // TEMP: handoff instrumentation
        if copy_ms > 3.0 {
            info!(copy_ms, "handoff_h2 copy_input_to_surface");
        }
        write_coded_padding(&self.sync, &self.queue, &self.padding_rects, &input.imported.frame)
    }

    fn retire_completed(
        &mut self,
        wait: SyncWait,
    ) -> Result<Vec<H264EncodedOutputChunk<Bytes>>, QuickSyncH264EncoderError> {
        let completed = self
            .inner
            .drain_completed(wait)
            .map_err(QuickSyncH264EncoderError::Encode)?;
        Ok(self.collect_completed(completed))
    }

    fn collect_completed(
        &mut self,
        completed: Vec<EncodeCompletion<EncodeInputSurface>>,
    ) -> Vec<H264EncodedOutputChunk<Bytes>> {
        let mut chunks = Vec::with_capacity(completed.len());
        for completion in completed {
            self.input_pool.push_back(completion.token);
            chunks.push(completion.chunk);
        }
        chunks
    }
}

impl Drop for CopyEncoderH264 {
    fn drop(&mut self) {
        self.input_pool.clear();
        let _ = self.inner.flush();
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
    }
}

// =====================================================================
// Zero-copy path: the compositor renders NV12 directly into a bounded pool of
// dma-buf surfaces that VA + oneVPL encode with no copy (IMPORT_SHARED).
// =====================================================================

/// A bounded pool of renderable NV12 dma-buf surfaces, shared between the
/// compositor (renders into a free slot) and the encoder (encodes that exact
/// slot zero-copy, then frees it when the bitstream retires).
///
/// Holds only the producer-side, `Send + Sync` resources (the wgpu textures, the
/// dma-buf fences and the GPU-write sync machinery). The VA + oneVPL surfaces
/// that consume each slot live in [`ZeroCopyEncoderH264`] on the encoder thread.
pub struct ZeroCopyNv12Pool {
    slots: Box<[PoolSlot]>,
    sync: QuickSyncDmaBufSync,
    coded: VideoResolution,
    visible: VideoResolution,
}

struct PoolSlot {
    texture: Arc<wgpu::Texture>,
    frame: Arc<DmaBufFrame>,
    busy: AtomicBool,
}

/// A slot acquired by the compositor: render into `texture`, then stage the write
/// via [`ZeroCopyNv12Pool::stage_write`] and finish it after the batched submit.
pub struct AcquiredNv12Slot {
    pub index: usize,
    pub texture: Arc<wgpu::Texture>,
}

impl ZeroCopyNv12Pool {
    /// Coded (16-aligned) size of the pool textures. The compositor must render
    /// into the top-left `visible_resolution()` region and leave the rest as
    /// padding (see [`Self::padding_luma`] / [`Self::padding_chroma`]).
    pub fn coded_resolution(&self) -> VideoResolution {
        self.coded
    }

    pub fn visible_resolution(&self) -> VideoResolution {
        self.visible
    }

    /// Normalized clear value for the Y plane padding (luma 16).
    pub fn padding_luma(&self) -> f64 {
        f64::from(H264_PADDING_LUMA) / 255.0
    }

    /// Normalized clear value for the UV plane padding (chroma 128).
    pub fn padding_chroma(&self) -> f64 {
        f64::from(H264_PADDING_CHROMA) / 255.0
    }

    /// Acquire a free slot, or `None` if all slots are in flight. Bounded: the
    /// pool never grows. Callers apply natural backpressure via the bounded
    /// frame channel.
    pub fn acquire(&self) -> Option<AcquiredNv12Slot> {
        self.slots.iter().enumerate().find_map(|(index, slot)| {
            slot.busy
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
                .then(|| AcquiredNv12Slot {
                    index,
                    texture: Arc::clone(&slot.texture),
                })
        })
    }

    /// Stage the GPU-write fence for a slot whose render commands the compositor
    /// already recorded into its shared command encoder, *without* submitting.
    /// The compositor stages every output this way, performs one batched submit,
    /// then calls [`Self::finish_write`] for each token. This collapses the former
    /// per-output fenced submit into a single submit per frame (the regression
    /// fix). The returned token must be passed to [`Self::finish_write`] after the
    /// caller's single `queue.submit`.
    pub fn stage_write(&self, index: usize) -> Result<StagedDmaBufWrite, String> {
        self.sync
            .stage_dma_buf_write(
                &self.slots[index].frame,
                "Intel Quick Sync H264 zero-copy render",
            )
            .map_err(|err| err.to_string())
    }

    /// Complete a [`Self::stage_write`] token after the batched submit: import the
    /// release fence into the dma-buf so VA waits for the GPU write before encode.
    pub fn finish_write(&self, token: StagedDmaBufWrite) -> Result<(), String> {
        self.sync.finish_dma_buf_write(token).map_err(|err| err.to_string())
    }

    fn slot_index_of(&self, texture: &wgpu::Texture) -> Option<usize> {
        // `wgpu::Texture` equality is proxied to its inner dispatch handle, so a
        // cloned handle of the same GPU texture compares equal to the pool slot's.
        self.slots.iter().position(|slot| slot.texture.as_ref() == texture)
    }

    fn free(&self, index: usize) {
        self.slots[index].busy.store(false, Ordering::Release);
    }
}

struct SlotSurface {
    // Field order matters for teardown: the oneVPL surface must be released
    // before the VA surface it wraps is destroyed.
    surface: Option<FrameSurface>,
    _va: ImportedVaSurface,
}

/// Encoder input token for the zero-copy path: the slot's oneVPL surface travels
/// with the in-flight frame and is returned to its slot on retire.
struct ZeroCopyInput {
    surface: FrameSurface,
    slot_index: usize,
}

pub struct ZeroCopyEncoderH264 {
    // Drop order (consumers before producers): slot_surfaces (oneVPL + VA) ->
    // inner (oneVPL session close + VA display terminate) -> pool (dma-buf fds +
    // wgpu textures). `pool` is shared with the compositor via `Arc`, so its
    // textures outlive every consumer regardless of refcount.
    slot_surfaces: Vec<SlotSurface>,
    inner: QuickSyncH264Encoder<ZeroCopyInput>,
    pool: Arc<ZeroCopyNv12Pool>,
    resolution: VideoResolution,
}

impl ZeroCopyEncoderH264 {
    fn new(
        device: &Arc<wgpu::Device>,
        queue: &Arc<wgpu::Queue>,
        config: H264EncoderConfig<'_>,
        layout: H264EncoderLayout,
    ) -> Result<Self, QuickSyncH264EncoderError> {
        let (interop, sync) = init_dmabuf_interop(device, queue)?;
        let pool_size =
            encoder_pool_size(config.max_pending_frames) + ZERO_COPY_POOL_CHANNEL_MARGIN;

        // `pool_slots` is declared (and so dropped) last of the three so its
        // textures/fds outlive the consumers built below.
        let mut pool_slots: Vec<PoolSlot> = Vec::with_capacity(pool_size);
        let (quicksync, parameter_sets) = init_h264_encode_session(config, layout)?;
        let mut slot_surfaces: Vec<SlotSurface> = Vec::with_capacity(pool_size);

        for slot in 0..pool_size {
            let (pool_slot, slot_surface) =
                Self::import_slot(&interop, &quicksync, layout).map_err(|err| {
                    QuickSyncH264EncoderError::Encode(format!(
                        "zero-copy slot {slot} import failed: {err}"
                    ))
                })?;
            pool_slots.push(pool_slot);
            slot_surfaces.push(slot_surface);
        }

        let inner =
            QuickSyncH264Encoder::with_session(quicksync, parameter_sets, layout);
        let pool = Arc::new(ZeroCopyNv12Pool {
            slots: pool_slots.into_boxed_slice(),
            sync,
            coded: layout.coded,
            visible: layout.visible,
        });

        Ok(Self { slot_surfaces, inner, pool, resolution: layout.visible })
    }

    /// Allocate one renderable NV12 dma-buf, import it into VA + oneVPL, and
    /// assert it imported zero-copy ([`MFX_SURFACE_FLAG_IMPORT_SHARED`]).
    fn import_slot(
        interop: &DmaBufInterop,
        quicksync: &H264Session,
        layout: H264EncoderLayout,
    ) -> Result<(PoolSlot, SlotSurface), String> {
        let coded = layout.coded;
        let renderable: RenderableNv12DmaBuf =
            export_renderable_nv12(interop, coded).map_err(|err| err.to_string())?;

        // Import into VA while the fd is still borrowable, then take ownership of
        // the fd by wrapping the texture as a DmaBufFrame.
        let va_surface = quicksync
            .display()
            .import_nv12_surface(ExternalNv12DmaBuf {
                fd: renderable.fd_raw(),
                size: renderable.size,
                modifier: renderable.modifier,
                width: coded.width,
                height: coded.height,
                y_offset: renderable.y_offset,
                y_pitch: renderable.y_pitch,
                uv_offset: renderable.uv_offset,
                uv_pitch: renderable.uv_pitch,
            })
            .map_err(|err| err.to_string())?;

        let (texture, frame) =
            renderable.into_dmabuf_frame(coded).map_err(|err| err.to_string())?;

        let (mut surface, import_flags) = quicksync
            .session
            .import_va_surface(
                quicksync.display().handle(),
                va_surface.id(),
                Component::Encode,
            )
            .map_err(|err| err.to_string())?;
        if import_flags & vpl::MFX_SURFACE_FLAG_IMPORT_SHARED == 0 {
            return Err(format!(
                "VA surface imported without IMPORT_SHARED (flags {import_flags:#x}); zero-copy not available"
            ));
        }
        configure_imported_surface_info(&mut surface, layout);

        Ok((
            PoolSlot { texture, frame, busy: AtomicBool::new(false) },
            SlotSurface { surface: Some(surface), _va: va_surface },
        ))
    }

    fn encode(
        &mut self,
        frame: InputFrame<wgpu::Texture>,
        force_keyframe: bool,
    ) -> Result<Vec<H264EncodedOutputChunk<Bytes>>, QuickSyncH264EncoderError> {
        validate_input_texture(&frame.data, self.resolution)?;
        let pts = frame.pts.ok_or(QuickSyncH264EncoderError::MissingPts)?;
        let slot_index = self
            .pool
            .slot_index_of(&frame.data)
            .ok_or(QuickSyncH264EncoderError::UnknownZeroCopyFrame)?;

        let mut chunks = self.retire_completed(SyncWait::Poll)?;

        if self.inner.is_full() {
            // TEMP: handoff instrumentation (H2 retire block). Note: no
            // copy_input_to_surface stage on this path — the copy is gone.
            let h2_pending = self.inner.pending_len();
            let h2_start = std::time::Instant::now();
            chunks.extend(self.retire_completed(SyncWait::Block)?);
            let h2_ms = h2_start.elapsed().as_secs_f64() * 1000.0;
            if h2_ms > 3.0 {
                info!(h2_ms, h2_pending, h2_pool = 0, "handoff_h2 retire block");
            }
        }

        let surface = self.slot_surfaces[slot_index]
            .surface
            .take()
            .expect("zero-copy slot surface present while idle");
        self.inner
            .encode(ZeroCopyInput { surface, slot_index }, pts, force_keyframe)
            .map_err(QuickSyncH264EncoderError::Encode)?;
        chunks.extend(self.retire_completed(SyncWait::Poll)?);
        Ok(chunks)
    }

    fn flush(
        &mut self,
    ) -> Result<Vec<H264EncodedOutputChunk<Bytes>>, QuickSyncH264EncoderError> {
        let completed = self.inner.flush().map_err(QuickSyncH264EncoderError::Encode)?;
        Ok(self.collect_completed(completed))
    }

    fn poll_output(
        &mut self,
    ) -> Result<Vec<H264EncodedOutputChunk<Bytes>>, QuickSyncH264EncoderError> {
        self.retire_completed(SyncWait::Poll)
    }

    fn retire_completed(
        &mut self,
        wait: SyncWait,
    ) -> Result<Vec<H264EncodedOutputChunk<Bytes>>, QuickSyncH264EncoderError> {
        let completed = self
            .inner
            .drain_completed(wait)
            .map_err(QuickSyncH264EncoderError::Encode)?;
        Ok(self.collect_completed(completed))
    }

    fn collect_completed(
        &mut self,
        completed: Vec<EncodeCompletion<ZeroCopyInput>>,
    ) -> Vec<H264EncodedOutputChunk<Bytes>> {
        let mut chunks = Vec::with_capacity(completed.len());
        for completion in completed {
            let ZeroCopyInput { surface, slot_index } = completion.token;
            self.slot_surfaces[slot_index].surface = Some(surface);
            self.pool.free(slot_index);
            chunks.push(completion.chunk);
        }
        chunks
    }
}

impl Drop for ZeroCopyEncoderH264 {
    fn drop(&mut self) {
        let _ = self.inner.flush();
        // Release in-flight oneVPL surfaces first (consumers before producers).
        self.slot_surfaces.clear();
    }
}

// =====================================================================
// Shared oneVPL bitstream pump (generic over the input token type).
// =====================================================================

/// Provides the oneVPL input surface for a frame being submitted. The token is
/// owned by the pump until the frame retires.
trait EncodeInput {
    fn surface(&self) -> &FrameSurface;
    fn surface_mut(&mut self) -> &mut FrameSurface;
}

impl EncodeInput for EncodeInputSurface {
    fn surface(&self) -> &FrameSurface {
        &self.surface
    }
    fn surface_mut(&mut self) -> &mut FrameSurface {
        &mut self.surface
    }
}

impl EncodeInput for ZeroCopyInput {
    fn surface(&self) -> &FrameSurface {
        &self.surface
    }
    fn surface_mut(&mut self) -> &mut FrameSurface {
        &mut self.surface
    }
}

struct QuickSyncH264Encoder<I> {
    quicksync: H264Session,
    parameter_sets: Bytes,
    frame_index: u64,
    bitstream_buffer_size: u32,
    bitstream_pool: Vec<Box<OutputBitstream>>,
    pending_bitstreams: VplSyncQueue<Box<OutputBitstream>>,
    pending_frames: HashMap<u64, PendingFrame<I>>,
}

impl<I: EncodeInput> QuickSyncH264Encoder<I> {
    fn with_session(
        quicksync: H264Session,
        parameter_sets: Bytes,
        layout: H264EncoderLayout,
    ) -> Self {
        Self {
            quicksync,
            parameter_sets,
            frame_index: 0,
            bitstream_buffer_size: layout.bitstream_buffer_size,
            bitstream_pool: Vec::with_capacity(
                usize::from(QUICKSYNC_ENCODER_ASYNC_DEPTH) + 1,
            ),
            pending_bitstreams: VplSyncQueue::new(usize::from(
                QUICKSYNC_ENCODER_ASYNC_DEPTH,
            )),
            pending_frames: HashMap::new(),
        }
    }

    fn parameter_sets(&self) -> &Bytes {
        &self.parameter_sets
    }

    fn encode(
        &mut self,
        mut token: I,
        pts: u64,
        force_keyframe: bool,
    ) -> Result<(), String> {
        let frame_index = self.frame_index;
        token.surface_mut().set_timestamp(frame_index);

        self.submit_bitstream(EncodeSubmit::Input {
            force_keyframe,
            surface: token.surface(),
        })?;
        self.pending_frames.insert(
            frame_index,
            PendingFrame { pts, forced_keyframe: force_keyframe, token },
        );
        self.frame_index += 1;
        Ok(())
    }

    fn flush(&mut self) -> Result<Vec<EncodeCompletion<I>>, String> {
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

    fn pending_len(&self) -> usize {
        // TEMP: handoff instrumentation
        self.pending_bitstreams.len()
    }

    fn submit_bitstream(
        &mut self,
        submit: EncodeSubmit<'_>,
    ) -> Result<EncodeAsyncStatus, String> {
        let mut output = self.bitstream_pool.pop().unwrap_or_else(|| {
            Box::new(OutputBitstream::new(self.bitstream_buffer_size))
        });
        output.reset();
        let status =
            encode_frame_async(&self.quicksync.session, submit, &mut output.bitstream)?;
        if let EncodeAsyncStatus::Submitted(syncp) = status {
            self.pending_bitstreams.push(syncp, output);
        } else {
            self.bitstream_pool.push(output);
        }
        Ok(status)
    }

    fn drain_completed(
        &mut self,
        wait: SyncWait,
    ) -> Result<Vec<EncodeCompletion<I>>, String> {
        let outputs = self.pending_bitstreams.drain_completed(&self.quicksync, wait)?;
        self.complete_bitstreams(outputs)
    }

    fn drain_all_completed(&mut self) -> Result<Vec<EncodeCompletion<I>>, String> {
        let outputs = self.pending_bitstreams.drain_all_completed(&self.quicksync)?;
        self.complete_bitstreams(outputs)
    }

    fn complete_bitstreams(
        &mut self,
        outputs: Vec<Box<OutputBitstream>>,
    ) -> Result<Vec<EncodeCompletion<I>>, String> {
        let mut completed = Vec::with_capacity(outputs.len());
        for output in outputs {
            completed.push(complete_bitstream(
                &output,
                &self.parameter_sets,
                &mut self.pending_frames,
            )?);
            self.bitstream_pool.push(output);
        }
        Ok(completed)
    }
}

impl QuickSyncH264Encoder<EncodeInputSurface> {
    fn create_input_surface(
        &mut self,
        interop: &DmaBufInterop,
    ) -> Result<EncodeInputSurface, String> {
        let surface = self
            .quicksync
            .session
            .get_surface_for_encode()
            .map_err(|err| err.to_string())?;
        let imported = self
            .quicksync
            .import_nv12_surface(interop, &surface)
            .map_err(|err| err.to_string())?;
        Ok(EncodeInputSurface { imported, surface })
    }
}

impl<I> Drop for QuickSyncH264Encoder<I> {
    fn drop(&mut self) {
        self.pending_bitstreams.clear();
        self.pending_frames.clear();
    }
}

struct EncodeInputSurface {
    imported: ImportedNv12Surface,
    surface: FrameSurface,
}

struct OutputBitstream {
    bitstream: vpl::mfxBitstream,
    buffer: BitstreamBuffer,
}

impl OutputBitstream {
    fn new(size: u32) -> Self {
        let mut buffer = BitstreamBuffer::new(size);
        let bitstream = new_mfx_bitstream(&mut buffer, size);
        Self { bitstream, buffer }
    }

    fn reset(&mut self) {
        let size = self.buffer.len as u32;
        self.bitstream = new_mfx_bitstream(&mut self.buffer, size);
    }
}

fn new_mfx_bitstream(buffer: &mut BitstreamBuffer, size: u32) -> vpl::mfxBitstream {
    let mut bitstream = unsafe { std::mem::zeroed::<vpl::mfxBitstream>() };
    bitstream.Data = buffer.as_mut_ptr();
    bitstream.MaxLength = size;
    bitstream
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
        let ptr = NonNull::new(unsafe { alloc(layout) }).unwrap_or_else(|| {
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

struct PendingFrame<I> {
    pts: u64,
    forced_keyframe: bool,
    token: I,
}

struct EncodeCompletion<I> {
    chunk: H264EncodedOutputChunk<Bytes>,
    token: I,
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct PaddingRect {
    plane: Nv12Plane,
    origin: wgpu::Origin3d,
    size: wgpu::Extent3d,
    bytes_per_row: u32,
    data: Vec<u8>,
}

impl PaddingRect {
    fn new(plane: Nv12Plane, x: u32, y: u32, width: u32, height: u32, value: u8) -> Self {
        let bytes_per_row = width * plane.bytes_per_texel();
        Self {
            plane,
            origin: wgpu::Origin3d { x, y, z: 0 },
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            bytes_per_row,
            data: vec![value; (bytes_per_row * height) as usize],
        }
    }

    fn from_luma_rect(x: u32, y: u32, width: u32, height: u32) -> [Self; 2] {
        [
            Self::new(Nv12Plane::Y, x, y, width, height, H264_PADDING_LUMA),
            Self::new(
                Nv12Plane::Uv,
                x / 2,
                y / 2,
                width / 2,
                height / 2,
                H264_PADDING_CHROMA,
            ),
        ]
    }
}

fn validate_input_texture(
    texture: &wgpu::Texture,
    resolution: VideoResolution,
) -> Result<(), QuickSyncH264EncoderError> {
    let expected = resolution.extent_2d();
    if !texture.usage().contains(wgpu::TextureUsages::COPY_SRC) {
        return Err(QuickSyncH264EncoderError::NoCopySrcTextureUsage(texture.usage()));
    }
    if texture.format() != wgpu::TextureFormat::NV12 {
        return Err(QuickSyncH264EncoderError::UnsupportedInputTexture(texture.format()));
    }
    // The zero-copy pool textures are coded-size; the copy path textures are
    // visible-size. Only the copy path's plain textures must match `expected`.
    if texture.size() != expected
        && texture.size() != coded_extent(resolution)
    {
        return Err(QuickSyncH264EncoderError::InconsistentTextureSize {
            expected,
            provided: texture.size(),
        });
    }
    Ok(())
}

fn coded_extent(resolution: VideoResolution) -> wgpu::Extent3d {
    h264_coded_resolution(resolution).extent_2d()
}

fn encoder_pool_size(max_pending_frames: usize) -> usize {
    (max_pending_frames + 2).max(usize::from(QUICKSYNC_ENCODER_ASYNC_DEPTH) + 1)
}

fn write_coded_padding(
    sync: &QuickSyncDmaBufSync,
    queue: &wgpu::Queue,
    padding_rects: &[PaddingRect],
    frame: &DmaBufFrame,
) -> Result<(), QuickSyncH264EncoderError> {
    if padding_rects.is_empty() {
        return Ok(());
    }
    let texture = frame.texture();
    for rect in padding_rects {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: rect.origin,
                aspect: rect.plane.aspect(),
            },
            &rect.data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(rect.bytes_per_row),
                rows_per_image: Some(rect.size.height),
            },
            rect.size,
        );
    }

    sync.submit_pending_dma_buf_writes(
        frame,
        "Intel Quick Sync H264 coded padding write",
    )
    .map_err(|err| QuickSyncH264EncoderError::Encode(err.to_string()))
}

/// Configure an imported oneVPL surface's `Info` to match the encoder layout.
fn configure_imported_surface_info(surface: &mut FrameSurface, layout: H264EncoderLayout) {
    unsafe {
        let info = &mut (*surface.raw()).Info;
        info.FourCC = vpl::MFX_FOURCC_NV12;
        info.ChromaFormat = vpl::MFX_CHROMAFORMAT_YUV420 as u16;
        info.PicStruct = vpl::MFX_PICSTRUCT_PROGRESSIVE as u16;
        info.BitDepthLuma = 8;
        info.BitDepthChroma = 8;
        let dims = &mut info.__bindgen_anon_1.__bindgen_anon_1;
        dims.Width = layout.coded.width as u16;
        dims.Height = layout.coded.height as u16;
        dims.CropW = layout.visible.width as u16;
        dims.CropH = layout.visible.height as u16;
    }
}

fn init_h264_encode_session(
    config: H264EncoderConfig<'_>,
    layout: H264EncoderLayout,
) -> Result<(H264Session, Bytes), QuickSyncH264EncoderError> {
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
    Ok((quicksync, parameter_sets))
}

fn complete_bitstream<I>(
    output: &OutputBitstream,
    parameter_sets: &Bytes,
    pending_frames: &mut HashMap<u64, PendingFrame<I>>,
) -> Result<EncodeCompletion<I>, String> {
    let frame_index = output.bitstream.TimeStamp;
    let pending_frame = pending_frames.remove(&frame_index).ok_or_else(|| {
        format!(
            "Intel Quick Sync returned H264 bitstream for unknown frame {frame_index}"
        )
    })?;
    Ok(EncodeCompletion {
        chunk: output_chunk(
            parameter_sets,
            output,
            pending_frame.pts,
            pending_frame.forced_keyframe,
        )?,
        token: pending_frame.token,
    })
}

fn encoder_video_param(
    config: &H264EncoderConfig<'_>,
    layout: H264EncoderLayout,
) -> Result<H264EncoderVideoParam, String> {
    let mut frame_info = nv12_progressive_frame_info();
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
        mfx.CodecLevel = h264_codec_level(layout.coded, config.framerate);
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
    option.ScenarioInfo = vpl::MFX_SCENARIO_CAMERA_CAPTURE as u16;
    option.ContentInfo = vpl::MFX_CONTENT_FULL_SCREEN_VIDEO as u16;
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

fn h264_coded_padding_rects(
    visible: VideoResolution,
    coded: VideoResolution,
) -> Box<[PaddingRect]> {
    assert!(coded.width >= visible.width);
    assert!(coded.height >= visible.height);

    let mut rects = Vec::with_capacity(4);
    if coded.width > visible.width {
        rects.extend(PaddingRect::from_luma_rect(
            visible.width,
            0,
            coded.width - visible.width,
            visible.height,
        ));
    }
    if coded.height > visible.height {
        rects.extend(PaddingRect::from_luma_rect(
            0,
            visible.height,
            coded.width,
            coded.height - visible.height,
        ));
    }
    rects.into_boxed_slice()
}

struct H264LevelLimit {
    level: u16,
    max_macroblocks_per_second: u64,
    max_macroblocks_per_frame: u64,
}

const H264_LEVEL_LIMITS: &[H264LevelLimit] = &[
    H264LevelLimit {
        level: vpl::MFX_LEVEL_AVC_4 as u16,
        max_macroblocks_per_second: 245_760,
        max_macroblocks_per_frame: 8_192,
    },
    H264LevelLimit {
        level: vpl::MFX_LEVEL_AVC_42 as u16,
        max_macroblocks_per_second: 522_240,
        max_macroblocks_per_frame: 8_704,
    },
    H264LevelLimit {
        level: vpl::MFX_LEVEL_AVC_5 as u16,
        max_macroblocks_per_second: 589_824,
        max_macroblocks_per_frame: 22_080,
    },
    H264LevelLimit {
        level: vpl::MFX_LEVEL_AVC_51 as u16,
        max_macroblocks_per_second: 983_040,
        max_macroblocks_per_frame: 36_864,
    },
    H264LevelLimit {
        level: vpl::MFX_LEVEL_AVC_52 as u16,
        max_macroblocks_per_second: 2_073_600,
        max_macroblocks_per_frame: 36_864,
    },
    H264LevelLimit {
        level: vpl::MFX_LEVEL_AVC_62 as u16,
        max_macroblocks_per_second: 16_711_680,
        max_macroblocks_per_frame: 139_264,
    },
];

fn h264_codec_level(coded_resolution: VideoResolution, framerate: VideoFramerate) -> u16 {
    let macroblocks_per_frame =
        u64::from(coded_resolution.width / 16) * u64::from(coded_resolution.height / 16);
    let macroblocks_per_second = macroblocks_per_frame
        .saturating_mul(u64::from(framerate.num.get()))
        .div_ceil(u64::from(framerate.den.get()));

    H264_LEVEL_LIMITS
        .iter()
        .find(|limit| {
            macroblocks_per_frame <= limit.max_macroblocks_per_frame
                && macroblocks_per_second <= limit.max_macroblocks_per_second
        })
        .map(|limit| limit.level)
        .unwrap_or(vpl::MFX_LEVEL_AVC_62 as u16)
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

    fn framerate(num: u32, den: u32) -> VideoFramerate {
        VideoFramerate::new(num, den).unwrap()
    }

    #[test]
    fn h264_level_matches_resolution_and_framerate() {
        assert_eq!(
            h264_codec_level(resolution(1920, 1088), framerate(30_000, 1001)),
            vpl::MFX_LEVEL_AVC_4 as u16
        );
        assert_eq!(
            h264_codec_level(resolution(1920, 1088), framerate(60_000, 1001)),
            vpl::MFX_LEVEL_AVC_42 as u16
        );
        assert_eq!(
            h264_codec_level(resolution(3840, 2160), framerate(30_000, 1001)),
            vpl::MFX_LEVEL_AVC_51 as u16
        );
        assert_eq!(
            h264_codec_level(resolution(3840, 2160), framerate(60_000, 1001)),
            vpl::MFX_LEVEL_AVC_52 as u16
        );
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
