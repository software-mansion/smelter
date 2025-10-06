use std::{iter, sync::Arc};

use ffmpeg_next::codec::Id;
use ffmpeg_next::{Rational, codec::Context, format::Pixel};
use smelter_render::{Frame, OutputFrameFormat};
use tracing::{error, info, trace, warn};

use crate::pipeline::encoder::ffmpeg_utils::{
    FfmpegOptions, create_av_frame, encoded_chunk_from_av_packet, read_extradata,
};
use crate::prelude::*;

use super::{VideoEncoder, VideoEncoderConfig};

const TIME_BASE: i32 = 1_000_000;

pub struct FfmpegH264Encoder {
    encoder: ffmpeg_next::encoder::Video,
    packet: ffmpeg_next::Packet,
}

impl VideoEncoder for FfmpegH264Encoder {
    const LABEL: &'static str = "FFmpeg H264 encoder";

    type Options = FfmpegH264EncoderOptions;

    fn new(
        ctx: &Arc<PipelineCtx>,
        options: FfmpegH264EncoderOptions,
    ) -> Result<(Self, VideoEncoderConfig), EncoderInitError> {
        info!(?options, "Initialize FFmpeg H264 encoder");
        let codec = ffmpeg_next::codec::encoder::find(Id::H264).ok_or(EncoderInitError::NoCodec)?;
        let codec_name = codec.name();

        let mut encoder = Context::new().encoder().video()?;

        let pts_unit_secs = Rational::new(1, TIME_BASE);
        let framerate = ctx.output_framerate;
        encoder.set_time_base(pts_unit_secs);
        encoder.set_format(Pixel::YUV420P);
        encoder.set_width(options.resolution.width as u32);
        encoder.set_height(options.resolution.height as u32);
        encoder.set_frame_rate(Some((framerate.num as i32, framerate.den as i32)));
        encoder.set_colorspace(ffmpeg_next::color::Space::BT709);
        encoder.set_color_range(ffmpeg_next::color::Range::MPEG);
        unsafe {
            let encoder = encoder.as_mut_ptr();
            use ffmpeg_next::ffi;
            (*encoder).color_primaries = ffi::AVColorPrimaries::AVCOL_PRI_BT709;
            (*encoder).color_trc = ffi::AVColorTransferCharacteristic::AVCOL_TRC_BT709;
        }

        let mut ffmpeg_options = FfmpegOptions::from(&[
            ("preset", preset_to_str(options.preset)),
            // Auto number of threads
            ("threads", "0"),
        ]);
        if codec_name != "libopenh264" {
            ffmpeg_options.append(&[
                // Quality-based VBR (0-51)
                ("crf", "23"),
                // QP curve compression
                ("qcomp", "0.6"),
                //  Maximum motion vector search range
                ("me_range", "16"),
                // Max QP step
                ("qdiff", "4"),
                // Min QP
                ("qmin", "0"),
                // Max QP
                ("qmax", "69"),
                // Maximum GOP (Group of Pictures) size - number of frames between keyframe
                ("g", "250"),
                // QP factor between I and P frames
                ("i_qfactor", "1.4"),
                // QP factor between P and B frames
                ("f_pb_factor", "1.3"),
                // A comma-separated list of partitions to consider. Possible values: p8x8, p4x4, b8x8, i8x8, i4x4, none, all
                ("partitions", default_partitions_for_preset(options.preset)),
                // Subpixel motion estimation and mode decision (decision quality: 1=fast, 11=best)
                ("subq", default_subq_mode_for_preset(options.preset)),
            ]);
        };
        ffmpeg_options.append(&options.raw_options);

        let encoder = encoder.open_as_with(codec, ffmpeg_options.into_dictionary())?;
        let extradata = read_extradata(&encoder);

        Ok((
            Self {
                encoder,
                packet: ffmpeg_next::Packet::empty(),
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
                        Ok(chunk) => {
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

fn preset_to_str(preset: FfmpegH264EncoderPreset) -> &'static str {
    match preset {
        FfmpegH264EncoderPreset::Ultrafast => "ultrafast",
        FfmpegH264EncoderPreset::Superfast => "superfast",
        FfmpegH264EncoderPreset::Veryfast => "veryfast",
        FfmpegH264EncoderPreset::Faster => "faster",
        FfmpegH264EncoderPreset::Fast => "fast",
        FfmpegH264EncoderPreset::Medium => "medium",
        FfmpegH264EncoderPreset::Slow => "slow",
        FfmpegH264EncoderPreset::Slower => "slower",
        FfmpegH264EncoderPreset::Veryslow => "veryslow",
        FfmpegH264EncoderPreset::Placebo => "placebo",
    }
}

fn default_partitions_for_preset(preset: FfmpegH264EncoderPreset) -> &'static str {
    match preset {
        FfmpegH264EncoderPreset::Ultrafast => "none",
        FfmpegH264EncoderPreset::Superfast => "i8x8,i4x4",
        FfmpegH264EncoderPreset::Veryfast => "p8x8,b8x8,i8x8,i4x4",
        FfmpegH264EncoderPreset::Faster => "p8x8,b8x8,i8x8,i4x4",
        FfmpegH264EncoderPreset::Fast => "p8x8,b8x8,i8x8,i4x4",
        FfmpegH264EncoderPreset::Medium => "p8x8,b8x8,i8x8,i4x4",
        FfmpegH264EncoderPreset::Slow => "all",
        FfmpegH264EncoderPreset::Slower => "all",
        FfmpegH264EncoderPreset::Veryslow => "all",
        FfmpegH264EncoderPreset::Placebo => "all",
    }
}

fn default_subq_mode_for_preset(preset: FfmpegH264EncoderPreset) -> &'static str {
    match preset {
        FfmpegH264EncoderPreset::Ultrafast => "0",
        FfmpegH264EncoderPreset::Superfast => "1",
        FfmpegH264EncoderPreset::Veryfast => "2",
        FfmpegH264EncoderPreset::Faster => "4",
        FfmpegH264EncoderPreset::Fast => "6",
        FfmpegH264EncoderPreset::Medium => "7",
        FfmpegH264EncoderPreset::Slow => "8",
        FfmpegH264EncoderPreset::Slower => "9",
        FfmpegH264EncoderPreset::Veryslow => "10",
        FfmpegH264EncoderPreset::Placebo => "11",
    }
}
