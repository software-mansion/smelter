use std::{marker::PhantomData, sync::Arc};

use crossbeam_channel::Sender;
use smelter_render::Frame;
use tracing::warn;

use crate::prelude::*;
use crate::{
    pipeline::{
        decoder::{VideoDecoder, VideoDecoderStream},
        rtp::{
            depayloader::{DepayloaderOptions, DepayloaderStream},
            RtpPacket,
        },
    },
    thread_utils::{InitializableThread, ThreadMetadata},
};

pub(crate) struct RtpVideoTrackThreadHandle {
    pub rtp_packet_sender: Sender<PipelineEvent<RtpPacket>>,
}

pub(super) struct RtpVideoThread<Decoder: VideoDecoder + 'static> {
    stream: Box<dyn Iterator<Item = PipelineEvent<Frame>>>,
    frame_sender: Sender<PipelineEvent<Frame>>,
    _decoder: PhantomData<Decoder>,
}

impl<Decoder: VideoDecoder> InitializableThread for RtpVideoThread<Decoder> {
    type InitOptions = (
        Arc<PipelineCtx>,
        DepayloaderOptions,
        Sender<PipelineEvent<Frame>>,
    );

    type SpawnOutput = RtpVideoTrackThreadHandle;
    type SpawnError = DecoderInitError;

    fn init(options: Self::InitOptions) -> Result<(Self, Self::SpawnOutput), Self::SpawnError> {
        let (ctx, depayloader_options, frame_sender) = options;

        let (rtp_packet_sender, rtp_packet_receiver) = crossbeam_channel::bounded(5);
        let depayloader_stream =
            DepayloaderStream::new(depayloader_options, rtp_packet_receiver.into_iter());
        let decoder_stream =
            VideoDecoderStream::<Decoder, _>::new(ctx, depayloader_stream.flatten())?;

        let state = Self {
            stream: Box::new(decoder_stream.flatten()),
            frame_sender,
            _decoder: PhantomData,
        };
        let output = RtpVideoTrackThreadHandle { rtp_packet_sender };
        Ok((state, output))
    }

    fn run(self) {
        for event in self.stream {
            if self.frame_sender.send(event).is_err() {
                warn!("Failed to send encoded video chunk from encoder. Channel closed.");
                return;
            }
        }
    }

    fn metadata() -> ThreadMetadata {
        ThreadMetadata {
            thread_name: format!("Rtp Video Decoder ({})", Decoder::LABEL),
            thread_instance_name: "Input".to_string(),
        }
    }
}
