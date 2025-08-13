use tracing::warn;

use crate::pipeline::rtp::payloader::{PayloadedCodec, Payloader, PayloaderOptions};
use crate::prelude::*;

pub(crate) fn init_payloaders(
    video_encoder: Option<VideoEncoderOptions>,
    audio_encoder: Option<AudioEncoderOptions>,
    video_ssrc: Option<u32>,
    audio_ssrc: Option<u32>,
) -> (Option<Payloader>, Option<Payloader>) {
    let video_payloader = if let (Some(ssrc), Some(encoder)) = (video_ssrc, video_encoder) {
        let (codec, payload_type, clock_rate) = match encoder {
            VideoEncoderOptions::FfmpegH264(_) => (PayloadedCodec::H264, 102, 90000),
            VideoEncoderOptions::FfmpegVp8(_) => (PayloadedCodec::Vp8, 96, 90000),
            VideoEncoderOptions::FfmpegVp9(_) => (PayloadedCodec::Vp9, 98, 90000),
        };

        Some(Payloader::new(PayloaderOptions {
            codec,
            payload_type,
            clock_rate,
            mtu: 1200,
            ssrc,
        }))
    } else {
        None
    };

    let audio_payloader = if let (Some(ssrc), Some(encoder)) = (audio_ssrc, audio_encoder) {
        let (codec, payload_type, clock_rate) = match encoder {
            AudioEncoderOptions::Opus(_) => (PayloadedCodec::Opus, 111, 48000),
            AudioEncoderOptions::FdkAac(_) => {
                warn!("AAC codec not supported for WHEP output");
                return (video_payloader, None);
            }
        };

        Some(Payloader::new(PayloaderOptions {
            codec,
            payload_type,
            clock_rate,
            mtu: 1200,
            ssrc,
        }))
    } else {
        None
    };

    (video_payloader, audio_payloader)
}
