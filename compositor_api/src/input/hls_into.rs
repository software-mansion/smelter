use std::time::Duration;

use compositor_pipeline::{
    pipeline::{
        self, decoder,
        input::{self, hls},
    },
    queue,
};

use crate::*;

impl TryFrom<HlsInput> for pipeline::RegisterInputOptions {
    type Error = TypeError;

    fn try_from(value: HlsInput) -> Result<Self, Self::Error> {
        let HlsInput {
            url,
            required,
            offset_ms,
        } = value;

        let queue_options = queue::QueueInputOptions {
            required: required.unwrap_or(false),
            offset: offset_ms.map(|offset_ms| Duration::from_secs_f64(offset_ms / 1000.0)),
            buffer_duration: None,
        };

        let input_options = hls::HlsInputOptions {
            url,
            video_decoder: decoder::VideoDecoderOptions {
                decoder: pipeline::VideoDecoder::FFmpegH264,
            },
        };

        Ok(pipeline::RegisterInputOptions {
            input_options: input::InputOptions::Hls(input_options),
            queue_options,
        })
    }
}
