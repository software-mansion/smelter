use compositor_render::Frame;
use crossbeam_channel::Sender;

use crate::*;

#[derive(Debug, Clone)]
pub struct RegisterRawDataInputOptions {
    pub video: Option<Sender<PipelineEvent<Frame>>>,
    pub audio: Option<Sender<PipelineEvent<InputSamples>>>,
}
