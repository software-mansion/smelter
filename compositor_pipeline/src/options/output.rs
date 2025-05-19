use std::sync::Arc;

use compositor_render::{scene::Component, InputId};

use crate::*;

#[derive(Debug, Clone)]
pub enum RegisterOutputOptions {
    Rtp(RtpOutputOptions),
    Rtmp(RtmpOutputOptions),
    Mp4(Mp4OutputOptions),
    Whip(WhipOutputOptions),
}

#[derive(Debug, Clone)]
pub struct AudioScene {
    pub inputs: Vec<InputParams>,
}

#[derive(Debug, Clone)]
pub struct VideoScene {
    pub root: Component,
}

#[derive(Debug, Clone)]
pub struct InputParams {
    pub input_id: InputId,
    // [0, 1] range of input volume
    pub volume: f32,
}

#[derive(Debug, Clone, Copy)]
pub enum MixingStrategy {
    SumClip,
    SumScale,
}

//#[derive(Debug, Clone)]
//pub struct OutputVideoOptions {}
//
//#[derive(Debug, Clone)]
//pub struct OutputAudioOptions {
//    pub initial: AudioScene,
//    pub mixing_strategy: MixingStrategy,
//    pub channels: AudioChannels,
//    pub end_condition: PipelineOutputEndCondition,
//}

#[derive(Debug, Clone)]
pub enum PipelineOutputEndCondition {
    AnyOf(Vec<InputId>),
    AllOf(Vec<InputId>),
    AnyInput,
    AllInputs,
    Never,
}
