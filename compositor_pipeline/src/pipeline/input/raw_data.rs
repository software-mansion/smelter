use std::sync::Arc;

use compositor_render::InputId;
use crossbeam_channel::bounded;

use crate::prelude::*;
use crate::{pipeline::input::Input, queue::QueueDataReceiver};

pub struct RawDataInput;

impl RawDataInput {
    pub fn new_input(
        _ctx: Arc<PipelineCtx>,
        _input_id: InputId,
        options: RawDataInputOptions,
    ) -> Result<(Input, RawDataInputSender, QueueDataReceiver), InputInitError> {
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
            Input::RawDataChannel,
            RawDataInputSender {
                video: video_sender,
                audio: audio_sender,
            },
            QueueDataReceiver {
                video: video_receiver,
                audio: audio_receiver,
            },
        ))
    }
}
