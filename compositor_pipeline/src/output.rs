use compositor_render::scene::Component;

use crate::prelude::*;

#[derive(Debug, Clone)]
pub struct RegisterOutputOptions {
    pub output_options: ProtocolOutputOptions,
    pub video: Option<RegisterOutputVideoOptions>,
    pub audio: Option<RegisterOutputAudioOptions>,
}

#[derive(Debug, Clone)]
pub struct RegisterEncodedDataOutputOptions {
    pub output_options: EncodedDataOutputOptions,
    pub video: Option<RegisterOutputVideoOptions>,
    pub audio: Option<RegisterOutputAudioOptions>,
}

#[derive(Debug, Clone)]
pub struct RegisterRawDataOutputOptions {
    pub output_options: RawDataOutputOptions,
    pub video: Option<RegisterOutputVideoOptions>,
    pub audio: Option<RegisterOutputAudioOptions>,
}

#[derive(Debug, Clone)]
pub enum ProtocolOutputOptions {
    Rtp(RtpOutputOptions),
    Rtmp(RtmpClientOutputOptions),
    Mp4(Mp4OutputOptions),
    Hls(HlsOutputOptions),
    Whip(WhipClientOutputOptions),
    Whep(WhepServerOutputOptions),
}

#[derive(Debug, Clone)]
pub struct RegisterOutputVideoOptions {
    pub initial: Component,
    pub end_condition: PipelineOutputEndCondition,
}

#[derive(Debug, Clone)]
pub struct RegisterOutputAudioOptions {
    pub initial: AudioMixerConfig,
    pub mixing_strategy: AudioMixingStrategy,
    pub channels: AudioChannels,
    pub end_condition: PipelineOutputEndCondition,
}

#[derive(Debug, Clone)]
pub struct AudioMixerConfig {
    pub inputs: Vec<AudioMixerInputConfig>,
}

#[derive(Debug, Clone)]
pub struct AudioMixerInputConfig {
    pub input_id: InputId,
    // [0, 1] range of input volume
    pub volume: f32,
}

#[derive(Debug, Clone)]
pub enum AudioMixingStrategy {
    SumClip,
    SumScale,
}

#[derive(Debug, Clone)]
pub enum PipelineOutputEndCondition {
    AnyOf(Vec<InputId>),
    AllOf(Vec<InputId>),
    AnyInput,
    AllInputs,
    Never,
}

#[derive(Debug)]
pub struct OutputInfo {
    pub protocol: OutputProtocolKind,
}

#[derive(Debug, Clone, Copy)]
pub enum OutputProtocolKind {
    Rtp,
    Rtmp,
    Whip,
    Whep,
    Mp4,
    Hls,
    EncodedDataChannel,
    RawDataChannel,
}
