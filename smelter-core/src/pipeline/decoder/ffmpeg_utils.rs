use std::time::Duration;

use ffmpeg_next::format::Pixel;
use smelter_render::{Frame, FrameData, Resolution, YuvPlanes};
use tracing::error;

use crate::prelude::*;

#[derive(Debug, thiserror::Error)]
pub(super) enum DecoderFrameConversionError {
    #[error("Error converting frame: {0}")]
    FrameConversionError(String),
    #[error("Unsupported pixel format: {0:?}")]
    UnsupportedPixelFormat(ffmpeg_next::format::pixel::Pixel),
}

pub(super) fn from_av_frame(
    decoded: &mut ffmpeg_next::frame::Video,
    time_base: i32,
) -> Result<Frame, DecoderFrameConversionError> {
    let Some(pts) = decoded.pts() else {
        return Err(DecoderFrameConversionError::FrameConversionError(
            "missing pts".to_owned(),
        ));
    };
    if pts < 0 {
        error!(
            pts,
            "Received negative PTS. PTS values of the decoder output are not monotonically increasing."
        )
    }
    let pts = Duration::from_secs_f64(f64::max(pts as f64 / time_base as f64, 0.0));

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

fn copy_plane_from_av(decoded: &ffmpeg_next::frame::Video, plane: usize) -> bytes::Bytes {
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

#[derive(Debug, thiserror::Error)]
#[error("Cannot send a chunk of kind {0:?} to {1:?} decoder.")]
pub(super) struct DecoderChunkConversionError(MediaKind, VideoCodec);

pub(super) fn create_av_packet(
    chunk: EncodedInputChunk,
    codec: VideoCodec,
    time_base: i32,
) -> Result<ffmpeg_next::Packet, DecoderChunkConversionError> {
    if chunk.kind != MediaKind::Video(codec) {
        return Err(DecoderChunkConversionError(chunk.kind, codec));
    }

    let mut packet = ffmpeg_next::Packet::new(chunk.data.len());

    let dts = chunk.dts;
    let pts = chunk.pts;

    packet.data_mut().unwrap().copy_from_slice(&chunk.data);
    packet.set_pts(Some((pts.as_secs_f64() * time_base as f64) as i64));
    packet.set_dts(dts.map(|dts| (dts.as_secs_f64() * time_base as f64) as i64));

    Ok(packet)
}
