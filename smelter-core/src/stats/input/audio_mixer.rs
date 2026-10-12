use std::time::Duration;

use crate::{
    AudioChannels, Timestamp,
    stats::{
        input_reports::{
            AudioMixerInputChannels, AudioMixerInputFormat, AudioMixerInputResamplerState,
            AudioMixerInputSlidingWindowStatsReport, AudioMixerInputStatsReport,
        },
        utils::SlidingWindowValue,
    },
};

/// Events sent by the audio mixer while processing an input.
#[derive(Debug, Clone, Copy)]
pub(crate) enum AudioMixerInputStatsEvent {
    /// Processing started for a new input format.
    FormatChanged {
        sample_rate: u32,
        channels: AudioChannels,
    },
    TimestampDiscontinuity,
    /// Input ran out, the rest of it was resampled without waiting for more.
    Underrun,
    /// Input timestamps got too far ahead of the received audio, silence was inserted.
    GapInserted,
    /// Input timestamps fell too far behind the received audio, audio was dropped.
    Drop,
    Concealment,
    FadeIn,
    FadeOut,
    /// Measured before resampling a batch; positive when input timestamps are ahead of the
    /// output.
    Drift(Timestamp),
    /// Sent before producing a chunk of samples.
    StateSnapshot {
        state: AudioMixerInputResamplerState,
        /// Input waiting to be resampled.
        input_buffer: Duration,
    },
}

#[derive(Debug)]
pub struct AudioMixerInputStatsState {
    state: AudioMixerInputResamplerState,
    /// Sample rate and channels; `None` until the mixer receives audio.
    format: Option<(u32, AudioChannels)>,

    timestamp_discontinuities: EventCounter,
    underruns: EventCounter,
    gaps_inserted: EventCounter,
    drops: EventCounter,
    concealments: EventCounter,
    fade_ins: EventCounter,
    fade_outs: EventCounter,

    drift_10_secs: SlidingWindowValue<Timestamp>,
    input_buffer_10_secs: SlidingWindowValue<Duration>,
}

#[derive(Debug)]
struct EventCounter {
    total: u32,
    last_10_secs: SlidingWindowValue<u32>,
}

impl AudioMixerInputStatsState {
    pub fn new() -> Self {
        Self {
            state: AudioMixerInputResamplerState::WaitingForInput,
            format: None,
            timestamp_discontinuities: EventCounter::new(),
            underruns: EventCounter::new(),
            gaps_inserted: EventCounter::new(),
            drops: EventCounter::new(),
            concealments: EventCounter::new(),
            fade_ins: EventCounter::new(),
            fade_outs: EventCounter::new(),
            drift_10_secs: SlidingWindowValue::new(Duration::from_secs(10)),
            input_buffer_10_secs: SlidingWindowValue::new(Duration::from_secs(10)),
        }
    }

    pub fn received_any(&self) -> bool {
        self.format.is_some()
    }

    pub fn handle_event(&mut self, event: AudioMixerInputStatsEvent) {
        match event {
            AudioMixerInputStatsEvent::FormatChanged {
                sample_rate,
                channels,
            } => self.format = Some((sample_rate, channels)),
            AudioMixerInputStatsEvent::TimestampDiscontinuity => {
                self.timestamp_discontinuities.record()
            }
            AudioMixerInputStatsEvent::Underrun => self.underruns.record(),
            AudioMixerInputStatsEvent::GapInserted => self.gaps_inserted.record(),
            AudioMixerInputStatsEvent::Drop => self.drops.record(),
            AudioMixerInputStatsEvent::Concealment => self.concealments.record(),
            AudioMixerInputStatsEvent::FadeIn => self.fade_ins.record(),
            AudioMixerInputStatsEvent::FadeOut => self.fade_outs.record(),
            AudioMixerInputStatsEvent::Drift(drift) => self.drift_10_secs.push(drift),
            AudioMixerInputStatsEvent::StateSnapshot {
                state,
                input_buffer,
            } => {
                self.state = state;
                self.input_buffer_10_secs.push(input_buffer);
            }
        }
    }

    pub fn report(&mut self) -> AudioMixerInputStatsReport {
        AudioMixerInputStatsReport {
            state: self.state,
            input_format: self
                .format
                .map(|(sample_rate, channels)| AudioMixerInputFormat {
                    sample_rate,
                    channels: match channels {
                        AudioChannels::Mono => AudioMixerInputChannels::Mono,
                        AudioChannels::Stereo => AudioMixerInputChannels::Stereo,
                    },
                }),
            timestamp_discontinuities: self.timestamp_discontinuities.total,
            underruns: self.underruns.total,
            gaps_inserted: self.gaps_inserted.total,
            drops: self.drops.total,
            concealments: self.concealments.total,
            fade_ins: self.fade_ins.total,
            fade_outs: self.fade_outs.total,
            last_10_seconds: AudioMixerInputSlidingWindowStatsReport {
                timestamp_discontinuities: self.timestamp_discontinuities.last_10_secs.sum(),
                underruns: self.underruns.last_10_secs.sum(),
                gaps_inserted: self.gaps_inserted.last_10_secs.sum(),
                drops: self.drops.last_10_secs.sum(),
                concealments: self.concealments.last_10_secs.sum(),
                fade_ins: self.fade_ins.last_10_secs.sum(),
                fade_outs: self.fade_outs.last_10_secs.sum(),
                drift_avg_seconds: self.drift_10_secs.avg().as_secs_f64(),
                drift_max_seconds: self.drift_10_secs.max().as_secs_f64(),
                drift_min_seconds: self.drift_10_secs.min().as_secs_f64(),
                input_buffer_avg_seconds: self.input_buffer_10_secs.avg().as_secs_f64(),
                input_buffer_max_seconds: self.input_buffer_10_secs.max().as_secs_f64(),
                input_buffer_min_seconds: self.input_buffer_10_secs.min().as_secs_f64(),
            },
        }
    }
}

impl EventCounter {
    fn new() -> Self {
        Self {
            total: 0,
            last_10_secs: SlidingWindowValue::new(Duration::from_secs(10)),
        }
    }

    fn record(&mut self) {
        self.total += 1;
        self.last_10_secs.push(1);
    }
}
