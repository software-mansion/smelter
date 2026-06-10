use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use bytes::Bytes;
use moq_mux::{catalog::hang::Container, container::Consumer as ContainerConsumer};
use moq_native::moq_net::{BroadcastConsumer, Error as MoqError, Track};
use smelter_render::error::ErrorStack;
use tracing::{info, trace, warn};

use crate::pipeline::{
    decoder::{
        DecoderThreadHandle,
        decoder_thread_audio::{AudioDecoderThread, AudioDecoderThreadOptions},
        decoder_thread_video::{VideoDecoderThread, VideoDecoderThreadOptions},
        fdk_aac::FdkAacDecoder,
        ffmpeg_h264,
        libopus::OpusDecoder,
        vulkan_h264,
    },
    moq::state::MoqInputState,
};
use crate::prelude::*;
use crate::queue::{QueueSender, QueueTrackOffset, QueueTrackOptions};
use crate::utils::{H264AvcDecoderConfig, H264AvccToAnnexB, InitializableThread};

use self::catalog::{MoqCatalogError, read_catalog};

mod catalog;

const MOQ_BUFFER: Duration = Duration::from_secs(1);
const MOQ_MAX_BUFFER: Duration = Duration::from_secs(20);

struct DiscoveredVideo {
    name: String,
    container: Container,
    description: Option<Bytes>,
}

struct DiscoveredAudio {
    name: String,
    codec: AudioCodec,
    container: Container,
    description: Option<Bytes>,
}

struct DiscoveredTracks {
    video: Option<DiscoveredVideo>,
    audio: Option<DiscoveredAudio>,
}

pub(crate) fn spawn_broadcast_handler(
    ctx: Arc<PipelineCtx>,
    input_ref: &Ref<InputId>,
    input: &MoqInputState,
    broadcast: BroadcastConsumer,
) -> Option<tokio::task::JoinHandle<()>> {
    let queue_input = input.queue_input.upgrade()?;

    let input_ref = input_ref.clone();
    let decoders = input.decoders.clone();
    let rt = ctx.tokio_rt.clone();

    let handle = rt.spawn(async move {
        info!(input_id = %input_ref, "MoQ broadcast connection established");

        let mut discovered = match read_catalog(&broadcast).await {
            Ok(d) => d,
            Err(err) => {
                warn!(
                    input_id = %input_ref,
                    "MoQ catalog error: {}",
                    ErrorStack::new(&err).into_string()
                );
                return;
            }
        };

        let has_video = discovered.video.is_some();
        let has_audio = discovered.audio.is_some();

        // TODO: This has to be handled in a more reliable way that does not introduce high latency,
        // probably jitter buffer.
        let (video_sender, audio_sender) = queue_input.queue_new_track(QueueTrackOptions {
            video: has_video,
            audio: has_audio,
            offset: QueueTrackOffset::Pts(ctx.queue_ctx.effective_last_pts() + MOQ_BUFFER),
        });

        if let Some(v) = &discovered.video {
            info!(input_id = %input_ref, track = %v.name, "Discovered MoQ video track");
        }
        if let Some(a) = &discovered.audio {
            info!(input_id = %input_ref, track = %a.name, "Discovered MoQ audio track");
        }

        let video = discovered.video.take();
        let audio = discovered.audio.take();

        // Shared across audio and video so both tracks are normalized against
        // the same first PTS, preserving A/V synchronization. Whichever track
        // produces the first frame sets the common zero point for both.
        let first_pts = Arc::new(Mutex::new(None));

        let stats_sender = MoqStatsSender::new(input_ref.clone(), ctx.stats_sender.clone());

        let first_pts_inner = first_pts.clone();
        let stats_sender_inner = stats_sender.clone();
        let video_fut = async {
            if let (Some(video), Some(frame_sender)) = (video, video_sender) {
                let decoder_handle =
                    spawn_video_decoder(&ctx, &input_ref, &decoders, &video, frame_sender);
                if let Some(decoder_handle) = decoder_handle {
                    run_video_track(
                        video,
                        decoder_handle,
                        &broadcast,
                        first_pts_inner,
                        stats_sender_inner,
                    )
                    .await;
                }
            }
        };

        let audio_fut = async {
            if let (Some(audio), Some(sample_sender)) = (audio, audio_sender) {
                let decoder_handle = spawn_audio_decoder(&ctx, &input_ref, &audio, sample_sender);
                if let Some(decoder_handle) = decoder_handle {
                    run_audio_track(audio, decoder_handle, &broadcast, first_pts, stats_sender)
                        .await;
                }
            }
        };

        tokio::join!(video_fut, audio_fut);
        info!(input_id = %input_ref, "MoQ broadcast connection closed");
    });

    Some(handle)
}

fn spawn_video_decoder(
    ctx: &Arc<PipelineCtx>,
    input_ref: &Ref<InputId>,
    decoders: &MoqServerInputDecoders,
    discovered: &DiscoveredVideo,
    frame_sender: QueueSender<Frame>,
) -> Option<DecoderThreadHandle> {
    match process_video_config(ctx, input_ref, decoders, discovered, frame_sender) {
        Ok(handle) => Some(handle),
        Err(err) => {
            warn!(
                "MoQ video config error: {}",
                ErrorStack::new(&err).into_string()
            );
            None
        }
    }
}

fn spawn_audio_decoder(
    ctx: &Arc<PipelineCtx>,
    input_ref: &Ref<InputId>,
    discovered: &DiscoveredAudio,
    sample_sender: QueueSender<InputAudioSamples>,
) -> Option<DecoderThreadHandle> {
    match process_audio_config(ctx, input_ref, discovered, sample_sender) {
        Ok(handle) => Some(handle),
        Err(err) => {
            warn!(
                "MoQ audio config error: {}",
                ErrorStack::new(&err).into_string()
            );
            None
        }
    }
}

fn process_video_config(
    ctx: &Arc<PipelineCtx>,
    input_ref: &Ref<InputId>,
    decoders: &MoqServerInputDecoders,
    video: &DiscoveredVideo,
    frame_sender: QueueSender<Frame>,
) -> Result<DecoderThreadHandle, MoqConnectionError> {
    // Only CMAF H264 is allowed right now, other codecs are rejected before this function
    let avcc_bytes = video
        .description
        .clone()
        .ok_or(MoqConnectionError::InvalidAvcc)?;

    let h264_config =
        H264AvcDecoderConfig::parse(avcc_bytes).map_err(|_| MoqConnectionError::InvalidAvcc)?;
    let options = VideoDecoderThreadOptions {
        ctx: ctx.clone(),
        transformer: Some(H264AvccToAnnexB::new(h264_config)),
        frame_sender,
        input_buffer_size: MOQ_MAX_BUFFER,
    };

    let h264_decoder =
        decoders
            .h264
            .unwrap_or_else(|| match ctx.graphics_context.has_vulkan_decoder_support() {
                true => VideoDecoderOptions::VulkanH264,
                false => VideoDecoderOptions::FfmpegH264,
            });

    match h264_decoder {
        VideoDecoderOptions::FfmpegH264 => {
            VideoDecoderThread::<ffmpeg_h264::FfmpegH264Decoder, _>::spawn(
                input_ref.clone(),
                options,
            )
            .map_err(MoqConnectionError::InitVideoDecoder)
        }
        VideoDecoderOptions::VulkanH264 => {
            VideoDecoderThread::<vulkan_h264::VulkanH264Decoder, _>::spawn(
                input_ref.clone(),
                options,
            )
            .map_err(MoqConnectionError::InitVideoDecoder)
        }
        _ => Err(MoqConnectionError::UnsupportedVideoCodec),
    }
}

fn process_audio_config(
    ctx: &Arc<PipelineCtx>,
    input_ref: &Ref<InputId>,
    audio: &DiscoveredAudio,
    samples_sender: QueueSender<InputAudioSamples>,
) -> Result<DecoderThreadHandle, MoqConnectionError> {
    match &audio.codec {
        AudioCodec::Aac => {
            let asc = audio
                .description
                .clone()
                .ok_or(MoqConnectionError::MissingAsc)?;
            let options = AudioDecoderThreadOptions {
                ctx: ctx.clone(),
                decoder_options: FdkAacDecoderOptions { asc: Some(asc) },
                samples_sender,
                input_buffer_size: MOQ_MAX_BUFFER,
            };
            AudioDecoderThread::<FdkAacDecoder>::spawn(input_ref.clone(), options)
                .map_err(MoqConnectionError::InitAudioDecoder)
        }
        AudioCodec::Opus => {
            let options = AudioDecoderThreadOptions {
                ctx: ctx.clone(),
                decoder_options: (),
                samples_sender,
                input_buffer_size: MOQ_MAX_BUFFER,
            };
            AudioDecoderThread::<OpusDecoder>::spawn(input_ref.clone(), options)
                .map_err(MoqConnectionError::InitAudioDecoder)
        }
    }
}

async fn run_video_track(
    video: DiscoveredVideo,
    decoder_handle: DecoderThreadHandle,
    broadcast: &BroadcastConsumer,
    first_pts: Arc<Mutex<Option<Duration>>>,
    stats_sender: MoqStatsSender,
) {
    match broadcast.subscribe_track(&Track::new(&video.name)) {
        Ok(track) => {
            // .with_latency() defines how long we wait for a stalled group. Group delay is a difference between
            // group start timestamp and highest received timestamp.
            let consumer = ContainerConsumer::new(track, video.container).with_latency(MOQ_BUFFER);
            if let Err(err) =
                read_video_track(consumer, decoder_handle, first_pts, stats_sender).await
            {
                warn!(
                    "MoQ video track error: {}",
                    ErrorStack::new(&err).into_string()
                );
            }
        }
        Err(err) => {
            warn!("Failed to subscribe to MoQ video track: {err}");
        }
    }
}

async fn run_audio_track(
    audio: DiscoveredAudio,
    decoder_handle: DecoderThreadHandle,
    broadcast: &BroadcastConsumer,
    first_pts: Arc<Mutex<Option<Duration>>>,
    stats_sender: MoqStatsSender,
) {
    match broadcast.subscribe_track(&Track::new(&audio.name)) {
        Ok(track) => {
            // .with_latency() defines how long we wait for a stalled group. Group delay is a difference between
            // group start timestamp and highest received timestamp.
            let consumer = ContainerConsumer::new(track, audio.container).with_latency(MOQ_BUFFER);
            if let Err(err) = read_audio_track(
                consumer,
                decoder_handle,
                audio.codec,
                first_pts,
                stats_sender,
            )
            .await
            {
                warn!(
                    "MoQ audio track error: {}",
                    ErrorStack::new(&err).into_string()
                );
            }
        }
        Err(err) => {
            warn!("Failed to subscribe to MoQ audio track: {err}");
        }
    }
}

#[derive(thiserror::Error, Debug)]
enum MoqConnectionError {
    #[error("MoQ track error")]
    TrackError(#[from] MoqError),

    #[error("MoQ catalog error: {0}")]
    CatalogError(#[from] MoqCatalogError),

    #[error("Failed to initialize H264 decoder")]
    InitVideoDecoder(#[source] DecoderInitError),

    #[error("Unsupported video codec, H264 expected.")]
    UnsupportedVideoCodec,

    #[error("Invalid H264 decoder config.")]
    InvalidAvcc,

    #[error("Failed to initialize audio decoder")]
    InitAudioDecoder(#[source] DecoderInitError),

    #[error("Missing AAC decoder config.")]
    MissingAsc,

    #[error("Decoder channel closed")]
    ChannelClosed,

    #[error("Container read error")]
    ContainerError(#[source] moq_mux::Error),
}

/// Normalizes a raw track timestamp against the first PTS observed across all
/// tracks of the broadcast, so audio and video share the same zero point.
fn normalize_pts(first_pts: &Arc<Mutex<Option<Duration>>>, raw_pts: Duration) -> Duration {
    let mut first_pts = first_pts.lock().unwrap();
    let first = *first_pts.get_or_insert(raw_pts);
    raw_pts.saturating_sub(first)
}

async fn read_video_track(
    mut consumer: ContainerConsumer<Container>,
    decoder_handle: DecoderThreadHandle,
    first_pts: Arc<Mutex<Option<Duration>>>,
    stats_sender: MoqStatsSender,
) -> Result<(), MoqConnectionError> {
    while let Some(frame) = consumer
        .read()
        .await
        .map_err(MoqConnectionError::ContainerError)?
    {
        stats_sender.bytes_received_event(frame.payload.len(), StatsTrackKind::Video);

        let raw_pts: Duration = frame.timestamp.into();
        let pts = normalize_pts(&first_pts, raw_pts);
        trace!(?pts, "MoQ video frame");
        let payload = frame.payload;

        let chunk = EncodedInputChunk {
            data: payload,
            pts,
            dts: None,
            kind: MediaKind::Video(VideoCodec::H264),
            present: true,
        };

        decoder_handle
            .chunk_sender
            .send(PipelineEvent::Data(chunk))
            .map_err(|_| MoqConnectionError::ChannelClosed)?;
    }

    Ok(())
}

async fn read_audio_track(
    mut consumer: ContainerConsumer<Container>,
    decoder_handle: DecoderThreadHandle,
    codec: AudioCodec,
    first_pts: Arc<Mutex<Option<Duration>>>,
    stats_sender: MoqStatsSender,
) -> Result<(), MoqConnectionError> {
    while let Some(frame) = consumer
        .read()
        .await
        .map_err(MoqConnectionError::ContainerError)?
    {
        stats_sender.bytes_received_event(frame.payload.len(), StatsTrackKind::Audio);

        let raw_pts: Duration = frame.timestamp.into();
        let pts = normalize_pts(&first_pts, raw_pts);
        trace!(?pts, "MoQ audio frame");
        let payload = frame.payload;

        let chunk = EncodedInputChunk {
            data: payload,
            pts,
            dts: None,
            kind: MediaKind::Audio(codec),
            present: true,
        };

        decoder_handle
            .chunk_sender
            .send(PipelineEvent::Data(chunk))
            .map_err(|_| MoqConnectionError::ChannelClosed)?;
    }

    Ok(())
}

#[derive(Clone)]
struct MoqStatsSender {
    input_ref: Ref<InputId>,
    stats_sender: StatsSender,
}

impl MoqStatsSender {
    fn new(input_ref: Ref<InputId>, stats_sender: StatsSender) -> Self {
        Self {
            input_ref,
            stats_sender,
        }
    }

    fn bytes_received_event(&self, size: usize, track_kind: StatsTrackKind) {
        self.stats_sender.send(
            MoqServerInputTrackStatsEvent::BytesReceived(size)
                .into_event(&self.input_ref, track_kind),
        );
    }
}
