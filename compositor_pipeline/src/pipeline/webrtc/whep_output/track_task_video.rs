use std::{marker::PhantomData, sync::Arc};

use crossbeam_channel::Sender;
use smelter_render::Frame;
use tokio::sync::broadcast;
use tracing::warn;

use crate::prelude::*;
use crate::{
    pipeline::encoder::{VideoEncoder, VideoEncoderConfig, VideoEncoderStream},
    thread_utils::{InitializableThread, ThreadMetadata},
};

#[derive(Debug, Clone)]
pub(crate) struct WhepVideoTrackThreadHandle {
    pub frame_sender: Sender<PipelineEvent<Frame>>,
    pub keyframe_request_sender: Sender<()>,
    pub config: VideoEncoderConfig,
}

pub(crate) struct WhepVideoTrackThreadOptions<Encoder: VideoEncoder> {
    pub ctx: Arc<PipelineCtx>,
    pub encoder_options: Encoder::Options,
    pub chunks_sender: broadcast::Sender<EncodedOutputEvent>,
}

pub(crate) struct WhepVideoTrackThread<Encoder: VideoEncoder> {
    stream: Box<dyn Iterator<Item = EncodedOutputEvent>>,
    chunks_sender: broadcast::Sender<EncodedOutputEvent>,
    _encoder: PhantomData<Encoder>,
}

impl<Encoder> InitializableThread for WhepVideoTrackThread<Encoder>
where
    Encoder: VideoEncoder + 'static,
{
    type InitOptions = WhepVideoTrackThreadOptions<Encoder>;

    type SpawnOutput = WhepVideoTrackThreadHandle;
    type SpawnError = EncoderInitError;

    fn init(options: Self::InitOptions) -> Result<(Self, Self::SpawnOutput), Self::SpawnError> {
        let WhepVideoTrackThreadOptions {
            ctx,
            encoder_options,
            chunks_sender,
        } = options;

        let (frame_sender, frame_receiver) = crossbeam_channel::bounded(5);
        let (encoded_stream, encoder_ctx) = VideoEncoderStream::<Encoder, _>::new(
            ctx,
            encoder_options,
            frame_receiver.into_iter(),
        )?;

        let stream = encoded_stream.flatten().map(|event| match event {
            PipelineEvent::Data(packet) => EncodedOutputEvent::Data(packet),
            PipelineEvent::EOS => EncodedOutputEvent::VideoEOS,
        });

        let state = Self {
            stream: Box::new(stream),
            chunks_sender,
            _encoder: PhantomData,
        };
        let output = WhepVideoTrackThreadHandle {
            frame_sender,
            keyframe_request_sender: encoder_ctx.keyframe_request_sender,
            config: encoder_ctx.config,
        };
        Ok((state, output))
    }

    fn run(self) {
        for event in self.stream {
            if self.chunks_sender.send(event).is_err() {
                warn!("Failed to send encoded video chunk from encoder. Channel closed.");
                return;
            }
        }
    }

    fn metadata() -> ThreadMetadata {
        ThreadMetadata {
            thread_name: "Whep Video Encoder".to_string(),
            thread_instance_name: "Output".to_string(),
        }
    }
}
