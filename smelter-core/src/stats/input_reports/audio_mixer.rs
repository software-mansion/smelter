use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Stats report for the audio mixer processing of an input.
///
/// The mixer places input audio on the output timeline based on its timestamps, but plays it
/// back sample by sample. When timestamps diverge from the amount of received audio, the mixer
/// corrects the difference (drift): small drift by adjusting the playback speed, larger by
/// inserting silence or dropping audio.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct AudioMixerInputStatsReport {
    /// State of the input playback.
    pub state: AudioMixerInputResamplerState,

    /// Format of the input audio. `None` before any audio reaches the mixer.
    pub input_format: Option<AudioMixerInputFormat>,

    /// Total count of gaps or overlaps (10ms or longer) between timestamps of consecutive input
    /// chunks.
    pub timestamp_discontinuities: u32,

    /// Total count of cases when the mixer ran out of input audio and had to interrupt the
    /// playback until more arrived.
    pub underruns: u32,

    /// Total count of cases when silence was inserted because input timestamps got ahead of the
    /// received audio by more than 40ms.
    pub gaps_inserted: u32,

    /// Total count of cases when audio was dropped because input timestamps fell behind the
    /// received audio by more than 500ms.
    pub drops: u32,

    /// Total count of cases when part of a silent passage was cut out because input timestamps
    /// fell behind the received audio.
    pub silence_cuts: u32,

    /// Total count of cases when the end of input audio was extended with generated audio to
    /// cover a gap.
    pub concealments: u32,

    /// Total count of fade-ins applied when the playback resumed.
    pub fade_ins: u32,

    /// Total count of fade-outs applied when the playback was interrupted.
    pub fade_outs: u32,

    /// Audio mixer stats in the 10-second window.
    pub last_10_seconds: AudioMixerInputSlidingWindowStatsReport,
}

/// State of the input playback in the audio mixer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AudioMixerInputResamplerState {
    /// Input audio is played.
    Playing,
    /// Playback is interrupted until input audio that can be placed on the output timeline
    /// arrives.
    WaitingForInput,
}

/// Format of the input audio.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct AudioMixerInputFormat {
    /// Sample rate of the input audio.
    pub sample_rate: u32,

    /// Channel layout of the input audio.
    pub channels: AudioMixerInputChannels,
}

/// Channel layout of the input audio.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AudioMixerInputChannels {
    Mono,
    Stereo,
}

/// Stats report for the given time window in the audio mixer processing of an input.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct AudioMixerInputSlidingWindowStatsReport {
    /// Count of gaps or overlaps (10ms or longer) between timestamps of consecutive input chunks
    /// during the given time window.
    pub timestamp_discontinuities: u32,

    /// Count of cases when the mixer ran out of input audio and had to interrupt the playback
    /// during the given time window.
    pub underruns: u32,

    /// Count of cases when silence was inserted because input timestamps got ahead of the
    /// received audio by more than 40ms during the given time window.
    pub gaps_inserted: u32,

    /// Count of cases when audio was dropped because input timestamps fell behind the received
    /// audio by more than 500ms during the given time window.
    pub drops: u32,

    /// Count of cases when part of a silent passage was cut out because input timestamps fell
    /// behind the received audio during the given time window.
    pub silence_cuts: u32,

    /// Count of cases when the end of input audio was extended with generated audio during the
    /// given time window.
    pub concealments: u32,

    /// Count of fade-ins applied during the given time window.
    pub fade_ins: u32,

    /// Count of fade-outs applied during the given time window.
    pub fade_outs: u32,

    /// Difference between input timestamps and the position on the output timeline where the
    /// input audio is played. It is the audio/video desynchronization introduced by the input.
    /// Positive when audio is played ahead of its timestamps, negative when behind. Values within
    /// 2ms are not corrected.
    pub drift_avg_seconds: f64,
    /// Difference between input timestamps and the position on the output timeline where the
    /// input audio is played. It is the audio/video desynchronization introduced by the input.
    /// Positive when audio is played ahead of its timestamps, negative when behind. Values within
    /// 2ms are not corrected.
    pub drift_max_seconds: f64,
    /// Difference between input timestamps and the position on the output timeline where the
    /// input audio is played. It is the audio/video desynchronization introduced by the input.
    /// Positive when audio is played ahead of its timestamps, negative when behind. Values within
    /// 2ms are not corrected.
    pub drift_min_seconds: f64,

    /// Duration of input audio buffered in the audio mixer, waiting to be played.
    pub input_buffer_avg_seconds: f64,
    /// Duration of input audio buffered in the audio mixer, waiting to be played.
    pub input_buffer_max_seconds: f64,
    /// Duration of input audio buffered in the audio mixer, waiting to be played.
    pub input_buffer_min_seconds: f64,
}
