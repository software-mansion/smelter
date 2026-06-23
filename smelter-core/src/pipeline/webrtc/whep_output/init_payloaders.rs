use crate::pipeline::rtp::payloader::{PayloadedCodec, Payloader, PayloaderOptions};
use crate::prelude::*;

pub(crate) fn init_video_payloader(
    encoder: &VideoEncoderOptions,
    ssrc: u32,
) -> Payloader {
    let (codec, payload_type, clock_rate) = match encoder.codec() {
        VideoCodec::H264 => (PayloadedCodec::H264, 102, 90000),
        VideoCodec::Vp8 => (PayloadedCodec::Vp8, 96, 90000),
        VideoCodec::Vp9 => (PayloadedCodec::Vp9, 98, 90000),
    };

    Payloader::new(PayloaderOptions { codec, payload_type, clock_rate, mtu: 1200, ssrc })
}

pub(crate) fn init_audio_payloader(ssrc: u32) -> Payloader {
    Payloader::new(PayloaderOptions {
        codec: PayloadedCodec::Opus,
        payload_type: 111,
        clock_rate: 48000,
        mtu: 1200,
        ssrc,
    })
}
