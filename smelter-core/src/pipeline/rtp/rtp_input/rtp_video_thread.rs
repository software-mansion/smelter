use std::{marker::PhantomData, sync::Arc, time::Duration};

use tracing::warn;

use crate::{
    pipeline::{
        decoder::{VideoDecoder, VideoDecoderStream},
        rtp::{
            RtpInputEvent,
            depayloader::{DepayloaderOptions, DepayloaderStream},
        },
    },
    queue::QueueSender,
    utils::{
        InitializableThread, ThreadMetadata,
        channel::{Sender, duration_bounded},
    },
};

use crate::prelude::*;

const RTP_BUFFER: Duration = Duration::from_secs(1);

pub(crate) struct RtpVideoTrackThreadHandle {
    pub rtp_packet_sender: Sender<PipelineEvent<RtpInputEvent>>,
}

pub(super) struct RtpVideoThread<Decoder: VideoDecoder + 'static> {
    stream: Box<dyn Iterator<Item = Frame>>,
    frame_sender: QueueSender<Frame>,
    _decoder: PhantomData<Decoder>,
}

impl<Decoder: VideoDecoder> InitializableThread for RtpVideoThread<Decoder> {
    type InitOptions = (Arc<PipelineCtx>, DepayloaderOptions, QueueSender<Frame>);

    type SpawnOutput = RtpVideoTrackThreadHandle;
    type SpawnError = DecoderInitError;

    fn init(options: Self::InitOptions) -> Result<(Self, Self::SpawnOutput), Self::SpawnError> {
        let (ctx, depayloader_options, frame_sender) = options;

        let (rtp_packet_sender, rtp_packet_receiver) = duration_bounded(RTP_BUFFER);
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
                warn!("Failed to send decoded video frame from decoder. Channel closed.");
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
