use std::{sync::Arc, time::Duration};

use crossbeam_channel::Sender;
use smelter_render::Frame;
use tracing::{Instrument, debug, trace, warn};
use webrtc::{
    rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication,
    rtp_transceiver::rtp_receiver::RTCRtpReceiver, track::track_remote::TrackRemote,
};

use crate::{
    pipeline::{
        decoder::{
            AudioDecoderStream, DynamicVideoDecoderStream, KeyframeRequestSender,
            VideoDecoderMapping, libopus::OpusDecoder,
        },
        resampler::decoder_resampler::ResampledDecoderStream,
        rtp::{
            RtpPacket,
            depayloader::{
                DepayloaderOptions, DepayloaderStream, DynamicDepayloaderStream,
                VideoPayloadTypeMapping,
            },
        },
        webrtc::AsyncReceiverIter,
    },
    thread_utils::{InitializableThread, ThreadMetadata},
};

use crate::prelude::*;

pub fn start_pli_sender_task(
    track: &Arc<TrackRemote>,
    rtc_receiver: &Arc<RTCRtpReceiver>,
) -> KeyframeRequestSender {
    let (keyframe_request_sender, mut keyframe_request_receiver) =
        KeyframeRequestSender::new_async();
    let ssrc = track.ssrc();
    let transport = rtc_receiver.transport();
    tokio::spawn(
        async move {
            while keyframe_request_receiver.recv().await.is_some() {
                debug!(ssrc, "Sending PLI");
                let pli = PictureLossIndication {
                    // For receive-only endpoints RTP sender SSRC can be set to 0.
                    sender_ssrc: 0,
                    media_ssrc: ssrc,
                };

                if let Err(err) = transport.write_rtcp(&[Box::new(pli)]).await {
                    warn!(%err, "Failed to send RTCP packet (PictureLossIndication)")
                }
                tokio::time::sleep(Duration::from_secs(1)).await
            }
        }
        .instrument(tracing::Span::current()),
    );

    keyframe_request_sender
}

pub(super) struct VideoTrackThreadHandle {
    pub rtp_packet_sender: tokio::sync::mpsc::Sender<PipelineEvent<RtpPacket>>,
}

pub(super) struct VideoTrackThread {
    stream: Box<dyn Iterator<Item = PipelineEvent<Frame>>>,
    frame_sender: Sender<PipelineEvent<Frame>>,
}

impl InitializableThread for VideoTrackThread {
    type InitOptions = (
        Arc<PipelineCtx>,
        VideoDecoderMapping,
        VideoPayloadTypeMapping,
        Sender<PipelineEvent<Frame>>,
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
            frame_sender,
        };
        let output = VideoTrackThreadHandle { rtp_packet_sender };
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
            thread_name: "Video Decoder".to_string(),
            thread_instance_name: "Input".to_string(),
        }
    }
}

pub(super) struct AudioTrackThreadHandle {
    pub rtp_packet_sender: tokio::sync::mpsc::Sender<PipelineEvent<RtpPacket>>,
}

pub(super) struct AudioTrackThread {
    stream: Box<dyn Iterator<Item = PipelineEvent<InputAudioSamples>>>,
    samples_sender: Sender<PipelineEvent<InputAudioSamples>>,
}

impl InitializableThread for AudioTrackThread {
    type InitOptions = (Arc<PipelineCtx>, Sender<PipelineEvent<InputAudioSamples>>);

    type SpawnOutput = AudioTrackThreadHandle;
    type SpawnError = DecoderInitError;

    fn init(options: Self::InitOptions) -> Result<(Self, Self::SpawnOutput), Self::SpawnError> {
        let (ctx, samples_sender) = options;

        let (rtp_packet_sender, rtp_packet_receiver) = tokio::sync::mpsc::channel(5000);
        let output_sample_rate = ctx.mixing_sample_rate;

        let packet_stream = AsyncReceiverIter {
            receiver: rtp_packet_receiver,
        };

        let depayloader_stream =
            DepayloaderStream::new(DepayloaderOptions::Opus, packet_stream).flatten();

        let decoded_stream =
            AudioDecoderStream::<OpusDecoder, _>::new(ctx, (), depayloader_stream)?.flatten();

        let resampled_stream =
            ResampledDecoderStream::new(output_sample_rate, decoded_stream).flatten();

        let result_stream = resampled_stream
            .filter_map(|event| match event {
                PipelineEvent::Data(batch) => Some(PipelineEvent::Data(batch)),
                // Do not send EOS to queue
                // TODO: maybe queue should be able to handle packets after EOS
                PipelineEvent::EOS => None,
            })
            .inspect(|batch| trace!(?batch, "Sample batch produced"));

        let state = Self {
            stream: Box::new(result_stream),
            samples_sender,
        };
        let output = AudioTrackThreadHandle { rtp_packet_sender };
        Ok((state, output))
    }

    fn run(self) {
        for event in self.stream {
            if self.samples_sender.send(event).is_err() {
                warn!("Failed to send encoded audio chunk from decoder. Channel closed.");
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
