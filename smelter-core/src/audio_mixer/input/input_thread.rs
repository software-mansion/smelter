use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};
use tracing::trace;

use crate::audio_mixer::input::{AudioMixerInputEvent, AudioMixerInputResult, SHIFT_THRESHOLD, STRETCH_THRESHOLD};

use crate::prelude::*;

pub(super) fn start_input_thread(
    mixing_sample_rate: u32,
    input_receiver: Receiver<AudioMixerInputEvent>,
    result_sender: Sender<AudioSamples>,
) {
    std::thread::Builder::new()
        .name("audio mixer input".to_string())
        .spawn(move || {
            let processor = InputProcessor::new(mixing_sample_rate);

            for event in input_receiver {
                match event {
                    AudioMixerInputEvent::Samples(samples) => {
                        processor.write_batch(samples);
                    }
                    AudioMixerInputEvent::RangeRequest(pts_range) => {
                        let samples = processor.get_samples(pts_range);
                        let batch = AudioMixerInputResult { samples, pts_range };
                        if result_sender.send(batch).is_err() {
                            trace!("Closing audio mixer input processing thread. Channel closed.");
                            return;
                        }
                    }
                }
            }
        })
        .unwrap()
}

#[derive(Debug)]
struct InputProcessor {
    resampler: rubato::Async<f64>,
    mixing_sample_rate: u32,
    last_batch_received_end: Option<Duration>,
    last_batch_produced_end: Option<Duration>,

    // if below values change, full reset
    last_input_sample_rate: u32,
    last_channel_count: usize,
}

impl InputProcessor {
    pub fn new(mixing_sample_rate: u32) -> Self {
        Self {
            mixing_sample_rate,
            last_batch_received_end: None,
            last_batch_produced_end: None,
        }
    }

    pub fn write_batch(&mut self, batch: InputAudioSamples) {
        // samples and start time
        // - if sample start close to last end add to resmaple
        // - if a lot smaller drop input batch
        // - if a lot larger fill with zeros and append input batch to resampler
        // - in loop:
        //   - try to resmaple content of the buffer
        //   - write output to channel
        //
        //
        // on request resampled
        // - input buffer at this point has data and time it represents
        //   - we have time of the last sample and we can also calculate time of the first sample
        //   - squash or stretch

        if batch.sample_rate != self.last_input_sample_rate {
            // reset state drop everything
            // the same if channel layout changes
        }

        let (start_pts, end_pts) = batch.pts_range();
        if start_pts > self.last_batch_received_end + STRETCH_THRESHOLD {
            //self.resampler.write(InputAudioSamples{ zeros })
        } if start_pts + STRETCH_THRESHOLD < self.last_batch_received_end {
            // drop 
            return
        } 
        self.resampler.write_batch(batch)
    }

    pub fn get_samples(&mut self, pts_range: (Duration, Duration)) -> AudioSamples {
        let buffer_duration = self.resampler.buffer_duration();
        let buffer_size = self.resampler.buffer_size();
    }

    /// This function expects that timestamp of the batches will be in `pts_range`
    /// or in the future (higher values).
    ///
    /// Batch that would start before the `pts_range` start should have been delivered in
    /// the previous call. However, batch like that is still used if received too late,
    /// but previous sample batch might have already been produced with a gap.
    pub fn next_batch_set(
        &mut self,
        batches: Vec<InputAudioSamples>,
        pts_range: (Duration, Duration),
    ) -> Option<AudioSamples> {
        let last_batch_produced_end = self.last_batch_produced_end.unwrap_or(pts_range.0);

        let (last_batch_received_end, batches) = match self.last_batch_received_end {
            Some(pts) => (pts, batches),
            None => {
                let batches: Vec<_> = batches
                    .into_iter()
                    .skip_while(|batch| batch.start_pts < pts_range.0)
                    .collect();
                match batches.first() {
                    Some(batch) => (batch.start_pts, batches),
                    None => {
                        // no samples to init
                        return None;
                    }
                }
            }
        };

        for batch in batches {
            let last_batch_received_end = self
                .last_batch_received_end
                .unwrap_or(last_batch_received_end);
            self.write_next_batch(batch, last_batch_received_end);
        }

        // resample into output or maybe copy into output
        // self.resample_input_buffer()

        // maybe resample
        // write resamples samples to buffer
        // read samples for that range from the buffer
        None
    }

    fn write_next_batch(&mut self, batch: InputAudioSamples, last_batch_received_end: Duration) {
        if last_batch_received_end + STRETCH_THRESHOLD > batch.start_pts {
            // fill zero reset
        } else if last_batch_received_end + SHIFT_THRESHOLD > batch.start_pts {
            // stretch + shift
        } else if last_batch_received_end.saturating_sub(SHIFT_THRESHOLD) > batch.start_pts {
            // shift
        } else if last_batch_received_end.saturating_sub(STRETCH_THRESHOLD) > batch.start_pts {
            // squeeze + shift
        } else {
            // drop/reset
        }
    }
}
