use std::time::Duration;

use crossbeam_channel::Sender;

use crate::prelude::*;

/// Senders are connected directly to the queue. Dropping a sender signals end of stream.
#[derive(Debug)]
pub struct RawDataInputSender {
    pub video: Option<Sender<Frame>>,
    pub audio: Option<Sender<InputAudioSamples>>,
}

#[derive(Debug, Clone)]
pub struct RawDataInputOptions {
    pub video: bool,
    pub audio: bool,
    pub required: bool,
    /// Defines how PTS of delivered frames/samples maps onto the queue timeline. PTS values
    /// are passed to the queue unchanged.
    pub offset: QueueTrackOffset,
}

#[derive(Debug, Clone)]
pub struct InputAudioSamples {
    pub samples: AudioSamples,
    pub start_pts: Timestamp,
    pub sample_rate: u32,
}

impl InputAudioSamples {
    pub fn new(samples: AudioSamples, start_pts: Timestamp, sample_rate: u32) -> Self {
        Self {
            samples,
            start_pts,
            sample_rate,
        }
    }

    pub fn pts_range(&self) -> (Timestamp, Timestamp) {
        (self.start_pts, self.end_pts())
    }

    pub fn end_pts(&self) -> Timestamp {
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
