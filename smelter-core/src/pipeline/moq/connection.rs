use std::{
    collections::VecDeque,
    sync::Arc,
    time::{Duration, Instant},
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

const MOQ_LATENCY_TOLERANCE: Duration = Duration::from_millis(500);
const MOQ_JITTER_BUFFER_SIZE: Duration = Duration::from_millis(500);
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
        let input_id_str = input_ref.to_string();
        info!(input_id = %input_id_str, "MoQ broadcast connection established");

        let mut discovered = match read_catalog(&broadcast).await {
            Ok(d) => d,
            Err(err) => {
                warn!(
                    input_id = %input_id_str,
                    "MoQ catalog error: {}",
                    ErrorStack::new(&err).into_string()
                );
                return;
            }
        };

        let has_video = discovered.video.is_some();
        let has_audio = discovered.audio.is_some();

        let (video_sender, audio_sender) = queue_input.queue_new_track(QueueTrackOptions {
            video: has_video,
            audio: has_audio,
            offset: QueueTrackOffset::Pts(Duration::ZERO),
        });

        if let Some(v) = &discovered.video {
            info!(input_id = %input_id_str, track = %v.name, "Discovered MoQ video track");
        }
        if let Some(a) = &discovered.audio {
            info!(input_id = %input_id_str, track = %a.name, "Discovered MoQ audio track");
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
        let sync_point = ctx.queue_ctx.sync_point;

        let video_fut = run_video_track(video, video_decoder_handle, &broadcast, sync_point);

        let audio_fut = run_audio_track(audio, audio_decoder_handle, &broadcast, sync_point);

        tokio::join!(video_fut, audio_fut);
        info!(input_id = %input_id_str, "MoQ broadcast connection closed");
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
    sync_point: Instant,
) {
    if let Some(video) = video
        && let Some(decoder_handle) = decoder_handle
    {
        match broadcast.subscribe_track(&Track::new(&video.name)) {
            Ok(track) => {
                let consumer = ContainerConsumer::new(track, video.container)
                    .with_latency(MOQ_LATENCY_TOLERANCE);
                let jitter_buffer = MoqJitterBuffer::new(MOQ_JITTER_BUFFER_SIZE, sync_point);
                if let Err(err) = jitter_buffer
                    .run(consumer, decoder_handle, MediaKind::Video(VideoCodec::H264))
                    .await
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
}

async fn run_audio_track(
    audio: Option<DiscoveredAudio>,
    decoder_handle: Option<DecoderThreadHandle>,
    broadcast: &BroadcastConsumer,
    sync_point: Instant,
) {
    if let Some(audio) = audio
        && let Some(decoder_handle) = decoder_handle
    {
        match broadcast.subscribe_track(&Track::new(&audio.name)) {
            Ok(track) => {
                let consumer = ContainerConsumer::new(track, audio.container)
                    .with_latency(MOQ_LATENCY_TOLERANCE);
                let jitter_buffer = MoqJitterBuffer::new(MOQ_JITTER_BUFFER_SIZE, sync_point);
                if let Err(err) = jitter_buffer
                    .run(consumer, decoder_handle, MediaKind::Audio(AudioCodec::Aac))
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

struct BufferedFrame {
    payload: Bytes,
    raw_pts: Duration,
}

struct MoqJitterBuffer {
    buffer: VecDeque<BufferedFrame>,
    buffer_size: Duration,
    sync_point: Instant,
    first_pts: Option<Duration>,
    /// Wall-clock instant when releasing begins (set after fill phase)
    wall_anchor: Option<Instant>,
    /// `sync_point.elapsed()` captured at `wall_anchor` time
    anchor_pts: Option<Duration>,
}

impl MoqJitterBuffer {
    fn new(buffer_size: Duration, sync_point: Instant) -> Self {
        Self {
            buffer: VecDeque::new(),
            buffer_size,
            sync_point,
            first_pts: None,
            wall_anchor: None,
            anchor_pts: None,
        }
    }

    fn pts_span(&self) -> Duration {
        match (self.buffer.front(), self.buffer.back()) {
            (Some(first), Some(last)) => last.raw_pts.saturating_sub(first.raw_pts),
            _ => Duration::ZERO,
        }
    }

    fn output_pts(&self, raw_pts: Duration) -> Duration {
        let first = self.first_pts.unwrap_or(raw_pts);
        let normalized = raw_pts.saturating_sub(first);
        let base = self.anchor_pts.unwrap_or_else(|| self.sync_point.elapsed());
        base + normalized + self.buffer_size
    }

    fn release_time(&self, raw_pts: Duration) -> tokio::time::Instant {
        let first = self.first_pts.unwrap_or(raw_pts);
        let normalized = raw_pts.saturating_sub(first);
        let wall_anchor = self.wall_anchor.unwrap_or_else(Instant::now);
        tokio::time::Instant::from_std(wall_anchor + normalized)
    }

    async fn run(
        mut self,
        mut consumer: ContainerConsumer<Container>,
        decoder_handle: DecoderThreadHandle,
        media_kind: MediaKind,
    ) -> Result<(), MoqConnectionError> {
        // Fill phase: buffer frames until we have buffer_size worth of PTS span
        loop {
            let frame = consumer
                .read()
                .await
                .map_err(MoqConnectionError::ContainerError)?;

            let Some(frame) = frame else {
                return self.flush_remaining(&decoder_handle, media_kind);
            };

            let raw_pts: Duration = frame.timestamp.into();
            self.first_pts.get_or_insert(raw_pts);
            self.buffer.push_back(BufferedFrame {
                payload: frame.payload,
                raw_pts,
            });

            if self.pts_span() >= self.buffer_size {
                break;
            }
        }

        self.anchor_pts = Some(self.sync_point.elapsed());
        self.wall_anchor = Some(Instant::now());
        trace!(
            buffered_frames = self.buffer.len(),
            pts_span_ms = self.pts_span().as_millis(),
            "MoQ jitter buffer filled"
        );

        // Release phase: select loop
        loop {
            let sleep_until = self.buffer.front().map(|f| self.release_time(f.raw_pts));

            tokio::select! {
                result = consumer.read() => {
                    let frame = result.map_err(MoqConnectionError::ContainerError)?;
                    let Some(frame) = frame else {
                        return self.flush_remaining(&decoder_handle, media_kind);
                    };
                    let raw_pts: Duration = frame.timestamp.into();
                    self.buffer.push_back(BufferedFrame {
                        payload: frame.payload,
                        raw_pts,
                    });
                }

                _ = async {
                    match sleep_until {
                        Some(deadline) => tokio::time::sleep_until(deadline).await,
                        None => std::future::pending().await,
                    }
                } => {
                    if let Some(frame) = self.buffer.pop_front() {
                        self.send_frame(&decoder_handle, media_kind, frame)?;
                    }
                }
            }
        }
    }

    fn send_frame(
        &self,
        decoder_handle: &DecoderThreadHandle,
        media_kind: MediaKind,
        frame: BufferedFrame,
    ) -> Result<(), MoqConnectionError> {
        let pts = self.output_pts(frame.raw_pts);
        trace!(?pts, "MoQ jitter buffer release");

        let chunk = EncodedInputChunk {
            data: frame.payload,
            pts,
            dts: None,
            kind: media_kind,
            present: true,
        };

        decoder_handle
            .chunk_sender
            .send(PipelineEvent::Data(chunk))
            .map_err(|_| MoqConnectionError::ChannelClosed)
    }

    fn flush_remaining(
        &mut self,
        decoder_handle: &DecoderThreadHandle,
        media_kind: MediaKind,
    ) -> Result<(), MoqConnectionError> {
        while let Some(frame) = self.buffer.pop_front() {
            self.send_frame(decoder_handle, media_kind, frame)?;
        }
        Ok(())
    }
}
