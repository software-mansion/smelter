use core::fmt;
use std::time::Duration;

use crate::*;

/// Options to configure output that sends h264 and opus audio via channel
#[derive(Debug, Clone)]
pub struct RegisterEncodedDataOutputOptions {
    pub video: Option<EncodedDataOutputVideoEncoderOptions>,
    pub audio: Option<EncodedDataOutputAudioEncoderOptions>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum EncodedDataOutputVideoEncoderOptions {
    H264(ffmpeg_h264::EncoderOptions),
    VP8(ffmpeg_vp8::EncoderOptions),
    VP9(ffmpeg_vp9::EncoderOptions),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum EncodedDataOutputAudioEncoderOptions {
    Opus(opus::EncoderOptions),
    Aac(fdk_aac::EncoderOptions),
}
