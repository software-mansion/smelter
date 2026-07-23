use std::{marker::PhantomData, sync::Arc};

use smelter_render::Frame;
use tokio::sync::mpsc;
use tracing::warn;

use crate::{
    pipeline::encoder::{VideoEncoder, VideoEncoderConfig, VideoEncoderStream},
    prelude::*,
    utils::{InitializableThread, ThreadMetadata},
};

pub(super) struct VideoEncoderThreadHandle {
    pub frame_sender: crossbeam_channel::Sender<PipelineEvent<Frame>>,
    pub config: VideoEncoderConfig,
}

pub(super) struct VideoEncoderThreadOptions<Encoder: VideoEncoder> {
    pub ctx: Arc<PipelineCtx>,
    pub encoder_options: Encoder::Options,
    pub chunks_sender: mpsc::Sender<EncodedOutputEvent>,
}

pub(super) struct VideoEncoderThread<Encoder: VideoEncoder> {
    stream: Box<dyn Iterator<Item = EncodedOutputEvent>>,
    chunks_sender: mpsc::Sender<EncodedOutputEvent>,
    _encoder: PhantomData<Encoder>,
}

impl<Encoder> InitializableThread for VideoEncoderThread<Encoder>
where
    Encoder: VideoEncoder + 'static,
{
    type InitOptions = VideoEncoderThreadOptions<Encoder>;

    type SpawnOutput = VideoEncoderThreadHandle;
    type SpawnError = EncoderInitError;

    fn init(options: Self::InitOptions) -> Result<(Self, Self::SpawnOutput), Self::SpawnError> {
        let VideoEncoderThreadOptions {
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
            PipelineEvent::Data(chunk) => EncodedOutputEvent::Data(chunk),
            PipelineEvent::EOS => EncodedOutputEvent::VideoEOS,
        });

        let state = Self {
            stream: Box::new(stream),
            chunks_sender,
            _encoder: PhantomData,
        };
        let output = VideoEncoderThreadHandle {
            frame_sender,
            config: encoder_ctx.config,
        };
        Ok((state, output))
    }

    fn run(self) {
        for event in self.stream {
            if self.chunks_sender.blocking_send(event).is_err() {
                warn!("Failed to send encoded video chunk from encoder. Channel closed.");
                return;
            }
        }
    }

    fn metadata() -> ThreadMetadata {
        ThreadMetadata {
            thread_name: format!("Video Encoder ({})", Encoder::LABEL),
            thread_instance_name: "Output".to_string(),
        }
    }
}

impl VideoEncoderThreadHandle {
    pub fn encoder_context(&self) -> Option<bytes::Bytes> {
        self.config.extradata.clone()
    }
}
