use std::marker::PhantomData;
use std::sync::Arc;

use crossbeam_channel::Sender;
use smelter_render::Frame;
use tracing::warn;
use webrtc::rtcp;

use crate::prelude::*;
use crate::{
    pipeline::{
        encoder::{VideoEncoder, VideoEncoderConfig, VideoEncoderStream},
        rtp::payloader::{PayloaderOptions, PayloaderStream},
    },
    thread_utils::{InitializableThread, ThreadMetadata},
};

use super::RtpOutputEvent;

pub(crate) struct RtpVideoTrackThreadHandle {
    pub frame_sender: Sender<PipelineEvent<Frame>>,
    pub keyframe_request_sender: Sender<()>,
    pub config: VideoEncoderConfig,
}

pub(super) struct RtpVideoTrackThreadOptions<Encoder: VideoEncoder> {
    pub ctx: Arc<PipelineCtx>,
    pub encoder_options: Encoder::Options,
    pub payloader_options: PayloaderOptions,
    pub chunks_sender: Sender<RtpOutputEvent>,
}

pub(super) struct RtpVideoTrackThread<Encoder: VideoEncoder> {
    stream: Box<dyn Iterator<Item = RtpOutputEvent>>,
    chunks_sender: Sender<RtpOutputEvent>,
    _encoder: PhantomData<Encoder>,
}

impl<Encoder> InitializableThread for RtpVideoTrackThread<Encoder>
where
    Encoder: VideoEncoder + 'static,
{
    type InitOptions = RtpVideoTrackThreadOptions<Encoder>;

    type SpawnOutput = RtpVideoTrackThreadHandle;
    type SpawnError = EncoderInitError;

    fn init(options: Self::InitOptions) -> Result<(Self, Self::SpawnOutput), Self::SpawnError> {
        let RtpVideoTrackThreadOptions {
            ctx,
            encoder_options,
            payloader_options,
            chunks_sender,
        } = options;

        let ssrc = payloader_options.ssrc;
        let (frame_sender, frame_receiver) = crossbeam_channel::bounded(5);

        let (encoded_stream, encoder_ctx) = VideoEncoderStream::<Encoder, _>::new(
            ctx,
            encoder_options,
            frame_receiver.into_iter(),
        )?;

        let payloaded_stream = PayloaderStream::new(payloader_options, encoded_stream.flatten());

        let stream = payloaded_stream.flatten().map(move |event| match event {
            Ok(PipelineEvent::Data(packet)) => RtpOutputEvent::Data(packet),
            Ok(PipelineEvent::EOS) => RtpOutputEvent::VideoEos(rtcp::goodbye::Goodbye {
                sources: vec![ssrc],
                reason: bytes::Bytes::from("Unregister output stream"),
            }),
            Err(err) => RtpOutputEvent::Err(err),
        });

        let state = Self {
            stream: Box::new(stream),
            chunks_sender,
            _encoder: PhantomData,
        };
        let output = RtpVideoTrackThreadHandle {
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
            thread_name: format!("Rtp Video Encoder ({})", Encoder::LABEL),
            thread_instance_name: "Output".to_string(),
        }
    }
}
