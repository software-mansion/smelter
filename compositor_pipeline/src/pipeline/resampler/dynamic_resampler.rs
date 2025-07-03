use std::time::Duration;

use crate::{
    audio_mixer::AudioSamples,
    pipeline::resampler::single_channel::{ChannelResampler, SingleChannelBatch},
};

#[derive(Debug)]
pub(crate) struct DynamicResamplerBatch {
    pub samples: AudioSamples,
    pub start_pts: Duration,
    pub sample_rate: u32,
}

struct StereoState {
    left: Box<ChannelResampler>,
    right: Box<ChannelResampler>,
    input_sample_rate: u32,
}

struct MonoState {
    resampler: Box<ChannelResampler>,
    input_sample_rate: u32,
}

enum State {
    Stereo(StereoState),
    Mono(MonoState),
}

pub(crate) struct DynamicResampler {
    state: Option<State>,
    first_batch_pts: Option<Duration>,
    output_sample_rate: u32,
}

impl DynamicResampler {
    pub fn new(output_sample_rate: u32) -> Self {
        Self {
            state: None,
            first_batch_pts: None,
            output_sample_rate,
        }
    }

    fn ensure_mono_resampler(
        &mut self,
        batch: &DynamicResamplerBatch,
    ) -> Result<&mut MonoState, rubato::ResamplerConstructionError> {
        let first_batch_pts = *self.first_batch_pts.get_or_insert(batch.start_pts);

        match &self.state {
            Some(State::Mono(state)) if state.input_sample_rate == batch.sample_rate => (),
            _ => {
                self.state = Some(State::Mono(MonoState {
                    input_sample_rate: batch.sample_rate,
                    resampler: ChannelResampler::new(
                        batch.sample_rate,
                        self.output_sample_rate,
                        first_batch_pts,
                    )?,
                }));
            }
        }
        let Some(State::Mono(state)) = &mut self.state else {
            panic!("Invalid state")
        };
        Ok(state)
    }

    fn ensure_stereo_resampler(
        &mut self,
        batch: &DynamicResamplerBatch,
    ) -> Result<&mut StereoState, rubato::ResamplerConstructionError> {
        let first_batch_pts = *self.first_batch_pts.get_or_insert(batch.start_pts);

        match &self.state {
            Some(State::Stereo(state)) if state.input_sample_rate == batch.sample_rate => (),
            _ => {
                self.state = Some(State::Stereo(StereoState {
                    input_sample_rate: batch.sample_rate,
                    left: ChannelResampler::new(
                        batch.sample_rate,
                        self.output_sample_rate,
                        first_batch_pts,
                    )?,
                    right: ChannelResampler::new(
                        batch.sample_rate,
                        self.output_sample_rate,
                        first_batch_pts,
                    )?,
                }));
            }
        }
        let Some(State::Stereo(state)) = &mut self.state else {
            panic!("Invalid state")
        };
        Ok(state)
    }

    pub fn resample(
        &mut self,
        batch: DynamicResamplerBatch,
    ) -> Result<Vec<DynamicResamplerBatch>, rubato::ResamplerConstructionError> {
        if batch.sample_rate == self.output_sample_rate {
            self.state = None;
            Ok(vec![batch])
        } else {
            match &batch.samples {
                AudioSamples::Mono(samples) => {
                    let state = self.ensure_mono_resampler(&batch)?;
                    let result = state
                        .resampler
                        .resample(SingleChannelBatch {
                            start_pts: batch.start_pts,
                            samples: samples.to_vec(),
                        })
                        .into_iter()
                        .map(|resampled| DynamicResamplerBatch {
                            samples: AudioSamples::Mono(resampled.samples),
                            start_pts: resampled.start_pts,
                            sample_rate: self.output_sample_rate,
                        })
                        .collect();

                    Ok(result)
                }
                AudioSamples::Stereo(samples) => {
                    let state = self.ensure_stereo_resampler(&batch)?;
                    let left = state
                        .left
                        .resample(SingleChannelBatch {
                            start_pts: batch.start_pts,
                            samples: samples.iter().map(|(l, _)| *l).collect(),
                        })
                        .into_iter();
                    let right = state
                        .right
                        .resample(SingleChannelBatch {
                            start_pts: batch.start_pts,
                            samples: samples.iter().map(|(_, r)| *r).collect(),
                        })
                        .into_iter();
                    Ok(left
                        .zip(right)
                        .map(|(l, r)| {
                            let joined_samples = l.samples.into_iter().zip(r.samples);
                            DynamicResamplerBatch {
                                samples: AudioSamples::Stereo(joined_samples.collect()),
                                start_pts: l.start_pts,
                                sample_rate: batch.sample_rate,
                            }
                        })
                        .collect())
                }
            }
        }
    }
}
