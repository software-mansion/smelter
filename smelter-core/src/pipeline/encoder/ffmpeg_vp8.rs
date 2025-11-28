use std::{iter, sync::Arc};

use ffmpeg_next::{
    Rational,
    codec::{Context, Id},
    format::Pixel,
};
use smelter_render::{Frame, OutputFrameFormat};
use tracing::{error, info, trace, warn};

use crate::pipeline::{
    encoder::ffmpeg_utils::{create_av_frame, encoded_chunk_from_av_packet},
    ffmpeg_utils::FfmpegOptions,
};
use crate::prelude::*;

use super::{VideoEncoder, VideoEncoderConfig};

const TIME_BASE: i32 = 1_000_000;

pub struct FfmpegVp8Encoder {
    encoder: ffmpeg_next::encoder::Video,
    packet: ffmpeg_next::Packet,
}

impl VideoEncoder for FfmpegVp8Encoder {
    const LABEL: &'static str = "FFmpeg VP8 encoder";

    type Options = FfmpegVp8EncoderOptions;

    fn new(
        ctx: &Arc<PipelineCtx>,
        options: Self::Options,
    ) -> Result<(Self, VideoEncoderConfig), EncoderInitError> {
        info!(?options, "Initializing FFmpeg VP8 encoder");
        let codec = ffmpeg_next::codec::encoder::find(Id::VP8).ok_or(EncoderInitError::NoCodec)?;

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
            // TODO: This is temporary value and requires more research on
            // what the default should be, definitely not fixed size, rather fixed time
            ("g", "250"),
            // Quality/Speed ratio modifier
            ("cpu-used", "0"),
            // Time to spend encoding.
            ("deadline", "realtime"),
            // Auto threads number used.
            ("threads", "0"),
            // Zero-latency. Disables frame reordering.
            ("lag-in-frames", "0"),
            // Min QP
            ("qmin", "4"),
            // Max QP
            ("qmax", "63"),
        ]);
        if let Some(bitrate) = options.bitrate {
            let b = bitrate.average_bitrate;
            let maxrate = bitrate.max_bitrate;

            // FFmpeg takes bufsize as bits. Setting it to the same value as `average_bitrate`
            // will make it to be set to 1000ms.
            let bufsize = bitrate.average_bitrate;
            ffmpeg_options.append(&[
                // Bitrate in b/s
                ("b", &b.to_string()),
                // Maximum bitrate allowed at spikes for vbr mode
                ("maxrate", &maxrate.to_string()),
                // Time period to calculate average bitrate from calculated as
                // bufsize * 1000 / bitrate
                ("bufsize", &bufsize.to_string()),
            ]);
        }
        ffmpeg_options.append(&options.raw_options);

        let encoder = encoder.open_as_with(codec, ffmpeg_options.into_dictionary())?;

        Ok((
            Self {
                encoder,
                packet: ffmpeg_next::Packet::empty(),
            },
            VideoEncoderConfig {
                resolution: options.resolution,
                output_format: OutputFrameFormat::PlanarYuv420Bytes,
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

impl FfmpegVp8Encoder {
    fn read_all_chunks(&mut self) -> Vec<EncodedOutputChunk> {
        iter::from_fn(|| {
            match self.encoder.receive_packet(&mut self.packet) {
                Ok(_) => {
                    match encoded_chunk_from_av_packet(
                        &self.packet,
                        MediaKind::Video(VideoCodec::Vp9),
                        TIME_BASE,
                    ) {
                        Ok(chunk) => {
                            trace!(pts=?self.packet.pts(), ?chunk, "VP8 encoder produced an encoded packet.");
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
