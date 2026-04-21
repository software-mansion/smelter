use bytes::Bytes;
use smelter_render::Frame;

use crate::{prelude::InputAudioSamples, types::AudioSamples};

// Binary format for audio batches:
//   u64 start_pts_nanos
//   u32 sample_rate
//   u8  channel_count (1 = mono, 2 = stereo)
//   u32 sample_count (number of samples per channel)
//   [f64; sample_count * channel_count] interleaved samples
//     mono: [s0, s1, s2, ...]
//     stereo: [l0, r0, l1, r1, ...]

// Binary format for video frames:
//   u32 width
//   u32 height
//   u64 pts_nanos
//   u8  format (see FrameFormat constants below)
//   u8  plane_count
//   For each plane:
//     u32 plane_len
//     [u8; plane_len] plane_data

mod frame_format {
    pub const PLANAR_YUV_420: u8 = 0;
    pub const PLANAR_YUV_422: u8 = 1;
    pub const PLANAR_YUV_444: u8 = 2;
    pub const PLANAR_YUVJ_420: u8 = 3;
    pub const INTERLEAVED_UYVY_422: u8 = 4;
    pub const INTERLEAVED_YUYV_422: u8 = 5;
    pub const NV12: u8 = 6;
    pub const BGRA: u8 = 7;
    pub const ARGB: u8 = 8;
}

fn write_plane(buf: &mut Vec<u8>, plane: &[u8]) {
    buf.extend_from_slice(&(plane.len() as u32).to_be_bytes());
    buf.extend_from_slice(plane);
}

pub(super) fn serialize_frame(frame: &Frame) -> Option<Bytes> {
    use smelter_render::FrameData;

    let (format, planes): (u8, Vec<&[u8]>) = match &frame.data {
        FrameData::PlanarYuv420(p) => (
            frame_format::PLANAR_YUV_420,
            vec![&p.y_plane, &p.u_plane, &p.v_plane],
        ),
        FrameData::PlanarYuv422(p) => (
            frame_format::PLANAR_YUV_422,
            vec![&p.y_plane, &p.u_plane, &p.v_plane],
        ),
        FrameData::PlanarYuv444(p) => (
            frame_format::PLANAR_YUV_444,
            vec![&p.y_plane, &p.u_plane, &p.v_plane],
        ),
        FrameData::PlanarYuvJ420(p) => (
            frame_format::PLANAR_YUVJ_420,
            vec![&p.y_plane, &p.u_plane, &p.v_plane],
        ),
        FrameData::InterleavedUyvy422(d) => (frame_format::INTERLEAVED_UYVY_422, vec![d]),
        FrameData::InterleavedYuyv422(d) => (frame_format::INTERLEAVED_YUYV_422, vec![d]),
        FrameData::Nv12(p) => (frame_format::NV12, vec![&p.y_plane, &p.uv_planes]),
        FrameData::Bgra(d) => (frame_format::BGRA, vec![d]),
        FrameData::Argb(d) => (frame_format::ARGB, vec![d]),
        FrameData::Rgba8UnormWgpuTexture(_) | FrameData::Nv12WgpuTexture(_) => return None,
    };

    let data_size: usize = planes
        .iter()
        .map(|p| 4 + p.len()) // u32 len + data per plane
        .sum();
    // header: u32 + u32 + u64 + u8 + u8 = 18 bytes
    let mut buf = Vec::with_capacity(18 + data_size);

    buf.extend_from_slice(&(frame.resolution.width as u32).to_be_bytes());
    buf.extend_from_slice(&(frame.resolution.height as u32).to_be_bytes());
    buf.extend_from_slice(&(frame.pts.as_nanos() as u64).to_be_bytes());
    buf.push(format);
    buf.push(planes.len() as u8);
    for plane in &planes {
        write_plane(&mut buf, plane);
    }

    Some(Bytes::from(buf))
}

pub(super) fn serialize_audio_batch(batch: &InputAudioSamples) -> Bytes {
    let (channel_count, sample_count) = match &batch.samples {
        AudioSamples::Mono(s) => (1u8, s.len()),
        AudioSamples::Stereo(s) => (2u8, s.len()),
    };

    // header: u64 + u32 + u8 + u32 = 17 bytes
    let total_f64_count = sample_count * channel_count as usize;
    let mut buf = Vec::with_capacity(17 + total_f64_count * 8);

    buf.extend_from_slice(&(batch.start_pts.as_nanos() as u64).to_be_bytes());
    buf.extend_from_slice(&batch.sample_rate.to_be_bytes());
    buf.push(channel_count);
    buf.extend_from_slice(&(sample_count as u32).to_be_bytes());

    match &batch.samples {
        AudioSamples::Mono(samples) => {
            for &s in samples {
                buf.extend_from_slice(&s.to_be_bytes());
            }
        }
        AudioSamples::Stereo(samples) => {
            for &(l, r) in samples {
                buf.extend_from_slice(&l.to_be_bytes());
                buf.extend_from_slice(&r.to_be_bytes());
            }
        }
    }

    Bytes::from(buf)
}
