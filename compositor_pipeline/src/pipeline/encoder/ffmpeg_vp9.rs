use std::{iter, sync::Arc, time::Duration};

use compositor_render::{Frame, FrameData, Resolution};
use ffmpeg_next::{
    codec::{Context, Id},
    format::Pixel,
    frame, Dictionary, Packet, Rational,
};
use tracing::{error, info, trace, warn};

use crate::{
    error::EncoderInitError,
    pipeline::{
        types::{ChunkFromFfmpegError, EncodedChunk, EncodedChunkKind, IsKeyframe, VideoCodec},
        PipelineCtx,
    },
};

use super::OutputPixelFormat;

use super::{VideoEncoder, VideoEncoderConfig};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Options {
    pub resolution: Resolution,
    pub pixel_format: OutputPixelFormat,
    pub raw_options: Vec<(String, String)>,
}

pub struct FfmpegVp9Encoder {
    encoder: ffmpeg_next::encoder::Video,
    packet: Packet,
    resolution: Resolution,
    pixel_format: OutputPixelFormat,
}

impl VideoEncoder for FfmpegVp9Encoder {
    const LABEL: &'static str = "FFmpeg VP9 encoder";

    type Options = Options;

    fn new(
        ctx: &Arc<PipelineCtx>,
        options: Self::Options,
    ) -> Result<(Self, VideoEncoderConfig), EncoderInitError> {
        info!(?options, "Initializing FFmpeg VP9 encoder");
        let codec = ffmpeg_next::codec::encoder::find(Id::VP9).ok_or(EncoderInitError::NoCodec)?;

        let mut encoder = Context::new().encoder().video()?;

        // We set this to 1 / 1_000_000, bc we use `as_micros` to convert frames to AV packets.
        let pts_unit_secs = Rational::new(1, 1_000_000);
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
        let defaults = [
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
        ];

        let encoder_opts_iter = merge_options_with_defaults(&defaults, &options.raw_options);
        let encoder = encoder.open_as_with(codec, Dictionary::from_iter(encoder_opts_iter))?;

        Ok((
            Self {
                encoder,
                packet: Packet::empty(),
                resolution: options.resolution,
                pixel_format: options.pixel_format,
            },
            VideoEncoderConfig {
                resolution: options.resolution,
                output_format: options.pixel_format.into(),
                extradata: None,
            },
        ))
    }

    fn encode(&mut self, frame: Frame, force_keyframe: bool) -> Vec<EncodedChunk> {
        let mut av_frame = frame::Video::new(
            self.pixel_format.into(),
            self.resolution.width as u32,
            self.resolution.height as u32,
        );

        if let Err(e) = frame_into_av(frame, &mut av_frame) {
            error!(
                "Failed to convert a frame to an ffmpeg frame: {}. Dropping",
                e.0
            );
        }

        if force_keyframe {
            av_frame.set_kind(ffmpeg_next::picture::Type::I);
        }

        if let Err(e) = self.encoder.send_frame(&av_frame) {
            error!("Encoder error: {e}.");
            return vec![];
        }
        self.read_all_chunks()
    }

    fn flush(&mut self) -> Vec<EncodedChunk> {
        if let Err(e) = self.encoder.send_eof() {
            error!("Failed to enter draining mode on encoder: {e}.");
        }
        self.read_all_chunks()
    }
}

impl FfmpegVp9Encoder {
    fn read_all_chunks(&mut self) -> Vec<EncodedChunk> {
        iter::from_fn(|| {
            match self.encoder.receive_packet(&mut self.packet) {
                Ok(_) => {
                    match encoded_chunk_from_av_packet(
                        &self.packet,
                        EncodedChunkKind::Video(VideoCodec::Vp9),
                        1_000_000,
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

#[derive(Debug)]
struct FrameConversionError(String);

fn frame_into_av(frame: Frame, av_frame: &mut frame::Video) -> Result<(), FrameConversionError> {
    let (data, expected_pixel_format) = match frame.data {
        FrameData::PlanarYuv420(data) => (data, Pixel::YUV420P),
        FrameData::PlanarYuv422(data) => (data, Pixel::YUV422P),
        FrameData::PlanarYuv444(data) => (data, Pixel::YUV444P),
        _ => {
            return Err(FrameConversionError(format!(
                "Unsupported pixel format {:?}",
                frame.data
            )))
        }
    };

    if av_frame.format() != expected_pixel_format {
        return Err(FrameConversionError(format!(
            "Frame format mismatch: expected {:?}, got {:?}",
            expected_pixel_format,
            av_frame.format()
        )));
    }

    let expected_y_plane_size = (av_frame.plane_width(0) * av_frame.plane_height(0)) as usize;
    let expected_u_plane_size = (av_frame.plane_width(1) * av_frame.plane_height(1)) as usize;
    let expected_v_plane_size = (av_frame.plane_width(2) * av_frame.plane_height(2)) as usize;
    if expected_y_plane_size != data.y_plane.len() {
        return Err(FrameConversionError(format!(
            "Y plane is a wrong size, expected: {} received: {}",
            expected_y_plane_size,
            data.y_plane.len()
        )));
    }
    if expected_u_plane_size != data.u_plane.len() {
        return Err(FrameConversionError(format!(
            "U plane is a wrong size, expected: {} received: {}",
            expected_u_plane_size,
            data.u_plane.len()
        )));
    }
    if expected_v_plane_size != data.v_plane.len() {
        return Err(FrameConversionError(format!(
            "V plane is a wrong size, expected: {} received: {}",
            expected_v_plane_size,
            data.v_plane.len()
        )));
    }

    av_frame.set_pts(Some(frame.pts.as_micros() as i64));

    write_plane_to_av(av_frame, 0, &data.y_plane);
    write_plane_to_av(av_frame, 1, &data.u_plane);
    write_plane_to_av(av_frame, 2, &data.v_plane);

    Ok(())
}

fn write_plane_to_av(frame: &mut frame::Video, plane: usize, data: &[u8]) {
    let stride = frame.stride(plane);
    let width = frame.plane_width(plane) as usize;

    data.chunks(width)
        .zip(frame.data_mut(plane).chunks_mut(stride))
        .for_each(|(data, target)| target[..width].copy_from_slice(data));
}

fn merge_options_with_defaults<'a>(
    defaults: &'a [(&str, &str)],
    overrides: &'a [(String, String)],
) -> impl Iterator<Item = (&'a str, &'a str)> {
    defaults
        .iter()
        .copied()
        .filter(|(key, _value)| {
            // filter out any defaults that are in overrides
            !overrides
                .iter()
                .any(|(override_key, _)| key == override_key)
        })
        .chain(
            overrides
                .iter()
                .map(|(key, value)| (key.as_str(), value.as_str())),
        )
}

fn encoded_chunk_from_av_packet(
    value: &ffmpeg_next::Packet,
    kind: EncodedChunkKind,
    timescale: i64,
) -> Result<EncodedChunk, ChunkFromFfmpegError> {
    let data = match value.data() {
        Some(data) => bytes::Bytes::copy_from_slice(data),
        None => return Err(ChunkFromFfmpegError::NoData),
    };

    let rescale = |v: i64| Duration::from_secs_f64((v as f64) * (1.0 / timescale as f64));

    Ok(EncodedChunk {
        data,
        pts: value
            .pts()
            .map(rescale)
            .ok_or(ChunkFromFfmpegError::NoPts)?,
        dts: value.dts().map(rescale),
        is_keyframe: if value.flags().contains(ffmpeg_next::packet::Flags::KEY) {
            IsKeyframe::Yes
        } else {
            IsKeyframe::No
        },
        kind,
    })
}
