use core::slice;
use ffmpeg_next::{format::Pixel, frame, Dictionary};
use std::{collections::HashMap, time::Duration};

use smelter_render::FrameData;

use crate::prelude::*;

#[derive(Debug, thiserror::Error)]
#[error("Failed to create libav frame: {0}")]
pub(super) struct FrameConversionError(String);

#[derive(Debug, thiserror::Error)]
pub(super) enum ChunkFromFfmpegError {
    #[error("No data")]
    NoData,
    #[error("No pts")]
    NoPts,
}

pub(super) fn create_av_frame(
    frame: Frame,
    time_base: i32,
) -> Result<frame::Video, FrameConversionError> {
    let (data, pixel_format) = match frame.data {
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

    let mut av_frame = frame::Video::new(
        pixel_format,
        frame.resolution.width as u32,
        frame.resolution.height as u32,
    );

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

    av_frame.set_pts(Some((frame.pts.as_secs_f64() * time_base as f64) as i64));

    write_plane_to_av_frame(&mut av_frame, 0, &data.y_plane);
    write_plane_to_av_frame(&mut av_frame, 1, &data.u_plane);
    write_plane_to_av_frame(&mut av_frame, 2, &data.v_plane);

    Ok(av_frame)
}

fn write_plane_to_av_frame(frame: &mut frame::Video, plane: usize, data: &[u8]) {
    let stride = frame.stride(plane);
    let width = frame.plane_width(plane) as usize;

    data.chunks(width)
        .zip(frame.data_mut(plane).chunks_mut(stride))
        .for_each(|(data, target)| target[..width].copy_from_slice(data));
}

#[derive(Debug, Default)]
pub(super) struct FfmpegOptions(HashMap<String, String>);

impl FfmpegOptions {
    pub fn append<T: AsRef<str>>(&mut self, options: &[(T, T)]) {
        for (key, value) in options {
            self.0
                .insert(key.as_ref().to_string(), value.as_ref().to_string());
        }
    }

    pub fn into_dictionary(self) -> Dictionary<'static> {
        Dictionary::from_iter(self.0)
    }
}

impl<T: AsRef<str>, const N: usize> From<&[(T, T); N]> for FfmpegOptions {
    fn from(value: &[(T, T); N]) -> Self {
        let mut options = FfmpegOptions::default();
        options.append(value);
        options
    }
}

pub(super) fn read_extradata(encoder: &ffmpeg_next::codec::encoder::Video) -> Option<bytes::Bytes> {
    unsafe {
        let encoder_ptr = encoder.0 .0 .0.as_ptr();
        let size = (*encoder_ptr).extradata_size;
        if size > 0 {
            let extradata_slice = slice::from_raw_parts((*encoder_ptr).extradata, size as usize);
            Some(bytes::Bytes::copy_from_slice(extradata_slice))
        } else {
            None
        }
    }
}

pub(super) fn encoded_chunk_from_av_packet(
    packet: &ffmpeg_next::Packet,
    kind: MediaKind,
    time_base: i32,
) -> Result<EncodedOutputChunk, ChunkFromFfmpegError> {
    let data = match packet.data() {
        Some(data) => bytes::Bytes::copy_from_slice(data),
        None => return Err(ChunkFromFfmpegError::NoData),
    };

    let rescale = |v: i64| Duration::from_secs_f64((v as f64) * (1.0 / time_base as f64));

    let Some(pts) = packet.pts().map(rescale) else {
        return Err(ChunkFromFfmpegError::NoPts);
    };
    let dts = packet.dts().map(rescale);

    Ok(EncodedOutputChunk {
        data,
        pts,
        dts,
        is_keyframe: packet.flags().contains(ffmpeg_next::packet::Flags::KEY),
        kind,
    })
}
