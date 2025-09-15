use crate::pipeline::rtp::payloader::{PayloadedCodec, Payloader, PayloaderOptions};
use crate::prelude::*;

pub(crate) fn init_video_payloader(encoder: &VideoEncoderOptions, ssrc: u32) -> Payloader {
    let (codec, payload_type, clock_rate) = match encoder {
        VideoEncoderOptions::FfmpegH264(_) | VideoEncoderOptions::VulkanH264(_) => {
            (PayloadedCodec::H264, 102, 90000)
        }
        VideoEncoderOptions::FfmpegVp8(_) => (PayloadedCodec::Vp8, 96, 90000),
        VideoEncoderOptions::FfmpegVp9(_) => (PayloadedCodec::Vp9, 98, 90000),
    };

    Payloader::new(PayloaderOptions {
        codec,
        payload_type,
        clock_rate,
        mtu: 1200,
        ssrc,
    })
}

pub(crate) fn init_audio_payloader(ssrc: u32, payload_type: u8) -> Payloader {
    Payloader::new(PayloaderOptions {
        codec: PayloadedCodec::Opus,
        payload_type,
        clock_rate: 48000,
        mtu: 1200,
        ssrc,
    })
}
