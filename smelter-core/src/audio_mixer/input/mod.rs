use std::time::Duration;

use crossbeam_channel::{Receiver, Sender, bounded};
use tracing::{error, trace};

use crate::{audio_mixer::input::input_thread::start_input_thread, prelude::*};

mod input_thread;
mod resampler;

#[derive(Debug)]
pub(super) struct AudioMixerInput {
    input_sender: Sender<AudioMixerInputEvent>,
    result_receiver: Receiver<AudioMixerInputResult>,
    next: Option<AudioMixerInputResult>,
}

#[derive(Debug)]
struct AudioMixerInputEvent {
    batches: Vec<InputAudioSamples>,
    pts_range: (Duration, Duration),
}

#[derive(Debug)]
struct AudioMixerInputResult {
    samples: Vec<(f64, f64)>,
    pts_range: (Duration, Duration),
}

impl AudioMixerInput {
    pub fn new(mixing_sample_rate: u32) -> Self {
        let (input_sender, input_receiver) = bounded(100);
        let (result_sender, result_receiver) = bounded(100);
        start_input_thread(mixing_sample_rate, input_receiver, result_sender);
        Self {
            input_sender,
            result_receiver,
            next: None,
        }
    }

    pub fn process_batch(&self, batches: Vec<InputAudioSamples>, pts_range: (Duration, Duration)) {
        let result = self
            .input_sender
            .send(AudioMixerInputEvent { batches, pts_range });
        if result.is_err() {
            trace!("Failed to send samples. Channel closed.")
        }
    }

    pub fn get_samples(&mut self, pts_range: (Duration, Duration)) -> Option<Vec<(f64, f64)>> {
        loop {
            if self.next.is_none() {
                let Ok(result) = self
                    .result_receiver
                    .recv_timeout(Duration::from_millis(100))
                else {
                    // Timeout here is just in case of deadlock, it should not happen
                    error!("Failed to read samples.");
                    return None;
                };
                self.next = Some(result)
            }
            let next = self.next.as_ref()?;
            if next.pts_range == pts_range {
                return Some(self.next.take()?.samples);
            }
            error!("Found batch for different range. This should not happen");
            if next.pts_range.0 > pts_range.0 || next.pts_range.1 > pts_range.1 {
                // this should not happen, it would mean we missed some batch that was not pulled
                // in previous iteration
                return None;
            }
            // drop the old batch and wait for next
            self.next = None
        }
    }
}
