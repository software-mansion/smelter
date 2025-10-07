use std::{iter, sync::Arc};

use ffmpeg_next::{
    Rational,
    codec::{Context, Id},
    format::Pixel,
};
use smelter_render::{Frame, OutputFrameFormat};
use tracing::{error, info, trace, warn};

use crate::pipeline::{
    PipelineCtx,
    encoder::ffmpeg_utils::{FfmpegOptions, create_av_frame, encoded_chunk_from_av_packet},
};
use crate::prelude::*;

use super::{VideoEncoder, VideoEncoderConfig};

const TIME_BASE: i32 = 1_000_000;

pub struct FfmpegVp9Encoder {
    encoder: ffmpeg_next::encoder::Video,
    packet: ffmpeg_next::Packet,
}

impl VideoEncoder for FfmpegVp9Encoder {
    const LABEL: &'static str = "FFmpeg VP9 encoder";

    type Options = FfmpegVp9EncoderOptions;

    fn new(
        ctx: &Arc<PipelineCtx>,
        options: Self::Options,
    ) -> Result<(Self, VideoEncoderConfig), EncoderInitError> {
        info!(?options, "Initializing FFmpeg VP9 encoder");
        let codec = ffmpeg_next::codec::encoder::find(Id::VP9).ok_or(EncoderInitError::NoCodec)?;

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

        // configuration based on https://developers.google.com/media/vp9/live-encoding
        let mut ffmpeg_options = FfmpegOptions::from(&[
            // Quality/Speed ratio modifier
            ("speed", "5"),
            // Time to spend encoding.
            ("quality", "realtime"),
            // Tiling splits the video into rectangular regions, which allows multi-threading for encoding and decoding.
            ("title-columns", "2"),
            // Enable parallel decodability features.
            ("frame-parallel", "1"),
            // Auto number of threads to use.
            ("threads", "0"),
            // Minimum value for the quantizer.
            ("qmin", "4"),
            // Mazimum value for the quantizer.
            ("qmax", "48"),
            // Enable row-multithreading. Allows use of up to 2x thread as tile columns. 0 = off, 1 = on.
            ("row-mt", "1"),
            // Enable error resiliency features.
            ("error-resilient", "1"),
            // Maximum number of frames to lag
            ("lag-in-frames", "0"),
        ]);
        ffmpeg_options.append(&options.raw_options);

        let encoder = encoder.open_as_with(codec, ffmpeg_options.into_dictionary())?;

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
                extradata: None,
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

impl FfmpegVp9Encoder {
    fn read_all_chunks(&mut self) -> Vec<EncodedOutputChunk> {
        iter::from_fn(|| {
            match self.encoder.receive_packet(&mut self.packet) {
                Ok(_) => {
                    match encoded_chunk_from_av_packet(
                        &self.packet,
                        MediaKind::Video(VideoCodec::Vp9),
                        TIME_BASE
                    ) {
                        Ok(chunk) => {
                            trace!(pts=?self.packet.pts(), ?chunk, "VP9 encoder produced an encoded packet.");
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
