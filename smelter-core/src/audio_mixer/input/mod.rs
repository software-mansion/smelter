use std::time::Duration;

use crossbeam_channel::{Receiver, Sender, bounded};
use tracing::{error, trace};

use crate::{
    audio_mixer::input::input_thread::start_input_thread,
    stats::{AudioMixerInputStatsEvent, InputStatsEvent, StatsEvent, StatsSender},
};

use crate::prelude::*;

mod input_thread;
mod resampler;

#[derive(Debug)]
pub(super) struct AudioMixerInput {
    input_sender: Sender<AudioMixerInputEvent>,
    result_receiver: Receiver<AudioMixerInputResult>,
    next_chunk: Option<AudioMixerInputResult>,
}

#[derive(Debug)]
struct AudioMixerInputEvent {
    batches: Vec<InputAudioSamples>,
    pts_range: (Timestamp, Timestamp),
}

#[derive(Debug)]
struct AudioMixerInputResult {
    samples: Vec<(f64, f64)>,
    pts_range: (Timestamp, Timestamp),
}

/// Reports the audio mixer processing of an input to the stats module.
#[derive(Debug, Clone)]
struct AudioMixerInputStatsSender {
    input_ref: Ref<InputId>,
    stats_sender: StatsSender,
}

impl AudioMixerInputStatsSender {
    fn send(&self, event: AudioMixerInputStatsEvent) {
        self.stats_sender.send(StatsEvent::Input {
            input_ref: self.input_ref.clone(),
            event: InputStatsEvent::AudioMixer(event),
        });
    }
}

impl AudioMixerInput {
    pub fn new(
        mixing_sample_rate: u32,
        input_ref: Ref<InputId>,
        stats_sender: StatsSender,
    ) -> Self {
        let (input_sender, input_receiver) = bounded(100);
        let (result_sender, result_receiver) = bounded(100);
        let stats_sender = AudioMixerInputStatsSender {
            input_ref,
            stats_sender,
        };
        start_input_thread(
            mixing_sample_rate,
            stats_sender,
            input_receiver,
            result_sender,
        );
        Self {
            input_sender,
            result_receiver,
            next_chunk: None,
        }
    }

    pub fn process_batch(
        &self,
        batches: Vec<InputAudioSamples>,
        pts_range: (Timestamp, Timestamp),
    ) {
        let result = self
            .input_sender
            .send(AudioMixerInputEvent { batches, pts_range });
        if result.is_err() {
            trace!("Failed to send samples. Channel closed.")
        }
    }

    pub fn get_samples(&mut self, pts_range: (Timestamp, Timestamp)) -> Option<Vec<(f64, f64)>> {
        loop {
            if self.next_chunk.is_none() {
                let Ok(result) = self
                    .result_receiver
                    .recv_timeout(Duration::from_millis(100))
                else {
                    // Timeout here is just in case of deadlock, it should not happen
                    error!("Failed to read samples.");
                    return None;
                };
                self.next_chunk = Some(result)
            }
            let next = self.next_chunk.as_ref()?;
            if next.pts_range == pts_range {
                return Some(self.next_chunk.take()?.samples);
            }
            error!("Found batch for different range. This should not happen");
            if next.pts_range.0 > pts_range.0 || next.pts_range.1 > pts_range.1 {
                // this should not happen, it would mean we missed some batch that was not pulled
                // in previous iteration
                return None;
            }
            // drop the old batch and wait for next
            self.next_chunk = None
        }
    }
}
