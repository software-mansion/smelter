use std::time::Duration;

use smelter_render::InputId;

use crate::{
    Ref,
    stats::{
        StatsSender,
        input_reports::{AudioMixerSlidingWindowStatsReport, AudioMixerStatsReport},
        state::StatsEvent,
        utils::SlidingWindowValue,
    },
};

/// Stats events emitted by an input's audio-mixer stage (per-input
/// resampling + drift correction). Each input that goes through the audio
/// mixer has exactly one such stage, so these are not split per track.
#[derive(Debug, Clone, Copy)]
pub(crate) enum AudioMixerStatsEvent {
    /// PTS drift between the input buffer head and the mixer-requested PTS,
    /// sampled once per `get_samples` call.
    Drift(f64),
    /// Amount of audio currently sitting in the resampler input buffer.
    BufferDuration(Duration),
    /// Resampler had to reset (gap too large to stretch / partial run).
    DiscontinuityDetected,
}

impl AudioMixerStatsEvent {
    pub fn into_event(self, input_ref: &Ref<InputId>) -> StatsEvent {
        StatsEvent::AudioMixer {
            input_ref: input_ref.clone(),
            event: self,
        }
    }
}

#[derive(Debug)]
pub struct AudioMixerStatsState {
    pub discontinuities_total: u32,

    pub drift_1_sec: SlidingWindowValue<f64>,
    pub drift_10_secs: SlidingWindowValue<f64>,

    pub buffer_duration_1_sec: SlidingWindowValue<Duration>,
    pub buffer_duration_10_secs: SlidingWindowValue<Duration>,

    pub discontinuities_1_sec: SlidingWindowValue<u32>,
    pub discontinuities_10_secs: SlidingWindowValue<u32>,
}

impl AudioMixerStatsState {
    pub fn new() -> Self {
        Self {
            discontinuities_total: 0,
            drift_1_sec: SlidingWindowValue::new(Duration::from_secs(1)),
            drift_10_secs: SlidingWindowValue::new(Duration::from_secs(10)),
            buffer_duration_1_sec: SlidingWindowValue::new(Duration::from_secs(1)),
            buffer_duration_10_secs: SlidingWindowValue::new(Duration::from_secs(10)),
            discontinuities_1_sec: SlidingWindowValue::new(Duration::from_secs(1)),
            discontinuities_10_secs: SlidingWindowValue::new(Duration::from_secs(10)),
        }
    }

    pub fn handle_event(&mut self, event: AudioMixerStatsEvent) {
        match event {
            AudioMixerStatsEvent::Drift(drift) => {
                self.drift_1_sec.push(drift);
                self.drift_10_secs.push(drift);
            }
            AudioMixerStatsEvent::BufferDuration(duration) => {
                self.buffer_duration_1_sec.push(duration);
                self.buffer_duration_10_secs.push(duration);
            }
            AudioMixerStatsEvent::DiscontinuityDetected => {
                self.discontinuities_total += 1;
                self.discontinuities_1_sec.push(1);
                self.discontinuities_10_secs.push(1);
            }
        }
    }

    pub fn report(&mut self) -> AudioMixerStatsReport {
        AudioMixerStatsReport {
            discontinuities_total: self.discontinuities_total,
            last_1_second: AudioMixerSlidingWindowStatsReport {
                drift_avg_seconds: self.drift_1_sec.avg(),
                drift_min_seconds: self.drift_1_sec.min(),
                drift_max_seconds: self.drift_1_sec.max(),
                buffer_duration_avg_seconds: self.buffer_duration_1_sec.avg().as_secs_f64(),
                buffer_duration_min_seconds: self.buffer_duration_1_sec.min().as_secs_f64(),
                buffer_duration_max_seconds: self.buffer_duration_1_sec.max().as_secs_f64(),
                discontinuities_count: self.discontinuities_1_sec.sum(),
            },
            last_10_seconds: AudioMixerSlidingWindowStatsReport {
                drift_avg_seconds: self.drift_10_secs.avg(),
                drift_min_seconds: self.drift_10_secs.min(),
                drift_max_seconds: self.drift_10_secs.max(),
                buffer_duration_avg_seconds: self.buffer_duration_10_secs.avg().as_secs_f64(),
                buffer_duration_min_seconds: self.buffer_duration_10_secs.min().as_secs_f64(),
                buffer_duration_max_seconds: self.buffer_duration_10_secs.max().as_secs_f64(),
                discontinuities_count: self.discontinuities_10_secs.sum(),
            },
        }
    }
}

/// Thin wrapper that bundles the `StatsSender` and `Ref<InputId>` so the
/// audio-mixer stage doesn't have to know about either at the call site.
#[derive(Debug, Clone)]
pub(crate) struct AudioMixerStatsSender {
    stats_sender: StatsSender,
    input_ref: Ref<InputId>,
}

impl AudioMixerStatsSender {
    pub fn new(stats_sender: StatsSender, input_ref: Ref<InputId>) -> Self {
        Self {
            stats_sender,
            input_ref,
        }
    }

    pub fn send_drift(&self, drift: f64) {
        self.send(AudioMixerStatsEvent::Drift(drift));
    }

    pub fn send_buffer_duration(&self, duration: Duration) {
        self.send(AudioMixerStatsEvent::BufferDuration(duration));
    }

    pub fn send_discontinuity(&self) {
        self.send(AudioMixerStatsEvent::DiscontinuityDetected);
    }

    fn send(&self, event: AudioMixerStatsEvent) {
        self.stats_sender.send(event.into_event(&self.input_ref));
    }
}
