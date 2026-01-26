use std::time::Duration;

mod dynamic_resampler;
pub(super) mod encoder_resampler;
mod single_channel;

const SAMPLE_BATCH_DURATION: Duration = Duration::from_millis(20);
