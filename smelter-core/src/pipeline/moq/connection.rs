use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use bytes::Bytes;
use moq_mux::catalog::hang::Container;
use moq_mux::container::Consumer as ContainerConsumer;
use moq_native::moq_net::{BroadcastConsumer, Error as MoqError, Track};
use smelter_render::error::ErrorStack;
use tracing::{info, trace, warn};

use crate::pipeline::moq::connection::catalog::{MoqCatalogError, read_catalog};
use crate::utils::{H264AvcDecoderConfig, H264AvccToAnnexB};
use crate::{
    MediaKind, PipelineCtx, PipelineEvent,
    codecs::{
        AudioCodec, AudioDecoderOptions, FdkAacDecoderOptions, VideoCodec, VideoDecoderOptions,
    },
    error::DecoderInitError,
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
    queue::{QueueSender, QueueTrackOffset, QueueTrackOptions},
    utils::InitializableThread,
};

use crate::prelude::*;

mod catalog;

// This seems to be a safe value even for large groups
const MOQ_BUFFER: Duration = Duration::from_secs(5);
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

        let (video_decoder_handle, audio_decoder_handle) = spawn_decoders(
            &ctx,
            &input_ref,
            &decoders,
            &discovered,
            video_sender,
            audio_sender,
        );

        let video = discovered.video.take();
        let audio = discovered.audio.take();

        // Shared across audio and video so both tracks are normalized against
        // the same first PTS, preserving A/V synchronization. Whichever track
        // produces the first frame sets the common zero point for both.
        let first_pts = Arc::new(Mutex::new(None));

        let video_fut = run_video_track(video, video_decoder_handle, &broadcast, first_pts.clone());

        let audio_fut = run_audio_track(audio, audio_decoder_handle, &broadcast, first_pts);

        tokio::join!(video_fut, audio_fut);
        info!(input_id = %input_ref, "MoQ broadcast connection closed");
    });

    Some(handle)
}

fn spawn_decoders(
    ctx: &Arc<PipelineCtx>,
    input_ref: &Ref<InputId>,
    decoders: &MoqServerInputDecoders,
    discovered: &DiscoveredTracks,
    video_sender: Option<QueueSender<Frame>>,
    audio_sender: Option<QueueSender<InputAudioSamples>>,
) -> (Option<DecoderThreadHandle>, Option<DecoderThreadHandle>) {
    let video_decoder_handle = match (&discovered.video, video_sender) {
        (Some(video), Some(sender)) => {
            match process_video_config(ctx, input_ref, decoders, video, sender) {
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
        _ => None,
    };

    let audio_decoder_handle = match (&discovered.audio, audio_sender) {
        (Some(audio), Some(sender)) => match process_audio_config(ctx, input_ref, audio, sender) {
            Ok(handle) => Some(handle),
            Err(err) => {
                warn!(
                    "MoQ audio config error: {}",
                    ErrorStack::new(&err).into_string()
                );
                None
            }
        },
        _ => None,
    };
    (video_decoder_handle, audio_decoder_handle)
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
                samples_sender,
                input_buffer_size: MOQ_MAX_BUFFER,
            };
            AudioDecoderThread::<FdkAacDecoder>::spawn(input_ref.clone(), options)
                .map_err(MoqConnectionError::InitAudioDecoder)
        }
        _ => Err(MoqConnectionError::UnsupportedAudioCodec),
    }
}

async fn run_video_track(
    video: Option<DiscoveredVideo>,
    decoder_handle: Option<DecoderThreadHandle>,
    broadcast: &BroadcastConsumer,
    first_pts: Arc<Mutex<Option<Duration>>>,
) {
    if let Some(video) = video
        && let Some(decoder_handle) = decoder_handle
    {
        match broadcast.subscribe_track(&Track::new(&video.name)) {
            Ok(track) => {
                // The `.with_latency()` call sets the tolerated latency between groups
                // E.g.
                // - Group A starts at 200ms
                // - Group B starts at 400ms
                // - Group C starts at 600ms and its last timestamp is 790ms
                //
                // If `.with_latency()` is set to e.g. 150ms AND if during reading group A we stall at any moment (do not get frame on poll)
                // then we check 790ms - 150ms = 550ms. 550ms > 200ms so we skip group A ONLY and
                // proceed to the next one.
                let consumer =
                    ContainerConsumer::new(track, video.container).with_latency(MOQ_BUFFER);
                if let Err(err) = read_video_track(consumer, decoder_handle, first_pts).await {
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
}

async fn run_audio_track(
    audio: Option<DiscoveredAudio>,
    decoder_handle: Option<DecoderThreadHandle>,
    broadcast: &BroadcastConsumer,
    first_pts: Arc<Mutex<Option<Duration>>>,
) {
    if let Some(audio) = audio
        && let Some(decoder_handle) = decoder_handle
    {
        match broadcast.subscribe_track(&Track::new(&audio.name)) {
            Ok(track) => {
                // The `.with_latency()` call sets the tolerated latency between groups
                // E.g.
                // - Group A starts at 200ms
                // - Group B starts at 400ms
                // - Group C starts at 600ms and its last timestamp is 790ms
                //
                // If `.with_latency()` is set to e.g. 150ms AND if during reading group A we stall at any moment (do not get frame on poll)
                // then we check 790ms - 150ms = 550ms. 550ms > 200ms so we skip group A ONLY and
                // proceed to the next one.
                let consumer =
                    ContainerConsumer::new(track, audio.container).with_latency(MOQ_BUFFER);
                if let Err(err) = read_audio_track(consumer, decoder_handle, first_pts).await {
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
fn normalize_pts(first_pts: &Mutex<Option<Duration>>, raw_pts: Duration) -> Duration {
    let mut first_pts = first_pts.lock().unwrap();
    let first = *first_pts.get_or_insert(raw_pts);
    raw_pts.saturating_sub(first)
}

async fn read_video_track(
    mut consumer: ContainerConsumer<Container>,
    decoder_handle: DecoderThreadHandle,
    first_pts: Arc<Mutex<Option<Duration>>>,
) -> Result<(), MoqConnectionError> {
    while let Some(frame) = consumer
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

async fn read_audio_track(
    mut consumer: ContainerConsumer<Container>,
    decoder_handle: DecoderThreadHandle,
    first_pts: Arc<Mutex<Option<Duration>>>,
) -> Result<(), MoqConnectionError> {
    while let Some(frame) = consumer
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
