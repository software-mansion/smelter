use std::time::Duration;

use crate::prelude::*;

#[derive(Debug)]
pub(super) struct AudioMixerInput {
    mixing_sample_rate: u32,
    last_batch_received_end: Option<Duration>,
    input_buffer:
}

const SHIFT_THRESHOLD: Duration = Duration::from_millis(5);
const STRETCH_THRESHOLD: Duration = Duration::from_millis(100);

impl AudioMixerInput {
    pub fn new(mixing_sample_rate: u32) -> Self {
        Self {
            mixing_sample_rate,
            last_batch_received_end: None,
        }
    }

    pub fn next_batch_set(
        &mut self,
        batches: Vec<InputAudioSamples>,
        pts_range: (Duration, Duration),
    ) -> AudioSamples {
        let (last_batch_received_end, batches) = match self.last_batch_received_end {
            Some(pts) => (pts, batches),
            None => {
                let batches: Vec<_> = batches
                    .into_iter()
                    .skip_while(|batch| batch.start_pts > pts_range.0)
                    .collect();
                match batches.first() {
                    Some(batch) => (batch.start_pts, batches),
                    None => {
                        // no samples to init
                        return;
                    }
                }
            }
        };

        for batch in batches {
            let last_batch_received_end = self
                .last_batch_received_end
                .unwrap_or(last_batch_received_end);
            self.next_batch(batch, last_batch_received_end);
        }

        // resample into output or maybe copy into output
        // self.resample_input_buffer()

        // maybe resample
        // write resamples samples to buffer
        // read samples for that range from the buffer
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
