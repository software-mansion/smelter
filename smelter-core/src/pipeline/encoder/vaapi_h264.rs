#[cfg(all(feature = "vaapi", target_os = "linux"))]
mod imp {
    use std::sync::Arc;

    use gpu_video::{
        VideoFramerate, VideoResolution,
        vaapi::h264::{EncodedFrame, H264Encoder, H264EncoderConfig},
    };
    use smelter_render::{FrameData, Framerate, OutputFrameFormat, Resolution};
    use tracing::{error, info};

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

    pub struct VaapiH264Encoder {
        encoder: H264Encoder,
        bitstream_format: H264BitstreamFormat,
    }

    impl VideoEncoder for VaapiH264Encoder {
        const LABEL: &'static str = "VA-API H264 encoder";

        type Options = VaapiH264EncoderOptions;

        fn new(
            ctx: &Arc<PipelineCtx>,
            options: Self::Options,
        ) -> Result<(Self, VideoEncoderConfig), EncoderInitError> {
            let framerate = ctx.output_framerate;
            let gop_size =
                gop_size_from_ms_framerate(options.keyframe_interval, framerate)
                    .clamp(1, u16::MAX as u64) as u16;
            let bitrate = match options.bitrate.unwrap_or_else(|| {
                let bitrate =
                    bitrate_from_resolution_framerate(options.resolution, framerate);
                VaapiH264EncoderRateControl::ConstantBitrate(bitrate.average_bitrate)
            }) {
                VaapiH264EncoderRateControl::ConstantBitrate(bitrate) => {
                    bitrate.min(u32::MAX as u64) as u32
                }
            };

            let max_pending_frames = match options.preset {
                VaapiH264EncoderPreset::HighQuality => 8,
                VaapiH264EncoderPreset::LowLatency => 1,
            };
            let video_resolution = video_resolution(options.resolution);
            let video_framerate = video_framerate(framerate);

            let encoder = H264Encoder::new(H264EncoderConfig {
                device: Arc::clone(&ctx.graphics_context.device),
                queue: Arc::clone(&ctx.graphics_context.queue),
                adapter_info: Some(ctx.graphics_context.adapter.get_info()),
                resolution: video_resolution,
                bitrate,
                gop_size,
                framerate: video_framerate,
                max_pending_frames,
            })
            .map_err(|err| {
                error!("Failed to initialize VA-API H264 encoder: {err}");
                EncoderInitError::VaapiH264EncoderUnavailable(err)
            })?;
            let extradata = (options.bitstream_format == H264BitstreamFormat::Avcc)
                .then(|| build_avc_decoder_config(encoder.parameter_sets()))
                .flatten();
            let output_format = OutputFrameFormat::Nv12DmaBuf;

            info!(
                width = options.resolution.width,
                height = options.resolution.height,
                bitrate,
                bitstream_format = ?options.bitstream_format,
                preset = ?options.preset,
                max_pending_frames,
                "Initialized VA-API H264 encoder"
            );

            Ok((
                Self { encoder, bitstream_format: options.bitstream_format },
                VideoEncoderConfig {
                    resolution: options.resolution,
                    output_format,
                    extradata,
                },
            ))
        }

        fn encode(
            &mut self,
            frame: Frame,
            force_keyframe: bool,
        ) -> Vec<EncodedOutputChunk> {
            let FrameData::Nv12DmaBuf(dmabuf) = frame.data else {
                error!("Unsupported pixel format {:?}. Dropping frame.", frame.data);
                return Vec::new();
            };

            match self.encoder.encode(dmabuf, frame.pts, force_keyframe) {
                Ok(frames) => {
                    frames.into_iter().map(|frame| self.chunk_from_frame(frame)).collect()
                }
                Err(err) => {
                    error!("VA-API encoder error: {err}");
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
                    error!("VA-API encoder flush error: {err}");
                    Vec::new()
                }
            }
        }
    }

    impl VaapiH264Encoder {
        fn chunk_from_frame(&self, frame: EncodedFrame) -> EncodedOutputChunk {
            let data = if self.bitstream_format == H264BitstreamFormat::Avcc {
                annexb_to_avcc(&frame.data)
            } else {
                frame.data
            };
            EncodedOutputChunk {
                data,
                pts: frame.pts,
                dts: None,
                is_keyframe: frame.is_keyframe,
                kind: MediaKind::Video(VideoCodec::H264),
            }
        }
    }

    fn video_resolution(resolution: Resolution) -> VideoResolution {
        VideoResolution {
            width: resolution.width as u32,
            height: resolution.height as u32,
        }
    }

    fn video_framerate(framerate: Framerate) -> VideoFramerate {
        VideoFramerate { num: framerate.num, den: framerate.den }
    }
}

#[cfg(all(feature = "vaapi", target_os = "linux"))]
pub use imp::VaapiH264Encoder;

#[cfg(not(all(feature = "vaapi", target_os = "linux")))]
mod imp {
    use std::sync::Arc;

    use smelter_render::Frame;

    use crate::{
        pipeline::encoder::{VideoEncoder, VideoEncoderConfig},
        prelude::*,
    };

    pub struct VaapiH264Encoder;

    impl VideoEncoder for VaapiH264Encoder {
        const LABEL: &'static str = "VA-API H264 encoder";

        type Options = VaapiH264EncoderOptions;

        fn new(
            _ctx: &Arc<PipelineCtx>,
            _options: Self::Options,
        ) -> Result<(Self, VideoEncoderConfig), EncoderInitError> {
            Err(EncoderInitError::VaapiH264EncoderUnavailable(
                "support was not compiled into smelter-core".into(),
            ))
        }

        fn encode(
            &mut self,
            _frame: Frame,
            _force_keyframe: bool,
        ) -> Vec<EncodedOutputChunk> {
            Vec::new()
        }

        fn flush(&mut self) -> Vec<EncodedOutputChunk> {
            Vec::new()
        }
    }
}

#[cfg(not(all(feature = "vaapi", target_os = "linux")))]
pub use imp::VaapiH264Encoder;
