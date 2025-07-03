use std::time::Duration;

use tracing::info;

use crate::pipeline::resampler::single_channel::ChannelResampler;

pub(super) struct StereoSampleBatch {
    pub samples: (Vec<f64>, Vec<f64>),
    pub start_pts: Duration,
    pub sample_rate: u32,
}

struct State {
    l: ChannelResampler,
    r: ChannelResampler,
    input_sample_rate: u32,
}

pub(super) struct DynamicStereoResampler {
    state: Option<State>,
    first_batch_pts: Option<Duration>,
    output_sample_rate: u32,
}

impl DynamicStereoResampler {
    pub fn new(output_sample_rate: u32) -> Self {
        Self {
            state: None,
            first_batch_pts: None,
            output_sample_rate,
        }
    }

    pub fn resample(
        &mut self,
        batch: StereoSampleBatch,
    ) -> Result<Vec<StereoSampleBatch>, rubato::ResamplerConstructionError> {
        let first_batch_pts = *self.first_batch_pts.get_or_insert(batch.start_pts);

        if batch.sample_rate == self.output_sample_rate {
            self.state = None;
            return Ok(batch);
        } else {
            match self.state {
                Some(state) if state.input_sample_rate == batch.sample_rate => (),
                Some(_) | None => {
                    info!(
                        "Instantiate new resampler (input: {}, output: {})",
                        batch.sample_rate, self.output_sample_rate
                    );
                    let state = State {
                        l: ChannelResampler::new(
                            batch.sample_rate,
                            self.output_sample_rate,
                            self.first_batch_pts,
                        )?,
                        r: ChannelResampler::new(
                            batch.sample_rate,
                            self.output_sample_rate,
                            self.first_batch_pts,
                        )?,
                        input_sample_rate: batch.sample_rate,
                    };
                    self.state = Some(state);
                }
            };
            match self.state {
                Some(state) => {
                    let l = state.l.resample(batch.0).into_iter();
                    let r = state.r.resample(batch.1).into_iter();
                    Ok(l.zip(r)
                        .map(|(l, r)| StereoSampleBatch {
                            samples: (l.samples, r.samples),
                            start_pts: l.start_pts,
                            sample_rate: batch.sample_rate,
                        })
                        .collect())
                }
                None => Ok(vec![]),
            }
        }
    }
}
