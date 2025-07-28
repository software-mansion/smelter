use std::time::Duration;

use crate::common_pipeline::prelude as pipeline;
use crate::*;

impl TryFrom<HlsInput> for pipeline::RegisterInputOptions {
    type Error = TypeError;

    fn try_from(value: HlsInput) -> Result<Self, Self::Error> {
        let HlsInput {
            url,
            required,
            offset_ms,
        } = value;

        let queue_options = compositor_pipeline::QueueInputOptions {
            required: required.unwrap_or(false),
            offset: offset_ms.map(|offset_ms| Duration::from_secs_f64(offset_ms / 1000.0)),
            buffer_duration: None,
        };

        let input_options = pipeline::HlsInputOptions {
            url,
            video_decoder: pipeline::VideoDecoderOptions::FfmpegH264,
        };

        Ok(pipeline::RegisterInputOptions {
            input_options: pipeline::ProtocolInputOptions::Hls(input_options),
            queue_options,
        })
    }
}
