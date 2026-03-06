use std::{marker::PhantomData, sync::Arc};

use crossbeam_channel::Sender;
use smelter_render::Frame;
use tracing::warn;

use crate::{
    prelude::*,
    utils::{InitializableThread, ThreadMetadata},
};

use super::{VideoEncoder, VideoEncoderConfig, VideoEncoderStream};

pub(crate) struct VideoEncoderThreadHandle {
    pub frame_sender: Sender<PipelineEvent<Frame>>,
    pub keyframe_request_sender: Sender<()>,
    pub config: VideoEncoderConfig,
}

pub(crate) struct VideoEncoderThreadOptions<Encoder: VideoEncoder> {
    pub ctx: Arc<PipelineCtx>,
    pub encoder_options: Encoder::Options,
    pub chunks_sender: Sender<EncodedOutputEvent>,
    pub output_ref: Ref<OutputId>,
    pub chunk_size_event: Option<fn(u64, &Ref<OutputId>) -> StatsEvent>,
}

pub(crate) struct VideoEncoderThread<Encoder: VideoEncoder> {
    stream: Box<dyn Iterator<Item = EncodedOutputEvent>>,
    chunks_sender: Sender<EncodedOutputEvent>,
    output_ref: Ref<OutputId>,
    stats_sender: StatsSender,
    chunk_size_event: Option<fn(u64, &Ref<OutputId>) -> StatsEvent>,
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
            output_ref,
            chunk_size_event,
        } = options;
        let stats_sender = ctx.stats_sender.clone();

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
            output_ref,
            stats_sender,
            chunk_size_event,
            _encoder: PhantomData,
        };
        let output = VideoEncoderThreadHandle {
            frame_sender,
            keyframe_request_sender: encoder_ctx.keyframe_request_sender,
            config: encoder_ctx.config,
        };
        Ok((state, output))
    }

    fn run(self) {
        for event in self.stream {
            if let Some(make_event) = self.chunk_size_event {
                self.stats_sender
                    .send(vec![make_event(event.data_size(), &self.output_ref)]);
            }
            if self.chunks_sender.send(event).is_err() {
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
