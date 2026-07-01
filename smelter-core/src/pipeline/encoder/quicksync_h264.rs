use std::{
    num::{NonZeroU16, NonZeroU64},
    sync::Arc,
    time::Duration,
};

use std::sync::Arc as StdArc;

use gpu_video::{
    InputFrame, VideoFramerate, VideoResolution,
    parameters::{ColorRange, ColorSpace},
    quicksync::h264::{
        H264EncodedOutputChunk, H264EncoderConfig, H264EncoderPreset,
        H264EncoderRateControl, H264VariableBitrate, QuickSyncH264EncoderError,
        StagedDmaBufWrite, WgpuTexturesEncoderH264, ZeroCopyNv12Pool,
    },
};
use smelter_render::{
    ExternalNv12Frame, ExternalNv12FramePool, FrameData, Framerate, OutputFrameFormat,
    Resolution,
};
use tracing::error;

use crate::{
    pipeline::{
        encoder::{
            VideoEncoder, VideoEncoderConfig,
            utils::{bitrate_from_resolution_framerate, gop_size_from_ms_framerate},
        },
        utils::{annexb_to_avcc, build_avc_decoder_config},
    },
    prelude::*,
};

pub struct QuickSyncH264Encoder {
    encoder: WgpuTexturesEncoderH264,
    bitstream_format: H264BitstreamFormat,
}

const HIGH_QUALITY_VIRTUAL_BUFFER_SIZE: Duration = Duration::from_secs(2);
const LOW_LATENCY_VIRTUAL_BUFFER_SIZE: Duration = Duration::from_millis(100);
const QUICKSYNC_MAX_PENDING_FRAMES: usize = 8;
const QUICKSYNC_OUTPUT_POLL_INTERVAL: Duration = Duration::from_millis(2);

impl VideoEncoder for QuickSyncH264Encoder {
    const LABEL: &'static str = "Intel Quick Sync H264 encoder";
    const OUTPUT_POLL_INTERVAL: Option<Duration> = Some(QUICKSYNC_OUTPUT_POLL_INTERVAL);

    type Options = QuickSyncH264EncoderOptions;

    fn new(
        ctx: &Arc<PipelineCtx>,
        options: Self::Options,
    ) -> Result<(Self, VideoEncoderConfig), EncoderInitError> {
        let adapter_info = ctx.graphics_context.adapter.get_info();
        let config = quicksync_h264_encoder_config(ctx, &options, &adapter_info)?;
        let encoder = WgpuTexturesEncoderH264::new(
            Arc::clone(&ctx.graphics_context.device),
            Arc::clone(&ctx.graphics_context.queue),
            config,
        )
        .map_err(|err| {
            let unavailable = matches!(&err, QuickSyncH264EncoderError::Unavailable(_));
            if unavailable {
                error!("Failed to initialize Intel Quick Sync H264 encoder: {err}");
            }
            quicksync_h264_encoder_init_error(err)
        })?;
        let extradata = if options.bitstream_format == H264BitstreamFormat::Avcc {
            build_avc_decoder_config(encoder.parameter_sets())
        } else {
            None
        };
        // Zero-copy "reverse ownership": when the encoder runs its zero-copy path
        // it exposes a dma-buf NV12 pool the compositor must render directly into.
        // `None` on the copy fallback, in which case the renderer allocates its
        // own NV12 textures and the encoder copies them in.
        let external_nv12_pool: Option<StdArc<dyn ExternalNv12FramePool>> =
            encoder.external_pool().map(|pool| {
                StdArc::new(QuickSyncNv12FramePool(pool)) as StdArc<dyn ExternalNv12FramePool>
            });

        Ok((
            Self { encoder, bitstream_format: options.bitstream_format },
            VideoEncoderConfig {
                resolution: options.resolution,
                output_format: OutputFrameFormat::Nv12WgpuTexture,
                extradata,
                external_nv12_pool,
            },
        ))
    }

    fn encode(&mut self, frame: Frame, force_keyframe: bool) -> Vec<EncodedOutputChunk> {
        let FrameData::Nv12WgpuTexture(texture) = frame.data else {
            error!("Unsupported pixel format {:?}. Dropping frame.", frame.data);
            return Vec::new();
        };

        match self.encoder.encode(
            InputFrame {
                data: (*texture).clone(),
                pts: Some(frame.pts.as_micros() as u64),
            },
            force_keyframe,
        ) {
            Ok(frames) => {
                frames.into_iter().map(|frame| self.chunk_from_frame(frame)).collect()
            }
            Err(err) => {
                error!("Intel Quick Sync encoder error: {err}");
                Vec::new()
            }
        }
    }

    fn poll_output(&mut self) -> Vec<EncodedOutputChunk> {
        match self.encoder.poll_output() {
            Ok(frames) => {
                frames.into_iter().map(|frame| self.chunk_from_frame(frame)).collect()
            }
            Err(err) => {
                error!("Intel Quick Sync encoder output poll error: {err}");
                Vec::new()
            }
        }
    }

    fn flush(&mut self) -> Vec<EncodedOutputChunk> {
        match self.encoder.flush() {
            Ok(frames) => {
                frames.into_iter().map(|frame| self.chunk_from_frame(frame)).collect()
            }
            Err(err) => {
                error!("Intel Quick Sync encoder flush error: {err}");
                Vec::new()
            }
        }
    }
}

impl QuickSyncH264Encoder {
    fn chunk_from_frame(
        &self,
        frame: H264EncodedOutputChunk<bytes::Bytes>,
    ) -> EncodedOutputChunk {
        let data = match self.bitstream_format {
            H264BitstreamFormat::AnnexB => frame.data,
            H264BitstreamFormat::Avcc => annexb_to_avcc(&frame.data),
        };
        EncodedOutputChunk {
            data,
            pts: Duration::from_micros(frame.pts),
            dts: None,
            is_keyframe: frame.is_keyframe,
            kind: MediaKind::Video(VideoCodec::H264),
        }
    }
}

/// Adapts the gpu-video Quick Sync zero-copy dma-buf pool to the smelter-render
/// [`ExternalNv12FramePool`] trait so the compositor can render NV12 output
/// directly into the encoder's surfaces.
struct QuickSyncNv12FramePool(StdArc<ZeroCopyNv12Pool>);

impl std::fmt::Debug for QuickSyncNv12FramePool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QuickSyncNv12FramePool").finish()
    }
}

impl ExternalNv12FramePool for QuickSyncNv12FramePool {
    fn acquire(&self) -> Option<ExternalNv12Frame> {
        self.0.acquire().map(|slot| ExternalNv12Frame {
            index: slot.index,
            texture: slot.texture,
        })
    }

    fn stage_write(
        &self,
        index: usize,
    ) -> Result<Box<dyn std::any::Any + Send>, String> {
        Ok(Box::new(self.0.stage_write(index)?))
    }

    fn finish_write(
        &self,
        token: Box<dyn std::any::Any + Send>,
    ) -> Result<(), String> {
        let token = token.downcast::<StagedDmaBufWrite>().map_err(|_| {
            "External NV12 finish_write received an unexpected token type".to_string()
        })?;
        self.0.finish_write(*token)
    }

    fn coded_resolution(&self) -> Resolution {
        video_resolution_to_render(self.0.coded_resolution())
    }

    fn visible_resolution(&self) -> Resolution {
        video_resolution_to_render(self.0.visible_resolution())
    }

    fn padding_luma(&self) -> f64 {
        self.0.padding_luma()
    }

    fn padding_chroma(&self) -> f64 {
        self.0.padding_chroma()
    }
}

fn video_resolution_to_render(resolution: VideoResolution) -> Resolution {
    Resolution { width: resolution.width as usize, height: resolution.height as usize }
}

fn quicksync_h264_encoder_config<'a>(
    ctx: &PipelineCtx,
    options: &QuickSyncH264EncoderOptions,
    adapter_info: &'a wgpu::AdapterInfo,
) -> Result<H264EncoderConfig<'a>, EncoderInitError> {
    let framerate = ctx.output_framerate;
    let resolution = quicksync_h264_resolution(options.resolution)?;
    let gop_size = NonZeroU16::new(
        gop_size_from_ms_framerate(options.keyframe_interval, framerate)
            .clamp(1, u16::MAX as u64) as u16,
    )
    .expect("clamped Quick Sync H264 GOP size must be non-zero");
    let quicksync_framerate = VideoFramerate::new(framerate.num, framerate.den)
        .ok_or_else(|| {
            EncoderInitError::InvalidQuickSyncH264EncoderOptions(
                "framerate numerator and denominator must be non-zero".into(),
            )
        })?;
    Ok(H264EncoderConfig {
        adapter_info,
        resolution,
        rate_control: quicksync_h264_rate_control(options, framerate)?,
        gop_size,
        framerate: quicksync_framerate,
        max_pending_frames: QUICKSYNC_MAX_PENDING_FRAMES,
        preset: quicksync_h264_preset(options.preset),
        color_space: ColorSpace::BT709,
        color_range: ColorRange::Limited,
    })
}

fn quicksync_h264_preset(preset: QuickSyncH264EncoderPreset) -> H264EncoderPreset {
    match preset {
        QuickSyncH264EncoderPreset::HighQuality => H264EncoderPreset::HighQuality,
        QuickSyncH264EncoderPreset::LowLatency => H264EncoderPreset::LowLatency,
    }
}

fn quicksync_h264_rate_control(
    options: &QuickSyncH264EncoderOptions,
    framerate: Framerate,
) -> Result<H264EncoderRateControl, EncoderInitError> {
    let bitrate = options.bitrate.unwrap_or_else(|| {
        QuickSyncH264EncoderRateControl::VariableBitrate(
            bitrate_from_resolution_framerate(options.resolution, framerate),
        )
    });
    let virtual_buffer_size = match options.preset {
        QuickSyncH264EncoderPreset::HighQuality => HIGH_QUALITY_VIRTUAL_BUFFER_SIZE,
        QuickSyncH264EncoderPreset::LowLatency => LOW_LATENCY_VIRTUAL_BUFFER_SIZE,
    };
    match bitrate {
        QuickSyncH264EncoderRateControl::VariableBitrate(bitrate) => {
            let average_bitrate =
                quicksync_h264_bitrate(bitrate.average_bitrate, "average bitrate")?;
            let max_bitrate = quicksync_h264_bitrate(bitrate.max_bitrate, "max bitrate")?;
            Ok(H264EncoderRateControl::VariableBitrate {
                bitrate: H264VariableBitrate::new(average_bitrate, max_bitrate).map_err(
                    |err| {
                        EncoderInitError::InvalidQuickSyncH264EncoderOptions(
                            err.to_string(),
                        )
                    },
                )?,
                virtual_buffer_size,
            })
        }
        QuickSyncH264EncoderRateControl::ConstantBitrate(bitrate) => {
            Ok(H264EncoderRateControl::ConstantBitrate {
                bitrate: quicksync_h264_bitrate(bitrate, "bitrate")?,
                virtual_buffer_size,
            })
        }
    }
}

fn quicksync_h264_bitrate(
    value: u64,
    label: &str,
) -> Result<NonZeroU64, EncoderInitError> {
    NonZeroU64::new(value).ok_or_else(|| {
        EncoderInitError::InvalidQuickSyncH264EncoderOptions(format!(
            "{label} must be non-zero"
        ))
    })
}

fn quicksync_h264_resolution(
    resolution: Resolution,
) -> Result<VideoResolution, EncoderInitError> {
    Ok(VideoResolution {
        width: quicksync_h264_dimension(resolution.width, "width")?,
        height: quicksync_h264_dimension(resolution.height, "height")?,
    })
}

fn quicksync_h264_dimension(value: usize, name: &str) -> Result<u32, EncoderInitError> {
    u32::try_from(value).map_err(|_| {
        EncoderInitError::InvalidQuickSyncH264EncoderOptions(format!(
            "resolution {name} exceeds u32 limit"
        ))
    })
}

fn quicksync_h264_encoder_init_error(err: QuickSyncH264EncoderError) -> EncoderInitError {
    match err {
        QuickSyncH264EncoderError::ZeroResolution(_)
        | QuickSyncH264EncoderError::OddResolution(_)
        | QuickSyncH264EncoderError::ResolutionTooLarge { .. }
        | QuickSyncH264EncoderError::BitstreamBufferTooLarge { .. } => {
            EncoderInitError::InvalidQuickSyncH264EncoderOptions(err.to_string())
        }
        err => EncoderInitError::QuickSyncH264EncoderUnavailable(err.to_string()),
    }
}
