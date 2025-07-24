use std::sync::Arc;

use compositor_render::{Frame, OutputFrameFormat, OutputId, Resolution};
use crossbeam_channel::Sender;
use rtmp::RtmpClientOutput;

use crate::pipeline::{hls::HlsOutput, mp4::Mp4Output, rtp::RtpOutput, webrtc::WhipOutput};

use crate::prelude::*;

pub mod encoded_data;
pub mod raw_data;
pub mod rtmp;

#[derive(Debug, Clone, Copy)]
pub(crate) struct OutputVideo<'a> {
    pub resolution: Resolution,
    pub frame_format: OutputFrameFormat,
    pub frame_sender: &'a Sender<PipelineEvent<Frame>>,
    pub keyframe_request_sender: &'a Sender<()>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct OutputAudio<'a> {
    pub samples_batch_sender: &'a Sender<PipelineEvent<OutputAudioSamples>>,
}

pub(crate) trait Output: Send {
    fn audio(&self) -> Option<OutputAudio>;
    fn video(&self) -> Option<OutputVideo>;
    fn kind(&self) -> OutputProtocolKind;
}

pub(super) fn new_external_output(
    ctx: Arc<PipelineCtx>,
    output_id: OutputId,
    options: ProtocolOutputOptions,
) -> Result<(Box<dyn Output>, Option<Port>), OutputInitError> {
    match options {
        ProtocolOutputOptions::Rtp(opt) => {
            let (output, port) = RtpOutput::new(ctx, output_id, opt)?;
            Ok((Box::new(output), Some(port)))
        }
        ProtocolOutputOptions::Rtmp(opt) => {
            let output = RtmpClientOutput::new(ctx, output_id, opt)?;
            Ok((Box::new(output), None))
        }
        ProtocolOutputOptions::Mp4(opt) => {
            let output = Mp4Output::new(ctx, output_id, opt)?;
            Ok((Box::new(output), None))
        }
        ProtocolOutputOptions::Hls(opt) => {
            let output = HlsOutput::new(ctx, output_id, opt)?;
            Ok((Box::new(output), None))
        }
        ProtocolOutputOptions::Whip(opt) => {
            let output = WhipOutput::new(ctx, output_id, opt)?;
            Ok((Box::new(output), None))
        }
    }
}
