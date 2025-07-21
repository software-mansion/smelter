use std::sync::Arc;

use compositor_render::InputId;
use crossbeam_channel::bounded;

use crate::{
    error::InputInitError,
    pipeline::{decoder::DecodedDataReceiver, input::Input, types::RawDataSender, PipelineCtx},
};

#[derive(Debug, Clone)]
pub struct RawDataInputOptions {
    pub video: bool,
    pub audio: bool,
}

pub struct RawDataInput;

impl RawDataInput {
    pub fn new_input(
        _ctx: Arc<PipelineCtx>,
        _input_id: InputId,
        options: RawDataInputOptions,
    ) -> Result<(Input, RawDataSender, DecodedDataReceiver), InputInitError> {
        let (video_sender, video_receiver) = match options.video {
            true => {
                let (sender, receiver) = bounded(1000);
                (Some(sender), Some(receiver))
            }
            false => (None, None),
        };
        let (audio_sender, audio_receiver) = match options.audio {
            true => {
                let (sender, receiver) = bounded(1000);
                (Some(sender), Some(receiver))
            }
            false => (None, None),
        };
        Ok((
            Input::RawDataInput,
            RawDataSender {
                video: video_sender,
                audio: audio_sender,
            },
            DecodedDataReceiver {
                video: video_receiver,
                audio: audio_receiver,
            },
        ))
    }
}
