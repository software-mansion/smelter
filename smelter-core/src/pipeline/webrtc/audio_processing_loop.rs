use std::sync::Arc;

use crossbeam_channel::Sender;
use tokio::sync::oneshot;
use tracing::{debug, trace, warn};
use webrtc::{rtp_transceiver::rtp_receiver::RTCRtpReceiver, track::track_remote::TrackRemote};

use crate::{
    pipeline::{
        PipelineCtx,
        decoder::{AudioDecoderStream, libopus::OpusDecoder},
        resampler::decoder_resampler::ResampledDecoderStream,
        rtp::{
            RtpNtpSyncPoint, RtpPacket, RtpTimestampSync,
            depayloader::{DepayloaderOptions, DepayloaderStream},
        },
        webrtc::listen_for_rtcp::listen_for_rtcp,
    },
    thread_utils::{InitializableThread, ThreadMetadata},
};

use crate::prelude::*;

pub struct AudioTrackCtx {
    pub sync_point: Arc<RtpNtpSyncPoint>,
    pub track: Arc<TrackRemote>,
    pub samples_sender: Sender<PipelineEvent<InputAudioSamples>>,
    pub rtc_receiver: Arc<RTCRtpReceiver>,
}

pub async fn audio_processing_loop(
    ctx: Arc<PipelineCtx>,
    audio_track_ctx: AudioTrackCtx,
    thread_instance_id: Arc<str>,
) -> Result<(), DecoderInitError> {
    let AudioTrackCtx {
        sync_point,
        track,
        samples_sender,
        rtc_receiver,
    } = audio_track_ctx;
    let handle = AudioTrackThread::spawn(thread_instance_id, (ctx.clone(), samples_sender))?;

    let mut timestamp_sync =
        RtpTimestampSync::new(&sync_point, 48_000, ctx.default_buffer_duration);

    let (sender_report_sender, mut sender_report_receiver) = oneshot::channel();
    listen_for_rtcp(&ctx, rtc_receiver, sender_report_sender);

    while let Ok((packet, _)) = track.read_rtp().await {
        if let Ok(report) = sender_report_receiver.try_recv() {
            timestamp_sync.on_sender_report(report.ntp_time, report.rtp_time);
        }
        let timestamp = timestamp_sync.pts_from_timestamp(packet.header.timestamp);

        let packet = RtpPacket { packet, timestamp };
        trace!(?packet, "Sending RTP packet");
        if let Err(e) = handle
            .rtp_packet_sender
            .send(PipelineEvent::Data(packet))
            .await
        {
            debug!("Failed to send audio RTP packet: {e}");
        }
    }

    Ok(())
}

struct AsyncReceiverIter<T> {
    pub receiver: tokio::sync::mpsc::Receiver<T>,
}

impl<T> Iterator for AsyncReceiverIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.receiver.blocking_recv()
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
            .inspect(|batch| trace!(?batch, "Produced a sample batch"));

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
