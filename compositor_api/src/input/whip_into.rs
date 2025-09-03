use itertools::Itertools;
use std::time::Duration;

use crate::common_pipeline::prelude as pipeline;
use crate::*;

impl TryFrom<WhipInput> for pipeline::RegisterInputOptions {
    type Error = TypeError;

    fn try_from(value: WhipInput) -> Result<Self, Self::Error> {
        let WhipInput {
            video,
            required,
            offset_ms,
            bearer_token,
            endpoint_override,
        } = value;

        let video_preferences = video
            .and_then(|options| options.decoder_preferences)
            .filter(|v| !v.is_empty())
            .unwrap_or(vec![WhipVideoDecoderOptions::Any])
            .into_iter()
            .map(Into::into)
            .unique()
            .collect();

        let whip_options = pipeline::WhipInputOptions {
            video_preferences,
            bearer_token,
            endpoint_override,
        };

        let input_options = pipeline::ProtocolInputOptions::Whip(whip_options);

        let queue_options = compositor_pipeline::QueueInputOptions {
            required: required.unwrap_or(false),
            offset: offset_ms.map(|offset_ms| Duration::from_secs_f64(offset_ms / 1000.0)),
        };

        Ok(pipeline::RegisterInputOptions {
            input_options,
            queue_options,
        })
    }
}
