use std::time::Duration;

use crossbeam_channel::{Receiver, Sender, bounded};
use tracing::trace;

use crate::prelude::*;

mod input_thread;

#[derive(Debug)]
pub(super) struct AudioMixerInput {
    input_sender: Sender<AudioMixerInputEvent>,
    result_receiver: Receiver<AudioMixerInputResult>,
    next: Option<AudioMixerInputResult>,
}

#[derive(Debug)]
enum AudioMixerInputEvent {
    Samples(InputAudioSamples),
    RangeRequest((Duration, Duration)),
}

#[derive(Debug)]
struct AudioMixerInputResult {
    samples: AudioSamples,
    pts_range: (Duration, Duration),
}

const SHIFT_THRESHOLD: Duration = Duration::from_millis(5);
const STRETCH_THRESHOLD: Duration = Duration::from_millis(100);

impl AudioMixerInput {
    pub fn new(mixing_sample_rate: u32) -> Self {
        let (input_sender, input_receiver) = bounded(100);
        let (result_sender, result_receiver) = bounded(100);
        Self {
            input_sender,
            result_receiver,
            next: None,
        }
    }

    pub fn write_batch(&self, samples: InputAudioSamples) {
        let result = self
            .input_sender
            .send(AudioMixerInputEvent::Samples(samples));
        if result.is_err() {
            trace!("Failed to send samples. Channel closed.")
        }
    }

    pub fn request_samples(&self, pts_range: (Duration, Duration)) {
        let result = self
            .input_sender
            .send(AudioMixerInputEvent::RangeRequest(pts_range));
        if result.is_err() {
            trace!("Failed to send range request. Channel closed.")
        }
    }

    pub fn get_samples(&mut self, pts_range: (Duration, Duration)) -> Option<AudioSamples> {
        loop {
            if self.next.is_none() {
                let Ok(result) = self.result_receiver.recv() else {
                    trace!("Failed to read samples. Channel closed.");
                    return None;
                };
                self.next = Some(result)
            }
            let next = self.next?;

        }
    }
}
