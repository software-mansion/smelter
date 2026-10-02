use std::{iter, sync::Arc};

use ffmpeg_next::codec::Id;
use ffmpeg_next::{Rational, codec::Context};
use smelter_render::OutputFrameFormat;
use tracing::{error, info, trace, warn};

use crate::pipeline::encoder::ffmpeg_utils::{
    create_av_frame, encoded_chunk_from_av_packet, into_ffmpeg_pixel_format, open_video_encoder,
};
use crate::pipeline::encoder::utils::{
    bitrate_from_resolution_framerate, gop_size_from_ms_framerate,
};
use crate::pipeline::ffmpeg_utils::{FfmpegOptions, ReadExtradataExt};
use crate::pipeline::utils::{annexb_to_avcc, build_avc_decoder_config};
use crate::prelude::*;

use super::{VideoEncoder, VideoEncoderConfig};

const TIME_BASE: i32 = 1_000_000;

pub struct FfmpegH264Encoder {
    encoder: ffmpeg_next::encoder::Video,
    packet: ffmpeg_next::Packet,
    bitstream_format: H264BitstreamFormat,
}

impl VideoEncoder for FfmpegH264Encoder {
    const LABEL: &'static str = "FFmpeg H264 encoder";

    type Options = FfmpegH264EncoderOptions;

    fn new(
        ctx: &Arc<PipelineCtx>,
        options: FfmpegH264EncoderOptions,
    ) -> Result<(Self, VideoEncoderConfig), EncoderInitError> {
        info!(?options, "Initialize FFmpeg H264 encoder");
        let codec = match &options.encoder_name {
            Some(name) => ffmpeg_next::codec::encoder::find_by_name(name)
                .filter(|codec| codec.id() == Id::H264)
                .ok_or_else(|| EncoderInitError::NoH264CodecWithName(name.clone()))?,
            None => ffmpeg_next::codec::encoder::find(Id::H264).ok_or(EncoderInitError::NoCodec)?,
        };
        let codec_name = codec.name();
        info!(h264_encoder = codec_name, "Selected FFmpeg H264 encoder");

        // Allocating with the codec applies its own defaults, generic AVCodecContext defaults would
        // override x264 presets.
        let mut encoder = Context::new_with_codec(codec).encoder().video()?;

        let pts_unit_secs = Rational::new(1, TIME_BASE);
        let framerate = ctx.output_framerate;
        encoder.set_time_base(pts_unit_secs);
        encoder.set_format(into_ffmpeg_pixel_format(options.pixel_format));
        encoder.set_width(options.resolution.width as u32);
        encoder.set_height(options.resolution.height as u32);
        encoder.set_frame_rate(Some((framerate.num as i32, framerate.den as i32)));
        encoder.set_colorspace(ffmpeg_next::color::Space::BT709);
        encoder.set_color_range(ffmpeg_next::color::Range::MPEG);
        if options.bitstream_format == H264BitstreamFormat::Avcc {
            encoder.set_flags(ffmpeg_next::codec::Flags::GLOBAL_HEADER);
        }
        unsafe {
            let encoder = encoder.as_mut_ptr();
            use ffmpeg_next::ffi;
            (*encoder).color_primaries = ffi::AVColorPrimaries::AVCOL_PRI_BT709;
            (*encoder).color_trc = ffi::AVColorTransferCharacteristic::AVCOL_TRC_BT709;
        }

        let ffmpeg_options = initialize_ffmpeg_h264_options(ctx, &options, codec_name);

        let encoder = open_video_encoder(encoder, codec, ffmpeg_options)?;
        let extradata = encoder
            .read_extradata()
            .and_then(|extradata| build_avc_decoder_config(&extradata));

        Ok((
            Self {
                encoder,
                packet: ffmpeg_next::Packet::empty(),
                bitstream_format: options.bitstream_format,
            },
            VideoEncoderConfig {
                resolution: options.resolution,
                output_format: match options.pixel_format {
                    OutputPixelFormat::YUV420P => OutputFrameFormat::PlanarYuv420Bytes,
                    OutputPixelFormat::YUV422P => OutputFrameFormat::PlanarYuv422Bytes,
                    OutputPixelFormat::YUV444P => OutputFrameFormat::PlanarYuv444Bytes,
                },
                extradata,
            },
        ))
    }

    fn encode(&mut self, frame: Frame, force_keyframe: bool) -> Vec<EncodedOutputChunk> {
        trace!(?frame, "FFmpeg H264 encoder received a frame.");
        let mut av_frame = match create_av_frame(frame, TIME_BASE) {
            Ok(av_frame) => av_frame,
            Err(e) => {
                error!("{e}. Dropping frame.");
                return Vec::new();
            }
        };

        if force_keyframe {
            av_frame.set_kind(ffmpeg_next::picture::Type::I);
        }

        if let Err(e) = self.encoder.send_frame(&av_frame) {
            error!("Encoder error: {e}.");
            return vec![];
        }
        self.read_all_chunks()
    }

    fn flush(&mut self) -> Vec<EncodedOutputChunk> {
        if let Err(e) = self.encoder.send_eof() {
            error!("Failed to enter draining mode on encoder: {e}.");
        }
        self.read_all_chunks()
    }
}

impl FfmpegH264Encoder {
    fn read_all_chunks(&mut self) -> Vec<EncodedOutputChunk> {
        iter::from_fn(|| {
            match self.encoder.receive_packet(&mut self.packet) {
                Ok(_) => {
                    match encoded_chunk_from_av_packet(
                        &self.packet,
                        MediaKind::Video(VideoCodec::H264),
                        TIME_BASE,
                    ) {
                        Ok(mut chunk) => {
                            if self.bitstream_format == H264BitstreamFormat::Avcc {
                                chunk.data = annexb_to_avcc(&chunk.data);
                            };
                            trace!(pts=?self.packet.pts(), ?chunk, "H264 encoder produced an encoded packet.");
                            Some(chunk)
                        }
                        Err(e) => {
                            warn!("failed to parse an ffmpeg packet received from encoder: {e}",);
                            None
                        }
                    }
                }

                Err(ffmpeg_next::Error::Eof) => None,

                Err(ffmpeg_next::Error::Other {
                    errno: ffmpeg_next::error::EAGAIN,
                }) => None, // encoder needs more frames to produce a packet

                Err(e) => {
                    error!("Encoder error: {e}.");
                    None
                }
            }
        }).collect()
    }
}

fn initialize_ffmpeg_h264_options(
    ctx: &Arc<PipelineCtx>,
    options: &FfmpegH264EncoderOptions,
    encoder_name: &str,
) -> FfmpegOptions {
    let mut ffmpeg_options = FfmpegOptions::default();
    match encoder_name {
        "libopenh264" => {
            ffmpeg_options.append(&[
                // Min QP. QP represents the video quality.
                ("qmin", "4"),
                // Max QP. Range is increased compared to encoder defaults to allow
                // low bitrate without dropping frames.
                ("qmax", "51"),
                // Rate control mode (0 - quality, 1 - bitrate)
                ("rc_mode", "0"),
                // Auto number of threads
                ("threads", "0"),
            ]);
            let bitrate = options.bitrate.unwrap_or_else(|| {
                bitrate_from_resolution_framerate(options.resolution, ctx.output_framerate)
            });
            let b = bitrate.average_bitrate;
            let maxrate = bitrate.max_bitrate;

            ffmpeg_options.append(&[
                // Bitrate in b/s
                ("b", &b.to_string()),
                // Maximum bitrate. Higher values allow short spikes of bitrate.
                ("maxrate", &maxrate.to_string()),
            ]);
        }
        "h264_videotoolbox" => {
            ffmpeg_options.append(&[
                // Min QP. QP represents the video quality.
                ("qmin", "4"),
                // Max QP. Range is increased compared to encoder defaults to allow
                // low bitrate without dropping frames.
                ("qmax", "51"),
                // Disable b frames
                ("bf", "0"),
            ]);
            let bitrate = options.bitrate.unwrap_or_else(|| {
                bitrate_from_resolution_framerate(options.resolution, ctx.output_framerate)
            });
            let b = bitrate.average_bitrate;
            let maxrate = bitrate.max_bitrate;

            ffmpeg_options.append(&[
                // Bitrate in b/s
                ("b", &b.to_string()),
                // Maximum bitrate. Higher values allow short spikes of bitrate.
                ("maxrate", &maxrate.to_string()),
            ]);
        }
        "libx264" => {
            ffmpeg_options.append(&[("preset", "fast")]);
            if options.low_latency {
                ffmpeg_options.append(&[
                    ("tune", "zerolatency"),
                    // Disable b frames
                    ("bf", "0"),
                    ("thread_type", "slice"),
                ]);
            }
            match options.bitrate {
                Some(bitrate) => {
                    let b = bitrate.average_bitrate;
                    let maxrate = bitrate.max_bitrate;
                    // Since FFmpeg takes bits, setting this to average_bitrate results in a 1000ms buffer.
                    let bufsize = bitrate.average_bitrate;
                    ffmpeg_options.append(&[
                        // Bitrate in b/s
                        ("b", &b.to_string()),
                        // Maximum bitrate. Higher values allow short spikes of bitrate.
                        ("maxrate", &maxrate.to_string()),
                        // Buffer to calculate average bitrate from.
                        ("bufsize", &bufsize.to_string()),
                    ]);
                }
                None => {
                    // Quality-based VBR (0-51), default if bitrate is not set
                    ffmpeg_options.append(&[("crf", "23")]);
                }
            }
        }
        _ => {
            if options.low_latency {
                // Disable b frames
                ffmpeg_options.append(&[("bf", "0")]);
            }
            if let Some(bitrate) = options.bitrate {
                ffmpeg_options.append(&[
                    // Bitrate in b/s
                    ("b", &bitrate.average_bitrate.to_string()),
                    // Maximum bitrate. Higher values allow short spikes of bitrate.
                    ("maxrate", &bitrate.max_bitrate.to_string()),
                ]);
            }
        }
    }
    let gop_size = gop_size_from_ms_framerate(options.keyframe_interval, ctx.output_framerate);
    ffmpeg_options.append(&[
        // Max distance between keyframes in bits, default is equivalent of 5000 ms.
        ("g", &gop_size.to_string()),
    ]);
    ffmpeg_options.append(&options.raw_options);
    ffmpeg_options
}
