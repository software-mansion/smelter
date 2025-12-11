use std::time::Duration;

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

    /// Duration of stream that should be buffered before stream is started.
    /// If you have both audio and video streams then make sure to use the same value
    /// to avoid desync.
    ///
    /// This value defines minimal latency on the queue, but if you set it to low and fail
    /// to deliver the input stream on time it can cause either black screen or flickering image.
    ///
    /// By default DEFAULT_BUFFER_DURATION will be used.
    pub buffer_duration: Option<Duration>,
}

#[derive(Debug, Clone)]
pub struct InputAudioSamples {
    pub samples: AudioSamples,
    pub start_pts: Duration,
    pub sample_rate: u32,
}

impl InputAudioSamples {
    pub fn new(samples: AudioSamples, start_pts: Duration, sample_rate: u32) -> Self {
        Self {
            samples,
            start_pts,
            sample_rate,
        }
    }

    pub fn pts_range(&self) -> (Duration, Duration) {
        (self.start_pts, self.end_pts())
    }

    pub fn end_pts(&self) -> Duration {
        self.start_pts
            + Duration::from_secs_f64(self.samples.len() as f64 / self.sample_rate as f64)
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.len() == 0
    }
}
