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
    queue::WeakQueueInput,
    utils::{InitializableThread, ThreadMetadata},
};

use crate::prelude::*;

pub(super) struct VideoTrackThreadHandle {
    pub rtp_packet_sender: tokio::sync::mpsc::Sender<PipelineEvent<RtpInputEvent>>,
}

pub(super) struct VideoTrackThread {
    stream: Box<dyn Iterator<Item = PipelineEvent<Frame>>>,
    queue_input: WeakQueueInput,
}

impl InitializableThread for VideoTrackThread {
    type InitOptions = (
        Arc<PipelineCtx>,
        VideoDecoderMapping,
        VideoPayloadTypeMapping,
        WeakQueueInput,
        KeyframeRequestSender,
    );

    type SpawnOutput = VideoTrackThreadHandle;
    type SpawnError = DecoderInitError;

    fn init(options: Self::InitOptions) -> Result<(Self, Self::SpawnOutput), Self::SpawnError> {
        let (ctx, decoder_mapping, payload_type_mapping, queue_input, keyframe_request_sender) =
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
        .flatten();

        let result_stream = decoder_stream
            .filter_map(|event| match event {
                PipelineEvent::Data(frame) => Some(PipelineEvent::Data(frame)),
                // Do not send EOS to queue
                // TODO: maybe queue should be able to handle packets after EOS
                PipelineEvent::EOS => None,
            })
            .inspect(|frame| trace!(?frame, "Frame produced"));

        let state = Self {
            stream: Box::new(result_stream),
            queue_input,
        };
        let output = VideoTrackThreadHandle { rtp_packet_sender };
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
            thread_name: "Video Decoder".to_string(),
            thread_instance_name: "Input".to_string(),
        }
    }
}

pub(super) struct AudioTrackThreadHandle {
    pub rtp_packet_sender: tokio::sync::mpsc::Sender<PipelineEvent<RtpInputEvent>>,
}

pub(super) struct AudioTrackThread {
    stream: Box<dyn Iterator<Item = PipelineEvent<InputAudioSamples>>>,
    queue_input: WeakQueueInput,
}

impl InitializableThread for AudioTrackThread {
    type InitOptions = (Arc<PipelineCtx>, WeakQueueInput);

    type SpawnOutput = AudioTrackThreadHandle;
    type SpawnError = DecoderInitError;

    fn init(options: Self::InitOptions) -> Result<(Self, Self::SpawnOutput), Self::SpawnError> {
        let (ctx, queue_input) = options;

        let (rtp_packet_sender, rtp_packet_receiver) = tokio::sync::mpsc::channel(5000);

        let packet_stream = AsyncReceiverIter {
            receiver: rtp_packet_receiver,
        };

        let depayloader_stream =
            DepayloaderStream::new(DepayloaderOptions::Opus, packet_stream).flatten();

        let decoded_stream =
            AudioDecoderStream::<OpusDecoder, _>::new(ctx, (), depayloader_stream)?.flatten();

        let result_stream = decoded_stream
            .filter_map(|event| match event {
                PipelineEvent::Data(batch) => Some(PipelineEvent::Data(batch)),
                // Do not send EOS to queue
                // TODO: maybe queue should be able to handle packets after EOS
                PipelineEvent::EOS => None,
            })
            .inspect(|batch| trace!(?batch, "Sample batch produced"));

        let state = Self {
            stream: Box::new(result_stream),
            queue_input,
        };
        let output = AudioTrackThreadHandle { rtp_packet_sender };
        Ok((state, output))
    }

    fn run(self) {
        for event in self.stream {
            if self.queue_input.send_audio(event).is_err() {
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
