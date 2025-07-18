use std::{iter, sync::Arc, time::Duration};

use crate::{
    error::DecoderInitError,
    pipeline::{
        decoder::{VideoDecoder, VideoDecoderInstance},
        types::{EncodedChunk, EncodedChunkKind, VideoCodec},
        PipelineCtx,
    },
};

use compositor_render::{Frame, FrameData, Resolution, YuvPlanes};
use ffmpeg_next::{
    codec::{Context, Id},
    format::Pixel,
    frame::Video,
    media::Type,
    Rational,
};
use tracing::{error, info, trace, warn};

pub struct FfmpegH264Decoder {
    decoder: ffmpeg_next::decoder::Opened,
    av_frame: ffmpeg_next::frame::Video,
}

impl VideoDecoder for FfmpegH264Decoder {
    const LABEL: &'static str = "FFmpeg H264 decoder";

    fn new(_ctx: &Arc<PipelineCtx>) -> Result<Self, DecoderInitError> {
        info!("Initializing FFmpeg H264 decoder");
        let mut parameters = ffmpeg_next::codec::Parameters::new();
        unsafe {
            let parameters = &mut *parameters.as_mut_ptr();

            parameters.codec_type = Type::Video.into();
            parameters.codec_id = Id::H264.into();
        };

        let mut decoder = Context::from_parameters(parameters)?;
        unsafe {
            // This is because we use microseconds as pts and dts in the packets.
            // See `chunk_to_av` and `frame_from_av`.
            (*decoder.as_mut_ptr()).pkt_timebase = Rational::new(1, 1_000_000).into();
        }

        let decoder = decoder.decoder();
        let decoder = decoder.open_as(Id::H264)?;
        Ok(Self {
            decoder,
            av_frame: ffmpeg_next::frame::Video::empty(),
        })
    }
}

impl VideoDecoderInstance for FfmpegH264Decoder {
    fn decode(&mut self, chunk: EncodedChunk) -> Vec<Frame> {
        if chunk.kind != EncodedChunkKind::Video(VideoCodec::H264) {
            error!(
                "H264 decoder received chunk of wrong kind: {:?}",
                chunk.kind
            );
            return Vec::new();
        }

        let av_packet: ffmpeg_next::Packet = match chunk_to_av(chunk) {
            Ok(packet) => packet,
            Err(err) => {
                warn!("Dropping frame: {}", err);
                return Vec::new();
            }
        };

        match self.decoder.send_packet(&av_packet) {
            Ok(()) => {}
            Err(e) => {
                warn!("Failed to send a packet to decoder: {:?}", e);
                return Vec::new();
            }
        }
        self.read_all_frames()
    }

    fn flush(&mut self) -> Vec<Frame> {
        self.decoder.flush();
        self.read_all_frames()
    }
}

impl FfmpegH264Decoder {
    fn read_all_frames(&mut self) -> Vec<Frame> {
        iter::from_fn(|| {
            match self.decoder.receive_frame(&mut self.av_frame) {
                Ok(_) => match frame_from_av(&mut self.av_frame) {
                    Ok(frame) => {
                        trace!(pts=?frame.pts, "H264 decoder produced a frame.");
                        Some(frame)
                    }
                    Err(err) => {
                        warn!("Dropping frame: {}", err);
                        None
                    }
                },
                Err(ffmpeg_next::Error::Eof) => None,
                Err(ffmpeg_next::Error::Other {
                    errno: ffmpeg_next::error::EAGAIN,
                }) => None, // decoder needs more chunks to produce frame
                Err(e) => {
                    error!("Decoder error: {e}.");
                    None
                }
            }
        })
        .collect()
    }
}

#[derive(Debug, thiserror::Error)]
enum DecoderChunkConversionError {
    #[error(
        "Cannot send a chunk of kind {0:?} to the decoder. The decoder only handles H264-encoded video."
    )]
    BadPayloadType(EncodedChunkKind),
}

fn chunk_to_av(chunk: EncodedChunk) -> Result<ffmpeg_next::Packet, DecoderChunkConversionError> {
    if chunk.kind != EncodedChunkKind::Video(VideoCodec::H264) {
        return Err(DecoderChunkConversionError::BadPayloadType(chunk.kind));
    }

    let mut packet = ffmpeg_next::Packet::new(chunk.data.len());

    packet.data_mut().unwrap().copy_from_slice(&chunk.data);
    packet.set_pts(Some(chunk.pts.as_micros() as i64));
    packet.set_dts(chunk.dts.map(|dts| dts.as_micros() as i64));

    Ok(packet)
}

#[derive(Debug, thiserror::Error)]
enum DecoderFrameConversionError {
    #[error("Error converting frame: {0}")]
    FrameConversionError(String),
    #[error("Unsupported pixel format: {0:?}")]
    UnsupportedPixelFormat(ffmpeg_next::format::pixel::Pixel),
}

fn frame_from_av(decoded: &mut Video) -> Result<Frame, DecoderFrameConversionError> {
    let Some(pts) = decoded.pts() else {
        return Err(DecoderFrameConversionError::FrameConversionError(
            "missing pts".to_owned(),
        ));
    };
    if pts < 0 {
        error!(pts, "Received negative PTS. PTS values of the decoder output are not monotonically increasing.")
    }
    let pts = Duration::from_micros(i64::max(pts, 0) as u64);
    let data = match decoded.format() {
        Pixel::YUV420P => FrameData::PlanarYuv420(YuvPlanes {
            y_plane: copy_plane_from_av(decoded, 0),
            u_plane: copy_plane_from_av(decoded, 1),
            v_plane: copy_plane_from_av(decoded, 2),
        }),
        Pixel::YUV422P => FrameData::PlanarYuv422(YuvPlanes {
            y_plane: copy_plane_from_av(decoded, 0),
            u_plane: copy_plane_from_av(decoded, 1),
            v_plane: copy_plane_from_av(decoded, 2),
        }),
        Pixel::YUV444P => FrameData::PlanarYuv444(YuvPlanes {
            y_plane: copy_plane_from_av(decoded, 0),
            u_plane: copy_plane_from_av(decoded, 1),
            v_plane: copy_plane_from_av(decoded, 2),
        }),
        Pixel::YUVJ420P => FrameData::PlanarYuvJ420(YuvPlanes {
            y_plane: copy_plane_from_av(decoded, 0),
            u_plane: copy_plane_from_av(decoded, 1),
            v_plane: copy_plane_from_av(decoded, 2),
        }),
        Pixel::UYVY422 => FrameData::InterleavedYuv422(copy_plane_from_av(decoded, 0)),
        fmt => return Err(DecoderFrameConversionError::UnsupportedPixelFormat(fmt)),
    };
    Ok(Frame {
        data,
        resolution: Resolution {
            width: decoded.width().try_into().unwrap(),
            height: decoded.height().try_into().unwrap(),
        },
        pts,
    })
}

fn copy_plane_from_av(decoded: &Video, plane: usize) -> bytes::Bytes {
    let mut output_buffer = bytes::BytesMut::with_capacity(
        decoded.plane_width(plane) as usize * decoded.plane_height(plane) as usize,
    );

    decoded
        .data(plane)
        .chunks(decoded.stride(plane))
        .map(|chunk| &chunk[..decoded.plane_width(plane) as usize])
        .for_each(|chunk| output_buffer.extend_from_slice(chunk));

    output_buffer.freeze()
}
