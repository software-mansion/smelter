use std::time::Duration;

use smelter_render::{Framerate, Resolution};

use crate::codecs::VideoEncoderBitrate;

pub(super) fn bitrate_from_resolution_framerate(
    resolution: Resolution,
    framerate: Framerate,
) -> VideoEncoderBitrate {
    const PRECISION: f64 = 500_000.0; // 500kb
    const BPP: f64 = 0.08;
    let width = u32::max(resolution.width as u32, 1);
    let height = u32::max(resolution.height as u32, 1);

    let average_bitrate =
        (width * height) as f64 * (framerate.num as f64 / framerate.den as f64) * BPP;
    let average_bitrate = (average_bitrate / PRECISION).ceil() * PRECISION;
    let max_bitrate = average_bitrate * 1.25;

    VideoEncoderBitrate {
        average_bitrate: average_bitrate as u64,
        max_bitrate: max_bitrate as u64,
    }
}

pub(super) fn gop_size_from_ms_framerate(keyframe_interval: Duration, framerate: Framerate) -> u64 {
    let framerate = framerate.num as f64 / framerate.den as f64;
    (framerate * keyframe_interval.as_secs_f64()).round() as u64
}
