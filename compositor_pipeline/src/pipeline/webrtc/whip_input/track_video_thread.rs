use std::{iter, sync::Arc};

use compositor_render::{error::ErrorStack, Frame};
use crossbeam_channel::Sender;
use tokio::sync::oneshot;
use tracing::{debug, error, trace, warn};
use webrtc::{
    rtp_transceiver::{PayloadType, RTCRtpTransceiver},
    track::track_remote::TrackRemote,
};

use crate::{
    pipeline::{
        decoder::{
            ffmpeg_h264::FfmpegH264Decoder, ffmpeg_vp8::FfmpegVp8Decoder,
            ffmpeg_vp9::FfmpegVp9Decoder, vulkan_h264::VulkanH264Decoder, VideoDecoder,
            VideoDecoderInstance,
        },
        rtp::{
            depayloader::{new_depayloader, Depayloader, DepayloaderOptions, DepayloadingError},
            RtpNtpSyncPoint, RtpPacket, RtpTimestampSync,
        },
        webrtc::{
            error::WhipWhepServerError,
            whip_input::{
                negotiated_codecs::NegotiatedVideoCodecsInfo, utils::listen_for_rtcp,
                AsyncReceiverIter,
            },
            WhipWhepServerState,
        },
    },
    thread_utils::{InitializableThread, ThreadMetadata},
};

use crate::prelude::*;

pub async fn process_video_track(
    sync_point: Arc<RtpNtpSyncPoint>,
    state: WhipWhepServerState,
    session_id: Arc<str>,
    track: Arc<TrackRemote>,
    transceiver: Arc<RTCRtpTransceiver>,
    video_preferences: Vec<VideoDecoderOptions>,
) -> Result<(), WhipWhepServerError> {
    let rtc_receiver = transceiver.receiver().await;
    let Some(negotiated_codecs) =
        NegotiatedVideoCodecsInfo::new(transceiver, &video_preferences).await
    else {
        warn!("Skipping video track, no valid codec negotiated");
        return Err(WhipWhepServerError::InternalError(
            "No video codecs negotiated".to_string(),
        ));
    };

    let WhipWhepServerState { inputs, ctx, .. } = state;
    let frame_sender = inputs.get_with(&session_id, |input| Ok(input.frame_sender.clone()))?;
    let handle =
        VideoTrackThread::spawn(&session_id, (ctx.clone(), negotiated_codecs, frame_sender))?;

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

pub(crate) struct VideoTrackThreadHandle {
    pub rtp_packet_sender: tokio::sync::mpsc::Sender<PipelineEvent<RtpPacket>>,
}

pub(super) struct VideoTrackThread {
    stream: Box<dyn Iterator<Item = PipelineEvent<Frame>>>,
    frame_sender: Sender<PipelineEvent<Frame>>,
}

impl InitializableThread for VideoTrackThread {
    type InitOptions = (
        Arc<PipelineCtx>,
        NegotiatedVideoCodecsInfo,
        Sender<PipelineEvent<Frame>>,
    );

    type SpawnOutput = VideoTrackThreadHandle;
    type SpawnError = DecoderInitError;

    fn init(options: Self::InitOptions) -> Result<(Self, Self::SpawnOutput), Self::SpawnError> {
        let (ctx, codec_info, frame_sender) = options;
        let (rtp_packet_sender, rtp_packet_receiver) = tokio::sync::mpsc::channel(5000);

        let packet_stream = AsyncReceiverIter {
            receiver: rtp_packet_receiver,
        };

        let depayloader_stream =
            DynamicDepayloaderStream::new(codec_info.clone(), packet_stream).flatten();

        let decoder_stream =
            DynamicVideoDecoderStream::new(ctx, codec_info, depayloader_stream).flatten();

        let result_stream = decoder_stream
            .filter_map(|event| match event {
                PipelineEvent::Data(frame) => Some(PipelineEvent::Data(frame)),
                // Do not send EOS to queue
                // TODO: maybe queue should be able to handle packets after EOS
                PipelineEvent::EOS => None,
            })
            .inspect(|frame| trace!(?frame, "WHIP input produced a frame"));

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
            thread_name: "Whip Video Decoder".to_string(),
            thread_instance_name: "Input".to_string(),
        }
    }
}

struct DynamicVideoDecoderStream<Source>
where
    Source: Iterator<Item = PipelineEvent<EncodedInputChunk>>,
{
    ctx: Arc<PipelineCtx>,
    decoder: Option<Box<dyn VideoDecoderInstance>>,
    last_chunk_kind: Option<MediaKind>,
    source: Source,
    eos_sent: bool,
    codec_info: NegotiatedVideoCodecsInfo,
}

impl<Source> DynamicVideoDecoderStream<Source>
where
    Source: Iterator<Item = PipelineEvent<EncodedInputChunk>>,
{
    fn new(ctx: Arc<PipelineCtx>, codec_info: NegotiatedVideoCodecsInfo, source: Source) -> Self {
        Self {
            ctx,
            decoder: None,
            last_chunk_kind: None,
            source,
            eos_sent: false,
            codec_info,
        }
    }

    fn ensure_decoder(&mut self, chunk_kind: MediaKind) {
        if self.last_chunk_kind == Some(chunk_kind) {
            return;
        }
        self.last_chunk_kind = Some(chunk_kind);
        let preferred_decoder = match chunk_kind {
            MediaKind::Video(VideoCodec::H264) => self
                .codec_info
                .h264
                .as_ref()
                .map(|info| info.preferred_decoder),
            MediaKind::Video(VideoCodec::Vp8) => self
                .codec_info
                .vp8
                .as_ref()
                .map(|info| info.preferred_decoder),
            MediaKind::Video(VideoCodec::Vp9) => self
                .codec_info
                .vp9
                .as_ref()
                .map(|info| info.preferred_decoder),
            MediaKind::Audio(_) => {
                error!("Found audio packet in video stream.");
                None
            }
        };
        let Some(preferred_decoder) = preferred_decoder else {
            error!("No matching decoder found");
            return;
        };
        let decoder = match self.create_decoder(preferred_decoder) {
            Ok(decoder) => decoder,
            Err(err) => {
                error!(
                    "Failed to instantiate a decoder {}",
                    ErrorStack::new(&err).into_string()
                );
                return;
            }
        };
        self.decoder = Some(decoder);
    }

    fn create_decoder(
        &self,
        decoder: VideoDecoderOptions,
    ) -> Result<Box<dyn VideoDecoderInstance>, DecoderInitError> {
        let decoder: Box<dyn VideoDecoderInstance> = match decoder {
            VideoDecoderOptions::FfmpegH264 => Box::new(FfmpegH264Decoder::new(&self.ctx)?),
            VideoDecoderOptions::FfmpegVp8 => Box::new(FfmpegVp8Decoder::new(&self.ctx)?),
            VideoDecoderOptions::FfmpegVp9 => Box::new(FfmpegVp9Decoder::new(&self.ctx)?),
            VideoDecoderOptions::VulkanH264 => Box::new(VulkanH264Decoder::new(&self.ctx)?),
        };
        Ok(decoder)
    }
}

impl<Source> Iterator for DynamicVideoDecoderStream<Source>
where
    Source: Iterator<Item = PipelineEvent<EncodedInputChunk>>,
{
    type Item = Vec<PipelineEvent<Frame>>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.source.next() {
            Some(PipelineEvent::Data(samples)) => {
                // TODO: flush on decoder change
                self.ensure_decoder(samples.kind);
                let decoder = self.decoder.as_mut()?;
                let chunks = decoder.decode(samples);
                Some(chunks.into_iter().map(PipelineEvent::Data).collect())
            }
            Some(PipelineEvent::EOS) | None => match self.eos_sent {
                true => None,
                false => {
                    let chunks = self
                        .decoder
                        .as_mut()
                        .map(|decoder| decoder.flush())
                        .unwrap_or_default();
                    let events = chunks.into_iter().map(PipelineEvent::Data);
                    let eos = iter::once(PipelineEvent::EOS);
                    self.eos_sent = true;
                    Some(events.chain(eos).collect())
                }
            },
        }
    }
}

struct DynamicDepayloaderStream<Source>
where
    Source: Iterator<Item = PipelineEvent<RtpPacket>>,
{
    depayloader: Option<Box<dyn Depayloader>>,
    last_payload_type: Option<PayloadType>,
    source: Source,
    eos_sent: bool,
    codec_info: NegotiatedVideoCodecsInfo,
}

impl<Source> DynamicDepayloaderStream<Source>
where
    Source: Iterator<Item = PipelineEvent<RtpPacket>>,
{
    fn new(codec_info: NegotiatedVideoCodecsInfo, source: Source) -> Self {
        Self {
            source,
            eos_sent: false,
            codec_info,
            depayloader: None,
            last_payload_type: None,
        }
    }

    fn ensure_depayloader(&mut self, payload_type: PayloadType) {
        if self.last_payload_type == Some(payload_type) {
            return;
        }
        self.last_payload_type = Some(payload_type);
        if self.codec_info.is_payload_type_h264(payload_type) {
            self.depayloader = Some(new_depayloader(DepayloaderOptions::H264));
        } else if self.codec_info.is_payload_type_vp8(payload_type) {
            self.depayloader = Some(new_depayloader(DepayloaderOptions::Vp8));
        } else if self.codec_info.is_payload_type_vp9(payload_type) {
            self.depayloader = Some(new_depayloader(DepayloaderOptions::Vp9));
        } else {
            error!("Failed to create depayloader for payload_type: {payload_type}")
        }
    }
}

impl<Source> Iterator for DynamicDepayloaderStream<Source>
where
    Source: Iterator<Item = PipelineEvent<RtpPacket>>,
{
    type Item = Vec<PipelineEvent<EncodedInputChunk>>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.source.next() {
            Some(PipelineEvent::Data(packet)) => {
                self.ensure_depayloader(packet.packet.header.payload_type);
                let depayloader = self.depayloader.as_mut()?;
                match depayloader.depayload(packet) {
                    Ok(chunks) => Some(chunks.into_iter().map(PipelineEvent::Data).collect()),
                    // TODO: Remove after updating webrc-rs
                    Err(DepayloadingError::Rtp(rtp::Error::ErrShortPacket)) => Some(vec![]),
                    Err(err) => {
                        warn!("Depayloader error: {}", ErrorStack::new(&err).into_string());
                        Some(vec![])
                    }
                }
            }
            Some(PipelineEvent::EOS) | None => match self.eos_sent {
                true => None,
                false => {
                    self.eos_sent = true;
                    Some(vec![PipelineEvent::EOS])
                }
            },
        }
    }
}
