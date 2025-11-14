use std::sync::Arc;

use crate::{
    InputBufferOptions,
    codecs::{AudioEncoderOptions, VideoDecoderOptions, VideoEncoderOptions},
};

#[derive(Debug, Clone)]
pub struct RtmpOutputOptions {
    pub url: Arc<str>,
    pub video: Option<VideoEncoderOptions>,
    pub audio: Option<AudioEncoderOptions>,
}

#[derive(Debug, Clone)]
pub struct RtmpServerInputOptions {
    pub url: Arc<str>,
    pub video_decoders: RtmpServerInputVideoDecoders,
    pub buffer: InputBufferOptions,
}

#[derive(Debug, Clone)]
pub struct RtmpServerInputVideoDecoders {
    pub h264: Option<VideoDecoderOptions>,
}
