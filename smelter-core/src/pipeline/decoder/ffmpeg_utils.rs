use ffmpeg_next::format::Pixel;
use smelter_render::{FrameData, Resolution, YuvPlanes};

use crate::prelude::*;

/// Packet and frame timestamps are in nanoseconds, so they map exactly to [`Timestamp`].
pub(super) const TIME_BASE: i32 = 1_000_000_000;

#[derive(Debug, thiserror::Error)]
pub(super) enum DecoderFrameConversionError {
    #[error("Error converting frame: {0}")]
    FrameConversionError(String),
    #[error("Unsupported pixel format: {0:?}")]
    UnsupportedPixelFormat(ffmpeg_next::format::pixel::Pixel),
}

pub(super) fn from_av_frame(
    decoded: &mut ffmpeg_next::frame::Video,
) -> Result<Frame, DecoderFrameConversionError> {
    let Some(pts) = decoded.pts() else {
        return Err(DecoderFrameConversionError::FrameConversionError(
            "missing pts".to_owned(),
        ));
    };
    let pts = Timestamp::from_nanos(pts);

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
) -> Result<ffmpeg_next::Packet, DecoderChunkConversionError> {
    if chunk.kind != MediaKind::Video(codec) {
        return Err(DecoderChunkConversionError(chunk.kind, codec));
    }

    let mut packet = ffmpeg_next::Packet::new(chunk.data.len());

    packet.data_mut().unwrap().copy_from_slice(&chunk.data);
    packet.set_pts(Some(chunk.pts.as_nanos()));
    packet.set_dts(chunk.dts.map(Timestamp::as_nanos));

    Ok(packet)
}
