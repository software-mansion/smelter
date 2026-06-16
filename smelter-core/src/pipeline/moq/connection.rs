use std::{
    sync::{Arc, Mutex, atomic::AtomicBool},
    time::Duration,
};

use bytes::Bytes;
use moq_mux::{catalog::hang::Container, container::Consumer as ContainerConsumer};
use moq_native::moq_net::{BroadcastConsumer, Error as MoqError, Track};
use smelter_render::error::ErrorStack;
use tracing::{info, trace, warn};

use crate::prelude::*;
use crate::queue::{QueueSender, QueueTrackOffset, QueueTrackOptions};
use crate::utils::{H264AvcDecoderConfig, H264AvccToAnnexB, InitializableThread};
use crate::{
    pipeline::{
        decoder::{
            DecoderThreadHandle,
            decoder_thread_audio::{AudioDecoderThread, AudioDecoderThreadOptions},
            decoder_thread_video::{VideoDecoderThread, VideoDecoderThreadOptions},
            fdk_aac::FdkAacDecoder,
            ffmpeg_h264, vulkan_h264,
        },
        moq::state::MoqInputState,
    },
    queue::QueueInput,
};

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
    let should_close = input.should_close.clone();

    let handle = rt.spawn(handle_broadcast(
        ctx,
        input_ref,
        decoders,
        queue_input,
        broadcast,
        should_close,
    ));

    Some(handle)
}

async fn handle_broadcast(
    ctx: Arc<PipelineCtx>,
    input_ref: Ref<InputId>,
    decoders: MoqServerInputDecoders,
    queue_input: QueueInput,
    broadcast: BroadcastConsumer,
    should_close: Arc<AtomicBool>,
) {
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
    let rt = ctx.tokio_rt.clone();

    let ctx_inner = ctx.clone();
    let input_ref_inner = input_ref.clone();
    let decoders_inner = decoders.clone();
    let broadcast_inner = broadcast.clone();
    let first_pts_inner = first_pts.clone();
    let should_close_inner = should_close.clone();
    let video_task = rt.spawn(async move {
        if let (Some(video), Some(frame_sender)) = (video, video_sender) {
            let video_result = run_video_track(
                ctx_inner,
                input_ref_inner,
                decoders_inner,
                video,
                frame_sender,
                broadcast_inner,
                first_pts_inner,
                should_close_inner,
            )
            .await;
            if let Err(error) = video_result {
                warn!(
                    "MoQ video track error: {}",
                    ErrorStack::new(&error).into_string(),
                );
            }
        }
    });

    let input_ref_inner = input_ref.clone();
    let audio_task = rt.spawn(async move {
        if let (Some(audio), Some(sample_sender)) = (audio, audio_sender) {
            let audio_result = run_audio_track(
                ctx,
                input_ref_inner,
                audio,
                sample_sender,
                broadcast,
                first_pts,
                should_close,
            )
            .await;
            if let Err(error) = audio_result {
                warn!(
                    "MoQ audio track error: {}",
                    ErrorStack::new(&error).into_string(),
                )
            }
        }
    });

    _ = video_task.await;
    _ = audio_task.await;
    info!(input_id = %input_ref, "MoQ broadcast connection closed");
}

fn spawn_video_decoder(
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

fn spawn_audio_decoder(
    ctx: &Arc<PipelineCtx>,
    input_ref: &Ref<InputId>,
    audio: &DiscoveredAudio,
    sample_sender: QueueSender<InputAudioSamples>,
) -> Result<DecoderThreadHandle, MoqConnectionError> {
    // Only AAC is allowed right now, different codecs are rejected before this function is called
    let asc = audio
        .description
        .clone()
        .ok_or(MoqConnectionError::MissingAsc)?;
    let aac_decoder_options = AudioDecoderOptions::FdkAac(FdkAacDecoderOptions { asc: Some(asc) });

    match aac_decoder_options {
        AudioDecoderOptions::FdkAac(decoder_options) => {
            let options = AudioDecoderThreadOptions {
                ctx: ctx.clone(),
                decoder_options,
                samples_sender: sample_sender,
                input_buffer_size: MOQ_MAX_BUFFER,
            };
            AudioDecoderThread::<FdkAacDecoder>::spawn(input_ref.clone(), options)
                .map_err(MoqConnectionError::InitAudioDecoder)
        }
        _ => Err(MoqConnectionError::UnsupportedAudioCodec),
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_video_track(
    ctx: Arc<PipelineCtx>,
    input_ref: Ref<InputId>,
    decoders: MoqServerInputDecoders,
    video: DiscoveredVideo,
    frame_sender: QueueSender<Frame>,
    broadcast: BroadcastConsumer,
    first_pts: Arc<Mutex<Option<Duration>>>,
    should_close: Arc<AtomicBool>,
) -> Result<(), MoqConnectionError> {
    let decoder_handle = spawn_video_decoder(&ctx, &input_ref, &decoders, &video, frame_sender)?;
    let mut consumer = match broadcast.subscribe_track(&Track::new(&video.name)) {
        Ok(track) => {
            // .with_latency() defines how long we wait for a stalled group. Group delay is a difference between
            // group start timestamp and highest received timestamp.
            ContainerConsumer::new(track, video.container).with_latency(MOQ_BUFFER)
        }
        Err(error) => return Err(error.into()),
    };

    while !should_close.load(std::sync::atomic::Ordering::Relaxed)
        && let Some(frame) = consumer
            .read()
            .await
            .map_err(MoqConnectionError::ContainerError)?
    {
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

async fn run_audio_track(
    ctx: Arc<PipelineCtx>,
    input_ref: Ref<InputId>,
    audio: DiscoveredAudio,
    sample_sender: QueueSender<InputAudioSamples>,
    broadcast: BroadcastConsumer,
    first_pts: Arc<Mutex<Option<Duration>>>,
    should_close: Arc<AtomicBool>,
) -> Result<(), MoqConnectionError> {
    let decoder_handle = spawn_audio_decoder(&ctx, &input_ref, &audio, sample_sender)?;
    let mut consumer = match broadcast.subscribe_track(&Track::new(&audio.name)) {
        Ok(track) => {
            // .with_latency() defines how long we wait for a stalled group. Group delay is a difference between
            // group start timestamp and highest received timestamp.
            ContainerConsumer::new(track, audio.container).with_latency(MOQ_BUFFER)
        }
        Err(error) => {
            return Err(error.into());
        }
    };

    while !should_close.load(std::sync::atomic::Ordering::Relaxed)
        && let Some(frame) = consumer
            .read()
            .await
            .map_err(MoqConnectionError::ContainerError)?
    {
        let raw_pts: Duration = frame.timestamp.into();
        let pts = normalize_pts(&first_pts, raw_pts);
        trace!(?pts, "MoQ audio frame");
        let payload = frame.payload;

        let chunk = EncodedInputChunk {
            data: payload,
            pts,
            dts: None,
            kind: MediaKind::Audio(AudioCodec::Aac),
            present: true,
        };

        decoder_handle
            .chunk_sender
            .send(PipelineEvent::Data(chunk))
            .map_err(|_| MoqConnectionError::ChannelClosed)?;
    }

    Ok(())
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

    #[error("Failed to initialize AAC decoder")]
    InitAudioDecoder(#[source] DecoderInitError),

    #[error("Unsupported audio codec, AAC expected.")]
    UnsupportedAudioCodec,

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
