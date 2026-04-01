use std::sync::Arc;

use tracing::{trace, warn};

use crate::{
    pipeline::{
        decoder::{
            AudioDecoderStream, DynamicVideoDecoderStream, KeyframeRequestSender,
            VideoDecoderMapping, libopus::OpusDecoder,
        },
        rtp::{
            RtpInputEvent,
            depayloader::{
                DepayloaderOptions, DepayloaderStream, DynamicDepayloaderStream,
                VideoPayloadTypeMapping,
            },
        },
        webrtc::AsyncReceiverIter,
    },
    utils::{InitializableThread, ThreadMetadata},
};

use crate::prelude::*;

pub(super) struct VideoTrackThreadHandle {
    pub rtp_packet_sender: tokio::sync::mpsc::Sender<PipelineEvent<RtpInputEvent>>,
}

pub(super) struct VideoTrackThread {
    stream: Box<dyn Iterator<Item = Frame>>,
    frame_sender: crossbeam_channel::Sender<Frame>,
}

impl InitializableThread for VideoTrackThread {
    type InitOptions = (
        Arc<PipelineCtx>,
        VideoDecoderMapping,
        VideoPayloadTypeMapping,
        crossbeam_channel::Sender<Frame>,
        KeyframeRequestSender,
    );

    type SpawnOutput = VideoTrackThreadHandle;
    type SpawnError = DecoderInitError;

    fn init(options: Self::InitOptions) -> Result<(Self, Self::SpawnOutput), Self::SpawnError> {
        let (ctx, decoder_mapping, payload_type_mapping, frame_sender, keyframe_request_sender) =
            options;
        let (rtp_packet_sender, rtp_packet_receiver) = tokio::sync::mpsc::channel(5000);

        let packet_stream = AsyncReceiverIter {
            receiver: rtp_packet_receiver,
        };

        let depayloader_stream =
            DynamicDepayloaderStream::new(payload_type_mapping, packet_stream).flatten();

        let decoder_stream = DynamicVideoDecoderStream::new(
            ctx,
            decoder_mapping,
            depayloader_stream,
            keyframe_request_sender,
        )
        .flatten()
        .inspect(|frame| trace!(?frame, "Frame produced"));

        let state = Self {
            stream: Box::new(decoder_stream),
            frame_sender,
        };
        let output = VideoTrackThreadHandle { rtp_packet_sender };
        Ok((state, output))
    }

    fn run(self) {
        for frame in self.stream {
            if self.frame_sender.send(frame).is_err() {
                warn!("Failed to send decoded video frame from decoder. Channel closed.");
                return;
            }
        }
    }

    fn metadata() -> ThreadMetadata {
        ThreadMetadata {
            thread_name: "Video Decoder".to_string(),
            thread_instance_name: "Input".to_string(),
        }
    }
}

pub(super) struct AudioTrackThreadHandle {
    pub rtp_packet_sender: tokio::sync::mpsc::Sender<PipelineEvent<RtpInputEvent>>,
}

pub(super) struct AudioTrackThread {
    stream: Box<dyn Iterator<Item = InputAudioSamples>>,
    samples_sender: crossbeam_channel::Sender<InputAudioSamples>,
}

impl InitializableThread for AudioTrackThread {
    type InitOptions = (
        Arc<PipelineCtx>,
        crossbeam_channel::Sender<InputAudioSamples>,
    );

    type SpawnOutput = AudioTrackThreadHandle;
    type SpawnError = DecoderInitError;

    fn init(options: Self::InitOptions) -> Result<(Self, Self::SpawnOutput), Self::SpawnError> {
        let (ctx, samples_sender) = options;

        let (rtp_packet_sender, rtp_packet_receiver) = tokio::sync::mpsc::channel(5000);

        let packet_stream = AsyncReceiverIter {
            receiver: rtp_packet_receiver,
        };

        let depayloader_stream =
            DepayloaderStream::new(DepayloaderOptions::Opus, packet_stream).flatten();

        let decoder_stream =
            AudioDecoderStream::<OpusDecoder, _>::new(ctx, (), depayloader_stream)?
                .flatten()
                .inspect(|batch| trace!(?batch, "Sample batch produced"));

        let state = Self {
            stream: Box::new(decoder_stream),
            samples_sender,
        };
        let output = AudioTrackThreadHandle { rtp_packet_sender };
        Ok((state, output))
    }

    fn run(self) {
        for samples in self.stream {
            if self.samples_sender.send(samples).is_err() {
                warn!("Failed to send decoded audio samples from decoder. Channel closed.");
                return;
            }
        }
    }

    fn metadata() -> ThreadMetadata {
        ThreadMetadata {
            thread_name: "Audio Decoder".to_string(),
            thread_instance_name: "Input".to_string(),
        }
    }
}
