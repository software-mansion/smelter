use std::time::Duration;

use bytes::Bytes;
use smelter_render::Resolution;

use crate::{prelude::InputAudioSamples, types::AudioSamples};

// Binary format for audio batches:
//   u64 start_pts_nanos
//   u32 sample_rate
//   u8  channel_count (1 = mono, 2 = stereo)
//   u32 sample_count (number of samples per channel)
//   [f64; sample_count * channel_count] interleaved samples
//     mono: [s0, s1, s2, ...]
//     stereo: [l0, r0, l1, r1, ...]

// Binary format for video frames (always RGBA):
//   u32 width
//   u32 height
//   u64 pts_nanos
//   [u8; width * height * 4] rgba_data

pub(super) fn serialize_rgba_frame(
    resolution: Resolution,
    pts: Duration,
    rgba_data: Bytes,
) -> Bytes {
    // header: u32 + u32 + u64 = 16 bytes
    let mut buf = Vec::with_capacity(16 + rgba_data.len());

    buf.extend_from_slice(&(resolution.width as u32).to_be_bytes());
    buf.extend_from_slice(&(resolution.height as u32).to_be_bytes());
    buf.extend_from_slice(&(pts.as_nanos() as u64).to_be_bytes());
    buf.extend_from_slice(&rgba_data);

    Bytes::from(buf)
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
