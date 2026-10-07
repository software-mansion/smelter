use std::{collections::VecDeque, f64::consts::PI, time::Duration};

use tracing::error;

use crate::{AudioChannels, AudioSamples};

/// Length of real input kept in `ConcealmentHistory`, used to fit the LPC model.
const HISTORY_DURATION: Duration = Duration::from_millis(20);

const LPC_ORDER: usize = 16;

/// Generated signal fades to silence over this duration.
const DECAY_DURATION: Duration = Duration::from_millis(10);

/// Last `HISTORY_DURATION` of real input samples. Never contains padding or generated samples,
/// so it has to be cleared on every discontinuity.
pub(super) struct ConcealmentHistory {
    samples: HistorySamples,
    capacity: usize,
    sample_rate: u32,
}

enum HistorySamples {
    Mono(VecDeque<f64>),
    Stereo(VecDeque<(f64, f64)>),
}

impl ConcealmentHistory {
    pub fn new(channels: AudioChannels, sample_rate: u32) -> Self {
        let capacity = (HISTORY_DURATION.as_secs_f64() * sample_rate as f64).round() as usize;
        let samples = match channels {
            AudioChannels::Mono => HistorySamples::Mono(VecDeque::with_capacity(capacity)),
            AudioChannels::Stereo => HistorySamples::Stereo(VecDeque::with_capacity(capacity)),
        };
        Self {
            samples,
            capacity,
            sample_rate,
        }
    }

    pub fn push(&mut self, samples: &AudioSamples) {
        match (&mut self.samples, samples) {
            (HistorySamples::Mono(history), AudioSamples::Mono(samples)) => {
                let skip = samples.len().saturating_sub(self.capacity);
                history.extend(&samples[skip..]);
                let excess = history.len().saturating_sub(self.capacity);
                history.drain(..excess);
            }
            (HistorySamples::Stereo(history), AudioSamples::Stereo(samples)) => {
                let skip = samples.len().saturating_sub(self.capacity);
                history.extend(&samples[skip..]);
                let excess = history.len().saturating_sub(self.capacity);
                history.drain(..excess);
            }
            _ => error!("Wrong channel layout"),
        }
    }

    pub fn clear(&mut self) {
        match &mut self.samples {
            HistorySamples::Mono(history) => history.clear(),
            HistorySamples::Stereo(history) => history.clear(),
        }
    }
}

/// Continues the signal from `ConcealmentHistory` by running an LPC synthesis filter with zero
/// excitation, faded out to silence over `DECAY_DURATION`.
pub(super) struct Concealment {
    predictors: Predictors,
    /// Frames generated so far, position in the decay window.
    generated: usize,
    decay_len: usize,
}

enum Predictors {
    Mono(LpcPredictor),
    Stereo(LpcPredictor, LpcPredictor),
}

impl Concealment {
    /// Returns `None` if history is too short to fit the model.
    pub fn new(history: &ConcealmentHistory) -> Option<Self> {
        let predictors = match &history.samples {
            HistorySamples::Mono(samples) => {
                Predictors::Mono(LpcPredictor::fit(samples.iter().copied().collect())?)
            }
            HistorySamples::Stereo(samples) => Predictors::Stereo(
                LpcPredictor::fit(samples.iter().map(|(l, _)| *l).collect())?,
                LpcPredictor::fit(samples.iter().map(|(_, r)| *r).collect())?,
            ),
        };
        let decay_len =
            (DECAY_DURATION.as_secs_f64() * history.sample_rate as f64).round() as usize;
        Some(Self {
            predictors,
            generated: 0,
            decay_len,
        })
    }

    /// Generate next `frames` frames. Returns zeros once the decay is finished.
    pub fn next(&mut self, frames: usize) -> AudioSamples {
        let start = self.generated;
        let decay_len = self.decay_len;
        self.generated += frames;
        let gain = |i: usize| decay_gain(start + i, decay_len);
        match &mut self.predictors {
            Predictors::Mono(predictor) => {
                AudioSamples::Mono((0..frames).map(|i| predictor.next() * gain(i)).collect())
            }
            Predictors::Stereo(left, right) => AudioSamples::Stereo(
                (0..frames)
                    .map(|i| (left.next() * gain(i), right.next() * gain(i)))
                    .collect(),
            ),
        }
    }

    pub fn is_exhausted(&self) -> bool {
        self.generated >= self.decay_len
    }
}

/// Half of a raised cosine, from 1.0 at frame 0 to 0.0 at `decay_len`.
fn decay_gain(frame: usize, decay_len: usize) -> f64 {
    if frame >= decay_len {
        return 0.0;
    }
    0.5 * (1.0 + (PI * frame as f64 / decay_len as f64).cos())
}

struct LpcPredictor {
    /// `coefficients[k]` multiplies the sample `k + 1` frames back.
    coefficients: Vec<f64>,
    /// Last `LPC_ORDER` samples, newest at the back.
    memory: VecDeque<f64>,
}

impl LpcPredictor {
    fn fit(samples: Vec<f64>) -> Option<Self> {
        if samples.len() <= LPC_ORDER {
            return None;
        }
        let len = samples.len();
        let windowed: Vec<f64> = samples
            .iter()
            .enumerate()
            .map(|(i, sample)| {
                sample * 0.5 * (1.0 - (2.0 * PI * i as f64 / (len - 1) as f64).cos())
            })
            .collect();
        let autocorrelation: Vec<f64> = (0..=LPC_ORDER)
            .map(|lag| {
                windowed[lag..]
                    .iter()
                    .zip(&windowed)
                    .map(|(a, b)| a * b)
                    .sum()
            })
            .collect();
        let coefficients = levinson_durbin(&autocorrelation)[1..]
            .iter()
            .map(|a| -a)
            .collect();
        let memory = samples[len - LPC_ORDER..].iter().copied().collect();
        Some(Self {
            coefficients,
            memory,
        })
    }

    fn next(&mut self) -> f64 {
        let sample = self
            .coefficients
            .iter()
            .zip(self.memory.iter().rev())
            .map(|(c, s)| c * s)
            .sum();
        self.memory.pop_front();
        self.memory.push_back(sample);
        sample
    }
}

/// Prediction error filter `[1, a_1, .., a_p]` for the given autocorrelation `[r_0, .., r_p]`.
fn levinson_durbin(autocorrelation: &[f64]) -> Vec<f64> {
    let order = autocorrelation.len() - 1;
    let mut a = vec![0.0; order + 1];
    a[0] = 1.0;
    // White noise correction (-40dB) keeps the recursion well conditioned.
    let mut error = autocorrelation[0] * (1.0 + 1e-4);
    if error <= 0.0 {
        return a;
    }
    for i in 1..=order {
        let acc: f64 =
            autocorrelation[i] + (1..i).map(|j| a[j] * autocorrelation[i - j]).sum::<f64>();
        let k = -acc / error;
        if k.abs() >= 1.0 {
            break;
        }
        for j in 1..=(i - 1) / 2 {
            let (a_j, a_ij) = (a[j], a[i - j]);
            a[j] = a_j + k * a_ij;
            a[i - j] = a_ij + k * a_j;
        }
        if i % 2 == 0 {
            a[i / 2] *= 1.0 + k;
        }
        a[i] = k;
        error *= 1.0 - k * k;
    }
    a
}
