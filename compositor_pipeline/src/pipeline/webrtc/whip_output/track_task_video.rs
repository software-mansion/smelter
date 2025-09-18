use std::{marker::PhantomData, sync::Arc};

use crossbeam_channel::Sender;
use smelter_render::{error::ErrorStack, Frame};
use tokio::sync::mpsc;
use tracing::warn;

use crate::prelude::*;
use crate::{
    pipeline::{
        encoder::{VideoEncoder, VideoEncoderConfig, VideoEncoderStream},
        rtp::{
            payloader::{PayloaderOptions, PayloaderStream},
            RtpPacket,
        },
    },
    thread_utils::{InitializableThread, ThreadMetadata},
};

#[derive(Debug)]
pub(crate) struct WhipVideoTrackThreadHandle {
    pub frame_sender: Sender<PipelineEvent<Frame>>,
    pub keyframe_request_sender: Sender<()>,
    pub config: VideoEncoderConfig,
}

pub(super) struct WhipVideoTrackThreadOptions<Encoder: VideoEncoder> {
    pub ctx: Arc<PipelineCtx>,
    pub encoder_options: Encoder::Options,
    pub payloader_options: PayloaderOptions,
    pub chunks_sender: mpsc::Sender<RtpPacket>,
}

pub(super) struct WhipVideoTrackThread<Encoder: VideoEncoder> {
    stream: Box<dyn Iterator<Item = RtpPacket>>,
    chunks_sender: mpsc::Sender<RtpPacket>,
    _encoder: PhantomData<Encoder>,
}

impl<Encoder> InitializableThread for WhipVideoTrackThread<Encoder>
where
    Encoder: VideoEncoder + 'static,
{
    type InitOptions = WhipVideoTrackThreadOptions<Encoder>;

    type SpawnOutput = WhipVideoTrackThreadHandle;
    type SpawnError = EncoderInitError;

    fn init(options: Self::InitOptions) -> Result<(Self, Self::SpawnOutput), Self::SpawnError> {
        let WhipVideoTrackThreadOptions {
            ctx,
            encoder_options,
            payloader_options,
            chunks_sender,
        } = options;

        let (frame_sender, frame_receiver) = crossbeam_channel::bounded(5);
        let (encoded_stream, encoder_ctx) = VideoEncoderStream::<Encoder, _>::new(
            ctx,
            encoder_options,
            frame_receiver.into_iter(),
        )?;

        let payloaded_stream = PayloaderStream::new(payloader_options, encoded_stream.flatten());

        let stream = payloaded_stream.flatten().filter_map(|event| match event {
            Ok(PipelineEvent::Data(packet)) => Some(packet),
            Ok(PipelineEvent::EOS) => None,
            Err(err) => {
                warn!(
                    "Depayloading error: {}",
                    ErrorStack::new(&err).into_string()
                );
                None
            }
        });

        let state = Self {
            stream: Box::new(stream),
            chunks_sender,
            _encoder: PhantomData,
        };
        let output = WhipVideoTrackThreadHandle {
            frame_sender,
            keyframe_request_sender: encoder_ctx.keyframe_request_sender,
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
            thread_name: "Whip Video Encoder".to_string(),
            thread_instance_name: "Output".to_string(),
        }
    }
}
