use std::sync::Arc;

use crossbeam_channel::Sender;
use smelter_render::Frame;
use tokio::sync::oneshot;
use tracing::{debug, trace, warn};
use webrtc::{rtp_transceiver::rtp_receiver::RTCRtpReceiver, track::track_remote::TrackRemote};

use crate::{
    pipeline::{
        decoder::DynamicVideoDecoderStream,
        rtp::{
            RtpNtpSyncPoint, RtpPacket, RtpTimestampSync, depayloader::DynamicDepayloaderStream,
        },
        webrtc::{
            AsyncReceiverIter, listen_for_rtcp::listen_for_rtcp,
            negotiated_codecs::VideoCodecMappings,
        },
    },
    thread_utils::{InitializableThread, ThreadMetadata},
};

use crate::prelude::*;

pub(super) struct VideoInputTrackCtx {
    pub sync_point: Arc<RtpNtpSyncPoint>,
    pub track: Arc<TrackRemote>,
    pub frame_sender: Sender<PipelineEvent<Frame>>,
    pub rtc_receiver: Arc<RTCRtpReceiver>,
}

pub async fn video_input_processing_loop(
    ctx: Arc<PipelineCtx>,
    video_track_ctx: VideoInputTrackCtx,
    thread_instance_id: Arc<str>,
    video_codec_mappings: VideoCodecMappings,
) -> Result<(), DecoderInitError> {
    let VideoInputTrackCtx {
        sync_point,
        track,
        frame_sender,
        rtc_receiver,
    } = video_track_ctx;
    let handle = VideoTrackThread::spawn(
        thread_instance_id,
        (ctx.clone(), video_codec_mappings, frame_sender),
    )?;

    let mut timestamp_sync =
        RtpTimestampSync::new(&sync_point, 90_000, ctx.default_buffer_duration);

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

struct VideoTrackThreadHandle {
    rtp_packet_sender: tokio::sync::mpsc::Sender<PipelineEvent<RtpPacket>>,
}

struct VideoTrackThread {
    stream: Box<dyn Iterator<Item = PipelineEvent<Frame>>>,
    frame_sender: Sender<PipelineEvent<Frame>>,
}

impl InitializableThread for VideoTrackThread {
    type InitOptions = (
        Arc<PipelineCtx>,
        VideoCodecMappings,
        Sender<PipelineEvent<Frame>>,
    );

    type SpawnOutput = VideoTrackThreadHandle;
    type SpawnError = DecoderInitError;

    fn init(options: Self::InitOptions) -> Result<(Self, Self::SpawnOutput), Self::SpawnError> {
        let (ctx, video_mappings, frame_sender) = options;
        let (rtp_packet_sender, rtp_packet_receiver) = tokio::sync::mpsc::channel(5000);

        let packet_stream = AsyncReceiverIter {
            receiver: rtp_packet_receiver,
        };

        let depayloader_stream =
            DynamicDepayloaderStream::new(video_mappings.payload_type_mapping, packet_stream)
                .flatten();

        let decoder_stream =
            DynamicVideoDecoderStream::new(ctx, video_mappings.decoder_mapping, depayloader_stream)
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
