use compositor_render::Resolution;
use crossbeam_channel::Receiver;

use crate::PipelineEvent;

/// Options to configure output that sends raw PCM audio + wgpu textures via channel
#[derive(Debug, Clone)]
pub struct RegisterRawDataOutputOptions {
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

/// Receiver sides of video/audio channels for data produced by
/// audio mixer and renderer
#[derive(Debug, Clone)]
pub struct RawDataReceiver {
    pub video: Option<Receiver<PipelineEvent<Frame>>>,
    pub audio: Option<Receiver<PipelineEvent<OutputSamples>>>,
}
