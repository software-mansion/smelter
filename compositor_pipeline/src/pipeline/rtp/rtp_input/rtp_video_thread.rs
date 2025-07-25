use std::{marker::PhantomData, sync::Arc};

use compositor_render::Frame;
use crossbeam_channel::Sender;
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
    thread_utils::InitializableThread,
};

pub(crate) struct RtpVideoTrackThreadHandle {
    pub rtp_packet_sender: Sender<PipelineEvent<RtpPacket>>,
}

pub(super) struct RtpVideoThread<Decoder: VideoDecoder + 'static>(PhantomData<Decoder>);

impl<Decoder: VideoDecoder> InitializableThread for RtpVideoThread<Decoder> {
    type InitOptions = (
        Arc<PipelineCtx>,
        DepayloaderOptions,
        Sender<PipelineEvent<Frame>>,
    );

    type SpawnOutput = RtpVideoTrackThreadHandle;
    type SpawnError = DecoderInitError;

    type ThreadState = (
        Box<dyn Iterator<Item = PipelineEvent<Frame>>>,
        Sender<PipelineEvent<Frame>>,
    );

    const LABEL: &'static str = Decoder::LABEL;

    fn init(
        options: Self::InitOptions,
    ) -> Result<(Self::SpawnOutput, Self::ThreadState), Self::SpawnError> {
        let (ctx, depayloader_options, frame_sender) = options;

        let (rtp_packet_sender, rtp_packet_receiver) = crossbeam_channel::bounded(5);
        let depayloader_stream =
            DepayloaderStream::new(depayloader_options, rtp_packet_receiver.into_iter());
        let decoder_stream =
            VideoDecoderStream::<Decoder, _>::new(ctx, depayloader_stream.flatten())?;

        let output = RtpVideoTrackThreadHandle { rtp_packet_sender };
        let state = (Box::new(decoder_stream.flatten()) as Box<_>, frame_sender);
        Ok((output, state))
    }

    fn run(state: Self::ThreadState) {
        let (stream, frame_sender) = state;
        for event in stream {
            if frame_sender.send(event).is_err() {
                warn!("Failed to send encoded video chunk from encoder. Channel closed.");
                return;
            }
        }
    }
}
