use std::{marker::PhantomData, sync::Arc};

use crossbeam_channel::Sender;
use tracing::warn;

use crate::{
    pipeline::{
        decoder::{VideoDecoder, VideoDecoderStream},
        rtp::{
            RtpInputEvent,
            depayloader::{DepayloaderOptions, DepayloaderStream},
        },
    },
    queue::WeakQueueInput,
    utils::{InitializableThread, ThreadMetadata},
};

use crate::prelude::*;

pub(crate) struct RtpVideoTrackThreadHandle {
    pub rtp_packet_sender: Sender<PipelineEvent<RtpInputEvent>>,
}

pub(super) struct RtpVideoThread<Decoder: VideoDecoder + 'static> {
    stream: Box<dyn Iterator<Item = PipelineEvent<Frame>>>,
    queue_input: WeakQueueInput,
    _decoder: PhantomData<Decoder>,
}

impl<Decoder: VideoDecoder> InitializableThread for RtpVideoThread<Decoder> {
    type InitOptions = (Arc<PipelineCtx>, DepayloaderOptions, WeakQueueInput);

    type SpawnOutput = RtpVideoTrackThreadHandle;
    type SpawnError = DecoderInitError;

    fn init(options: Self::InitOptions) -> Result<(Self, Self::SpawnOutput), Self::SpawnError> {
        let (ctx, depayloader_options, queue_input) = options;

        let (rtp_packet_sender, rtp_packet_receiver) = crossbeam_channel::bounded(5);
        let depayloader_stream =
            DepayloaderStream::new(depayloader_options, rtp_packet_receiver.into_iter());
        let decoder_stream =
            VideoDecoderStream::<Decoder, _>::new(ctx, depayloader_stream.flatten())?;

        let state = Self {
            stream: Box::new(decoder_stream.flatten()),
            queue_input,
            _decoder: PhantomData,
        };
        let output = RtpVideoTrackThreadHandle { rtp_packet_sender };
        Ok((state, output))
    }

    fn run(self) {
        for event in self.stream {
            if self.queue_input.send_video(event).is_err() {
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
