use tracing::warn;

use crate::pipeline::rtp::payloader::{PayloadedCodec, Payloader, PayloaderOptions};
use crate::prelude::*;

pub(crate) fn init_video_payloader(encoder: VideoEncoderOptions, ssrc: u32) -> Option<Payloader> {
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
}

pub(crate) fn init_audio_payloader(encoder: AudioEncoderOptions, ssrc: u32) -> Option<Payloader> {
    let (codec, payload_type, clock_rate) = match encoder {
        AudioEncoderOptions::Opus(_) => (PayloadedCodec::Opus, 111, 48000),
        AudioEncoderOptions::FdkAac(_) => {
            warn!("AAC codec not supported for WHEP output");
            return None;
        }
    };

    Some(Payloader::new(PayloaderOptions {
        codec,
        payload_type,
        clock_rate,
        mtu: 1200,
        ssrc,
    }))
}
