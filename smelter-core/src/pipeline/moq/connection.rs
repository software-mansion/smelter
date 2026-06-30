use std::{
    sync::{Arc, Mutex, atomic::AtomicBool},
    time::Duration,
};

use bytes::Bytes;
use moq_mux::{catalog::hang::Container, container::Consumer as ContainerConsumer};
use moq_native::moq_net::{BroadcastConsumer, Error as MoqError, Track};
use smelter_render::error::ErrorStack;
use tracing::{Instrument, Level, Span, debug, info, span, trace, warn};

use crate::{
    pipeline::{
        decoder::{
            DecoderThreadHandle,
            decoder_thread_audio::{AudioDecoderThread, AudioDecoderThreadOptions},
            decoder_thread_video::{VideoDecoderThread, VideoDecoderThreadOptions},
            fdk_aac::FdkAacDecoder,
            ffmpeg_h264, ffmpeg_vp8, ffmpeg_vp9,
            libopus::OpusDecoder,
            vulkan_h264,
        },
        moq::state::MoqInputState,
    },
    queue::{QueueSender, QueueTrackOffset, QueueTrackOptions, WeakQueueInput},
    utils::{H264AvcDecoderConfig, H264AvccToAnnexB, InitializableThread},
};

use crate::prelude::*;

use self::catalog::{MoqCatalogError, read_catalog};

mod catalog;

const MOQ_BUFFER: Duration = Duration::from_secs(1);
const MOQ_MAX_BUFFER: Duration = Duration::from_secs(20);

struct VideoTrack {
    name: String,
    codec: VideoCodec,
    container: Container,
    description: Option<Bytes>,
}

struct AudioTrack {
    name: String,
    codec: AudioCodec,
    container: Container,
    description: Option<Bytes>,
}

#[derive(Clone)]
struct TrackCtx {
    ctx: Arc<PipelineCtx>,
    input_ref: Ref<InputId>,
    broadcast: BroadcastConsumer,
    decoders: MoqServerInputDecoders,
    first_pts: Arc<Mutex<Option<Duration>>>,
    should_close: Arc<AtomicBool>,
    stats_sender: MoqStatsSender,
}

pub(crate) fn start_broadcast_handler_task(
    ctx: Arc<PipelineCtx>,
    input_ref: &Ref<InputId>,
    input: &MoqInputState,
    broadcast: BroadcastConsumer,
) -> Option<tokio::task::JoinHandle<()>> {
    let queue_input = input.queue_input.clone();
    let input_ref = input_ref.clone();
    let decoders = input.decoders;
    let rt = ctx.tokio_rt.clone();
    let should_close = input.should_close.clone();

    let span = span!(
        Level::INFO,
        "MoQ server input",
        input_id = input_ref.to_string()
    );

    let handle = rt.spawn(
        async move {
            let broadcast_result = handle_broadcast(
                ctx,
                input_ref.clone(),
                decoders,
                queue_input,
                broadcast,
                should_close,
            )
            .await;
            if let Err(error) = broadcast_result {
                warn!(
                    "broadcast failed: {}",
                    ErrorStack::new(&error).into_string()
                );
            }
        }
        .instrument(span),
    );

    Some(handle)
}

async fn handle_broadcast(
    ctx: Arc<PipelineCtx>,
    input_ref: Ref<InputId>,
    decoders: MoqServerInputDecoders,
    queue_input: WeakQueueInput,
    broadcast: BroadcastConsumer,
    should_close: Arc<AtomicBool>,
) -> Result<(), MoqConnectionError> {
    info!("MoQ broadcast connection established");

    let (video, audio) = read_catalog(&broadcast).await?;

    let mut handler = BroadcastHandler::new(
        ctx.clone(),
        input_ref.clone(),
        broadcast,
        video,
        audio,
        decoders,
        should_close,
    );

    let (video_sender, audio_sender) = {
        let Some(queue_input) = queue_input.upgrade() else {
            return Err(MoqConnectionError::InputUnregistered);
        };
        // TODO: This has to be handled in a more reliable way that does not introduce high latency,
        // probably jitter buffer.
        queue_input.queue_new_track(QueueTrackOptions {
            video: handler.has_video(),
            audio: handler.has_audio(),
            offset: QueueTrackOffset::Pts(ctx.queue_ctx.effective_last_pts() + MOQ_BUFFER),
        })
    };

    let video_task = handler.handle_video_track(video_sender);
    let audio_task = handler.handle_audio_track(audio_sender);

    if let Some(video_task) = video_task {
        _ = video_task.await;
    };
    if let Some(audio_task) = audio_task {
        _ = audio_task.await;
    }
    info!("MoQ broadcast connection closed");
    Ok(())
}

struct BroadcastHandler {
    track_ctx: TrackCtx,
    video: Option<VideoTrack>,
    audio: Option<AudioTrack>,
}

impl BroadcastHandler {
    fn new(
        ctx: Arc<PipelineCtx>,
        input_ref: Ref<InputId>,
        broadcast: BroadcastConsumer,
        video: Option<VideoTrack>,
        audio: Option<AudioTrack>,
        decoders: MoqServerInputDecoders,
        should_close: Arc<AtomicBool>,
    ) -> Self {
        // Shared across audio and video so both tracks are normalized against
        // the same first PTS, preserving A/V synchronization. Whichever track
        // produces the first frame sets the common zero point for both.
        let first_pts = Arc::new(Mutex::new(None));

        let stats_sender = MoqStatsSender::new(input_ref.clone(), ctx.stats_sender.clone());

        let track_ctx = TrackCtx {
            ctx,
            input_ref,
            broadcast,
            decoders,
            first_pts,
            should_close,
            stats_sender,
        };
        Self {
            track_ctx,
            video,
            audio,
        }
    }

    fn has_video(&self) -> bool {
        self.video.is_some()
    }

    fn has_audio(&self) -> bool {
        self.audio.is_some()
    }

    fn handle_video_track(
        &mut self,
        frame_sender: Option<QueueSender<Frame>>,
    ) -> Option<tokio::task::JoinHandle<()>> {
        let (Some(video), Some(frame_sender)) = (self.video.take(), frame_sender) else {
            return None;
        };

        info!(track = %video.name, "Discovered MoQ video track");
        let ctx = self.track_ctx.clone();
        let handle = self.track_ctx.ctx.tokio_rt.spawn(
            async move {
                if let Err(error) = run_video_track(ctx, video, frame_sender).await {
                    warn!(
                        "MoQ video track error: {}",
                        ErrorStack::new(&error).into_string(),
                    )
                };
            }
            .instrument(Span::current()),
        );
        Some(handle)
    }

    fn handle_audio_track(
        &mut self,
        sample_sender: Option<QueueSender<InputAudioSamples>>,
    ) -> Option<tokio::task::JoinHandle<()>> {
        let (Some(audio), Some(sample_sender)) = (self.audio.take(), sample_sender) else {
            return None;
        };

        info!(track = %audio.name, "Discovered MoQ audio track");
        let ctx = self.track_ctx.clone();
        let handle = self.track_ctx.ctx.tokio_rt.spawn(
            async move {
                if let Err(error) = run_audio_track(ctx, audio, sample_sender).await {
                    warn!(
                        "MoQ audio track error: {}",
                        ErrorStack::new(&error).into_string(),
                    )
                };
            }
            .instrument(Span::current()),
        );
        Some(handle)
    }
}

async fn run_video_track(
    track_ctx: TrackCtx,
    video: VideoTrack,
    frame_sender: QueueSender<Frame>,
) -> Result<(), MoqConnectionError> {
    let TrackCtx {
        ctx,
        input_ref,
        broadcast,
        decoders,
        first_pts,
        should_close,
        stats_sender,
    } = track_ctx;

    let decoder_handle = spawn_video_decoder(&ctx, &input_ref, &decoders, &video, frame_sender)?;
    let track = broadcast.subscribe_track(&Track::new(&video.name))?;

    // .with_latency() defines how long we wait for a stalled group. Group delay is a difference between
    // group start timestamp and highest received timestamp.
    let mut consumer = ContainerConsumer::new(track, video.container).with_latency(MOQ_BUFFER);

    loop {
        if should_close.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        };
        let Some(frame) = consumer.read().await? else {
            break;
        };
        stats_sender.bytes_received_event(frame.payload.len(), StatsTrackKind::Video);

        let raw_pts: Duration = frame.timestamp.into();
        let pts = normalize_pts(&first_pts, raw_pts);
        trace!(?pts, "MoQ video frame");
        let payload = frame.payload;

        let chunk = EncodedInputChunk {
            data: payload,
            pts,
            dts: None,
            kind: MediaKind::Video(video.codec),
            present: true,
        };

        if decoder_handle
            .chunk_sender
            .send(PipelineEvent::Data(chunk))
            .is_err()
        {
            debug!("Failed to send chunk, channel closed.");
            break;
        }
    }
    if decoder_handle
        .chunk_sender
        .send(PipelineEvent::EOS)
        .is_err()
    {
        debug!("Failed to send EOS, channel closed.");
    }

    Ok(())
}

async fn run_audio_track(
    track_ctx: TrackCtx,
    audio: AudioTrack,
    sample_sender: QueueSender<InputAudioSamples>,
) -> Result<(), MoqConnectionError> {
    let TrackCtx {
        ctx,
        input_ref,
        broadcast,
        decoders: _,
        first_pts,
        should_close,
        stats_sender,
    } = track_ctx;

    let decoder_handle = spawn_audio_decoder(&ctx, &input_ref, &audio, sample_sender)?;
    let track = broadcast.subscribe_track(&Track::new(&audio.name))?;
    // .with_latency() defines how long we wait for a stalled group. Group delay is a difference between
    // group start timestamp and highest received timestamp.
    let mut consumer = ContainerConsumer::new(track, audio.container).with_latency(MOQ_BUFFER);

    loop {
        if should_close.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        };
        let Some(frame) = consumer.read().await? else {
            break;
        };
        stats_sender.bytes_received_event(frame.payload.len(), StatsTrackKind::Audio);

        let raw_pts: Duration = frame.timestamp.into();
        let pts = normalize_pts(&first_pts, raw_pts);
        trace!(?pts, "MoQ audio frame");
        let payload = frame.payload;

        let chunk = EncodedInputChunk {
            data: payload,
            pts,
            dts: None,
            kind: MediaKind::Audio(audio.codec),
            present: true,
        };

        if decoder_handle
            .chunk_sender
            .send(PipelineEvent::Data(chunk))
            .is_err()
        {
            debug!("Failed to send chunk, channel closed.");
            break;
        }
    }
    if decoder_handle
        .chunk_sender
        .send(PipelineEvent::EOS)
        .is_err()
    {
        debug!("Failed to send EOS, channel closed.");
    }

    Ok(())
}

fn spawn_video_decoder(
    ctx: &Arc<PipelineCtx>,
    input_ref: &Ref<InputId>,
    decoders: &MoqServerInputDecoders,
    video: &VideoTrack,
    frame_sender: QueueSender<Frame>,
) -> Result<DecoderThreadHandle, MoqConnectionError> {
    let transformer = match (&video.codec, &video.description) {
        (VideoCodec::H264, Some(desc)) => {
            let h264_config = H264AvcDecoderConfig::parse(desc.clone())
                .map_err(|_| MoqConnectionError::InvalidAvcc)?;

            Some(H264AvccToAnnexB::new(h264_config))
        }
        _ => None,
    };
    if let Container::Cmaf(_) = video.container
        && video.codec == VideoCodec::H264
        && transformer.is_none()
    {
        return Err(MoqConnectionError::MissingAvcc);
    }

    let options = VideoDecoderThreadOptions {
        ctx: ctx.clone(),
        transformer,
        frame_sender,
        input_buffer_size: MOQ_MAX_BUFFER,
    };

    let decoder_opt = match &video.codec {
        VideoCodec::H264 => decoders.h264.unwrap_or_else(|| {
            match ctx.graphics_context.has_vulkan_decoder_support() {
                true => VideoDecoderOptions::VulkanH264,
                false => VideoDecoderOptions::FfmpegH264,
            }
        }),
        VideoCodec::Vp8 => VideoDecoderOptions::FfmpegVp8,
        VideoCodec::Vp9 => VideoDecoderOptions::FfmpegVp9,
    };

    match decoder_opt {
        VideoDecoderOptions::FfmpegH264 => Ok(VideoDecoderThread::<
            ffmpeg_h264::FfmpegH264Decoder,
            _,
        >::spawn(input_ref.clone(), options)?),
        VideoDecoderOptions::VulkanH264 => Ok(VideoDecoderThread::<
            vulkan_h264::VulkanH264Decoder,
            _,
        >::spawn(input_ref.clone(), options)?),
        VideoDecoderOptions::FfmpegVp8 => Ok(
            VideoDecoderThread::<ffmpeg_vp8::FfmpegVp8Decoder, _>::spawn(
                input_ref.clone(),
                options,
            )?,
        ),
        VideoDecoderOptions::FfmpegVp9 => Ok(
            VideoDecoderThread::<ffmpeg_vp9::FfmpegVp9Decoder, _>::spawn(
                input_ref.clone(),
                options,
            )?,
        ),
    }
}

fn spawn_audio_decoder(
    ctx: &Arc<PipelineCtx>,
    input_ref: &Ref<InputId>,
    audio: &AudioTrack,
    sample_sender: QueueSender<InputAudioSamples>,
) -> Result<DecoderThreadHandle, MoqConnectionError> {
    match &audio.codec {
        AudioCodec::Aac => {
            let asc = audio.description.clone();
            if let Container::Cmaf(_) = audio.container
                && asc.is_none()
            {
                return Err(MoqConnectionError::MissingAsc);
            }

            let options = AudioDecoderThreadOptions {
                ctx: ctx.clone(),
                decoder_options: FdkAacDecoderOptions { asc },
                samples_sender: sample_sender,
                input_buffer_size: MOQ_MAX_BUFFER,
            };
            Ok(AudioDecoderThread::<FdkAacDecoder>::spawn(
                input_ref.clone(),
                options,
            )?)
        }
        AudioCodec::Opus => {
            let options = AudioDecoderThreadOptions {
                ctx: ctx.clone(),
                decoder_options: (),
                samples_sender: sample_sender,
                input_buffer_size: MOQ_MAX_BUFFER,
            };
            Ok(AudioDecoderThread::<OpusDecoder>::spawn(
                input_ref.clone(),
                options,
            )?)
        }
    }
}

#[derive(thiserror::Error, Debug)]
enum MoqConnectionError {
    #[error("MoQ track error")]
    TrackError(#[from] MoqError),

    #[error("MoQ catalog error: {0}")]
    CatalogError(#[from] MoqCatalogError),

    #[error("Failed to initialize decoder: {0}")]
    InitDecoder(#[from] DecoderInitError),

    #[error("Invalid H264 decoder config.")]
    InvalidAvcc,

    #[error("Missing H264 decoder config.")]
    MissingAvcc,

    #[error("Missing AAC decoder config.")]
    MissingAsc,

    #[error("Container read error")]
    ContainerError(#[from] moq_mux::Error),

    #[error("Input unregistered")]
    InputUnregistered,
}

/// Normalizes a raw track timestamp against the first PTS observed across all
/// tracks of the broadcast, so audio and video share the same zero point.
fn normalize_pts(first_pts: &Arc<Mutex<Option<Duration>>>, raw_pts: Duration) -> Duration {
    let mut first_pts = first_pts.lock().unwrap();
    let first = *first_pts.get_or_insert(raw_pts);
    raw_pts.saturating_sub(first)
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
