use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};
use tracing::trace;

use crate::audio_mixer::input::{
    AudioMixerInputEvent, AudioMixerInputResult, resampler::InputResampler,
};

use crate::prelude::*;

pub(super) fn start_input_thread(
    mixing_sample_rate: u32,
    input_receiver: Receiver<AudioMixerInputEvent>,
    result_sender: Sender<AudioMixerInputResult>,
) {
    std::thread::Builder::new()
        .name("audio mixer input".to_string())
        .spawn(move || {
            let mut processor = InputProcessor::new(mixing_sample_rate);

            for event in input_receiver {
                // Separation to write_batch and get_samples exists here, because
                // we might need to move this logic to queue and writing batch to buffer
                // and reading resampled values would be a separate steps
                for batch in event.batches {
                    processor.write_batch(batch);
                }

                let pts_range = event.pts_range;
                let samples = processor.get_samples(pts_range);
                let result = AudioMixerInputResult { samples, pts_range };
                if result_sender.send(result).is_err() {
                    trace!("Closing audio mixer input processing thread. Channel closed.");
                    return;
                }
            }
        })
        .unwrap();
}

struct InputProcessor {
    mixing_sample_rate: u32,
    resampler: Option<InputResampler>,
}

impl InputProcessor {
    pub fn new(mixing_sample_rate: u32) -> Self {
        Self {
            mixing_sample_rate,
            resampler: None,
        }
    }

    pub fn write_batch(&mut self, batch: InputAudioSamples) {
        let channels = match batch.samples {
            AudioSamples::Mono(_) => AudioChannels::Mono,
            AudioSamples::Stereo(_) => AudioChannels::Stereo,
        };
        let input_sample_rate = batch.sample_rate;

        let resampler = self.resampler.get_or_insert_with(|| {
            InputResampler::new(
                input_sample_rate,
                self.mixing_sample_rate,
                channels,
                batch.start_pts,
            )
            .unwrap()
        });
        if resampler.channels() != channels || resampler.input_sample_rate() != input_sample_rate {
            *resampler = InputResampler::new(
                input_sample_rate,
                self.mixing_sample_rate,
                channels,
                batch.start_pts,
            )
            .unwrap();
        }
        resampler.write_batch(batch);
    }

    pub fn get_samples(&mut self, pts_range: (Duration, Duration)) -> Vec<(f64, f64)> {
        match &mut self.resampler {
            Some(resampler) => match resampler.get_samples(pts_range) {
                AudioSamples::Mono(items) => {
                    items.into_iter().map(|sample| (sample, sample)).collect()
                }
                AudioSamples::Stereo(items) => items,
            },
            None => {
                let sample_count = f64::floor(
                    (pts_range.1 - pts_range.0).as_secs_f64() * self.mixing_sample_rate as f64,
                ) as usize;
                vec![(0.0, 0.0); sample_count]
            }
        }
    }
}
