use std::time::Duration;

use crate::common_core::prelude as core;
use crate::*;

impl From<OpusEncoderPreset> for core::OpusEncoderPreset {
    fn from(value: OpusEncoderPreset) -> Self {
        match value {
            OpusEncoderPreset::Quality => core::OpusEncoderPreset::Quality,
            OpusEncoderPreset::Voip => core::OpusEncoderPreset::Voip,
            OpusEncoderPreset::LowestLatency => core::OpusEncoderPreset::LowestLatency,
        }
    }
}

impl From<PixelFormat> for core::OutputPixelFormat {
    fn from(value: PixelFormat) -> Self {
        match value {
            PixelFormat::Yuv420p => core::OutputPixelFormat::YUV420P,
            PixelFormat::Yuv422p => core::OutputPixelFormat::YUV422P,
            PixelFormat::Yuv444p => core::OutputPixelFormat::YUV444P,
        }
    }
}

impl TryFrom<VideoEncoderBitrate> for core::VideoEncoderBitrate {
    type Error = TypeError;

    fn try_from(value: VideoEncoderBitrate) -> Result<Self, Self::Error> {
        match value {
            VideoEncoderBitrate::AverageBitrate(average_bitrate) => Ok(core::VideoEncoderBitrate {
                average_bitrate,
                max_bitrate: (average_bitrate as f64 * 1.25) as u64,
            }),
            VideoEncoderBitrate::Vbr {
                average_bitrate,
                max_bitrate,
            } => {
                if average_bitrate > max_bitrate {
                    return Err(TypeError::new(
                        "max_bitrate has to be greater than average_bitrate",
                    ));
                }

                Ok(core::VideoEncoderBitrate {
                    average_bitrate,
                    max_bitrate,
                })
            }
        }
    }
}

pub(crate) fn timestamp_from_start_at(
    start_at_ms: Option<f64>,
) -> Result<Option<core::Timestamp>, TypeError> {
    let start_at = start_at_ms
        .map(duration_from_ms)
        .transpose()
        .map_err(|err| TypeError::new(format!("Invalid start_at_ms. {err}")))?;
    Ok(start_at.map(Into::into))
}

pub(crate) fn duration_from_keyframe_interval(
    keyframe_interval: &Option<f64>,
) -> Result<Duration, TypeError> {
    const DEFAULT_KEYFRAME_INTERVAL: Duration = Duration::from_millis(5000);

    match keyframe_interval {
        Some(ki) => duration_from_ms(*ki)
            .map_err(|err| TypeError::new(format!("Invalid keyframe_interval_ms. {err}"))),
        None => Ok(DEFAULT_KEYFRAME_INTERVAL),
    }
}
