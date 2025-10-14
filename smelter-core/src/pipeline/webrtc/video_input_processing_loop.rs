use std::sync::Arc;

use crossbeam_channel::Sender;
use smelter_render::Frame;
use tokio::sync::oneshot;
use tracing::{debug, trace, warn};
use webrtc::{rtp_transceiver::rtp_receiver::RTCRtpReceiver, track::track_remote::TrackRemote};

use crate::{
    pipeline::{
        decoder::{DynamicVideoDecoderStream, VideoDecoderMapping},
        rtp::{
            RtpNtpSyncPoint, RtpPacket, RtpTimestampSync,
            depayloader::{DynamicDepayloaderStream, VideoPayloadTypeMapping},
        },
        webrtc::{AsyncReceiverIter, rtcp_utils::listen_for_sender_reports},
    },
    thread_utils::{InitializableThread, ThreadMetadata},
};

use crate::prelude::*;

pub(super) struct VideoInputLoop {
    pub sync_point: Arc<RtpNtpSyncPoint>,
    pub track: Arc<TrackRemote>,
    pub rtc_receiver: Arc<RTCRtpReceiver>,
    pub handle: VideoTrackThreadHandle,
}

impl VideoInputLoop {
    pub(super) async fn run(self, ctx: Arc<PipelineCtx>) -> Result<(), DecoderInitError> {
        let mut timestamp_sync =
            RtpTimestampSync::new(&self.sync_point, 90_000, ctx.default_buffer_duration);

        let (sender_report_sender, mut sender_report_receiver) = oneshot::channel();
        listen_for_sender_reports(&ctx, self.rtc_receiver, sender_report_sender);

        while let Ok((packet, _)) = self.track.read_rtp().await {
            if let Ok(report) = sender_report_receiver.try_recv() {
                timestamp_sync.on_sender_report(report.ntp_time, report.rtp_time);
            }
            let timestamp = timestamp_sync.pts_from_timestamp(packet.header.timestamp);

            let packet = RtpPacket { packet, timestamp };
            trace!(?packet, "Sending RTP packet");
            if let Err(e) = self
                .handle
                .rtp_packet_sender
                .send(PipelineEvent::Data(packet))
                .await
            {
                debug!("Failed to send audio RTP packet: {e}");
            }
        }
        Ok(())
    }
}

pub(super) struct VideoTrackThreadHandle {
    rtp_packet_sender: tokio::sync::mpsc::Sender<PipelineEvent<RtpPacket>>,
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
    );

    type SpawnOutput = VideoTrackThreadHandle;
    type SpawnError = DecoderInitError;

    fn init(options: Self::InitOptions) -> Result<(Self, Self::SpawnOutput), Self::SpawnError> {
        let (ctx, decoder_mapping, payload_type_mapping, frame_sender) = options;
        let (rtp_packet_sender, rtp_packet_receiver) = tokio::sync::mpsc::channel(5000);

        let packet_stream = AsyncReceiverIter {
            receiver: rtp_packet_receiver,
        };

        let depayloader_stream =
            DynamicDepayloaderStream::new(payload_type_mapping, packet_stream).flatten();

        let decoder_stream =
            DynamicVideoDecoderStream::new(ctx, decoder_mapping, depayloader_stream).flatten();

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
