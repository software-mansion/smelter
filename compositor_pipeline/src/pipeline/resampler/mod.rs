use std::time::Duration;

pub(super) mod decoder_resampler;
pub(super) mod dynamic_resampler;
pub(super) mod encoder_resampler;
mod single_channel;

const SAMPLE_BATCH_DURATION: Duration = Duration::from_millis(20);
