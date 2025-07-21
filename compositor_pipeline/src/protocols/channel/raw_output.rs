use std::time::Duration;

use crossbeam_channel::Receiver;

use crate::prelude::*;

/// Options to configure output that sends raw PCM audio + wgpu textures via channel
#[derive(Debug, Clone)]
pub struct RawDataOutputOptions {
    pub video: Option<RawDataOutputVideoOptions>,
    pub audio: Option<RawDataOutputAudioOptions>,
}

/// Options to configure audio output that returns raw video via channel.
///
/// TODO: add option, for now it implies RGBA wgpu::Texture
#[derive(Debug, Clone)]
pub struct RawDataOutputVideoOptions {
    pub resolution: Resolution,
}

/// Options to configure audio output that returns raw audio via channel.
///
/// TODO: add option, for now it implies 16-bit stereo
#[derive(Debug, Clone)]
pub struct RawDataOutputAudioOptions;

/// channel receivers that return PCM audio and wgpu textures
#[derive(Debug, Clone)]
pub struct RawDataOutputReceiver {
    pub video: Option<Receiver<PipelineEvent<Frame>>>,
    pub audio: Option<Receiver<PipelineEvent<OutputAudioSamples>>>,
}

#[derive(Debug)]
pub struct OutputAudioSamples {
    pub samples: AudioSamples,
    pub start_pts: Duration,
}
