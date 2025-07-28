use core::fmt;
use std::{sync::Arc, time::Duration};

use crossbeam_channel::Sender;

use crate::prelude::*;

#[derive(Debug)]
pub struct RawDataInputSender {
    pub video: Option<Sender<PipelineEvent<Frame>>>,
    pub audio: Option<Sender<PipelineEvent<InputAudioSamples>>>,
}

#[derive(Debug, Clone)]
pub struct RawDataInputOptions {
    pub video: bool,
    pub audio: bool,
}

#[derive(Clone)]
pub struct InputAudioSamples {
    pub samples: Arc<Vec<(f64, f64)>>,
    pub start_pts: Duration,
    pub end_pts: Duration,
}

impl InputAudioSamples {
    pub fn new(
        samples: Arc<Vec<(f64, f64)>>,
        start_pts: Duration,
        mixing_sample_rate: u32,
    ) -> Self {
        let end_pts =
            start_pts + Duration::from_secs_f64(samples.len() as f64 / mixing_sample_rate as f64);

        Self {
            samples,
            start_pts,
            end_pts,
        }
    }

    pub fn duration(&self) -> Duration {
        self.end_pts.saturating_sub(self.start_pts)
    }

    pub(crate) fn len(&self) -> usize {
        self.samples.len()
    }
}

impl fmt::Debug for InputAudioSamples {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InputSamples")
            .field("samples", &format!("len={}", self.samples.len()))
            .field("start_pts", &self.start_pts)
            .field("end_pts", &self.end_pts)
            .finish()
    }
}
