use std::sync::OnceLock;

use compositor_render::{Frame, OutputFrameFormat, Resolution};
use crossbeam_channel::{bounded, Sender};

use crate::{
    audio_mixer::OutputSamples, error::OutputInitError, pipeline::RawDataReceiver,
    queue::PipelineEvent,
};

use super::{Output, OutputAudio, OutputVideo, RawDataOutputOptions};

pub(crate) struct RawDataOutput {
    video: Option<(Sender<PipelineEvent<Frame>>, Resolution)>,
    audio: Option<Sender<PipelineEvent<OutputSamples>>>,
}

impl RawDataOutput {
    pub fn new(options: RawDataOutputOptions) -> Result<(Self, RawDataReceiver), OutputInitError> {
        let (video, video_receiver) = match &options.video {
            Some(opts) => {
                let (sender, receiver) = bounded(100);
                (Some((sender, opts.resolution)), Some(receiver))
            }
            None => (None, None),
        };
        let (audio, audio_receiver) = match options.audio {
            Some(_) => {
                let (sender, receiver) = bounded(100);
                (Some(sender), Some(receiver))
            }
            None => (None, None),
        };
        Ok((
            Self { video, audio },
            RawDataReceiver {
                video: video_receiver,
                audio: audio_receiver,
            },
        ))
    }
}

impl Output for RawDataOutput {
    fn audio(&self) -> Option<OutputAudio> {
        self.audio.as_ref().map(|audio| OutputAudio {
            samples_batch_sender: audio,
        })
    }

    fn video(&self) -> Option<OutputVideo> {
        // fake closed channel (keyframe request do not make sense for this output)
        static FAKE_SENDER: OnceLock<Sender<()>> = OnceLock::new();
        let keyframe_request_sender = FAKE_SENDER.get_or_init(|| bounded(1).0);

        self.video.as_ref().map(|video| OutputVideo {
            resolution: video.1,
            frame_format: OutputFrameFormat::RgbaWgpuTexture,
            frame_sender: &video.0,
            keyframe_request_sender,
        })
    }
}
