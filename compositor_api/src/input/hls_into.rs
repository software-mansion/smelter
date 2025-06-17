use std::time::Duration;

use compositor_pipeline::{
    pipeline::{self, input},
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

        Ok(pipeline::RegisterInputOptions {
            input_options: input::InputOptions::Hls(input::hls::HlsInputOptions { url }),
            queue_options,
        })
    }
}
