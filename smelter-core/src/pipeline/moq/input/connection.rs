use std::{
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};

use bytes::Bytes;
use moq_mux::{catalog::hang::Container, container::Consumer as ContainerConsumer};
use moq_native::moq_net::{BroadcastConsumer, Error as MoqError, Track};
use smelter_render::error::ErrorStack;
use tracing::{Instrument, Span, debug, info, trace, warn};

use crate::{
    pipeline::{
        decoder::{
            DecoderThreadHandle, EncodedInputEvent,
            decoder_thread_audio::{AudioDecoderThread, AudioDecoderThreadOptions},
            decoder_thread_video::{VideoDecoderThread, VideoDecoderThreadOptions},
            fdk_aac::FdkAacDecoder,
            ffmpeg_h264::FfmpegH264Decoder,
            ffmpeg_vp8::FfmpegVp8Decoder,
            ffmpeg_vp9::FfmpegVp9Decoder,
            libopus::OpusDecoder,
            vulkan_h264::VulkanH264Decoder,
        },
        rtmp::rtmp_input::buffer::resolve_buffer_options,
    },
    queue::{QueueSender, QueueTrackOffset, QueueTrackOptions, WeakQueueInput},
    utils::{
        H264AvcDecoderConfig, H264AvccToAnnexB, InitializableThread,
        channel::TrySendError,
        input_sync::{
            InputSyncItem, TimestampAnchor, TrackClosedError, TrackEvent, TrackKind, TrackSink,
        },
        live_sync::{BufferingStrategy, FifoBuffer, LiveSync, LiveSyncOptions, LiveSyncTrack},
    },
};

use crate::prelude::*;

use self::catalog::{MoqCatalogError, read_catalog};

mod catalog;

/// Chunk with the keyframe flag the container consumer reports, so a track
/// can resume on a keyframe after a discontinuity or a dropped chunk.
struct MoqChunk {
    chunk: EncodedInputChunk,
    keyframe: bool,
}

impl InputSyncItem for MoqChunk {
    fn pts(&self) -> Duration {
        self.chunk.pts
    }

    fn apply_anchor(&mut self, anchor: TimestampAnchor) {
        self.chunk.apply_anchor(anchor)
    }
}

type MoqBuffer = FifoBuffer<MoqChunk>;

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
    decoders: MoqInputDecoders,
    input_sync: Arc<LiveSync<MoqBuffer>>,
    /// How long the container consumer waits for a stalled group.
    group_latency: Duration,
    decoder_buffer_size: Duration,
    should_close: Arc<AtomicBool>,
    stats_sender: MoqStatsSender,
}

pub(crate) struct BroadcastCtx {
    pub broadcast: BroadcastConsumer,
    pub decoders: MoqInputDecoders,
    pub buffer: LiveInputBufferOptions,
    pub should_close: Arc<AtomicBool>,
    pub endpoint_kind: MoqEndpointKind,
}

pub(crate) async fn handle_broadcast(
    ctx: Arc<PipelineCtx>,
    input_ref: Ref<InputId>,
    queue_input: WeakQueueInput,
    broadcast_ctx: BroadcastCtx,
) -> Result<(), MoqConnectionError> {
    info!("MoQ broadcast connection established");

    let (video, audio) = read_catalog(&broadcast_ctx.broadcast).await?;

    let mut handler =
        BroadcastHandler::new(ctx.clone(), input_ref.clone(), video, audio, broadcast_ctx);

    let (video_sender, audio_sender) = {
        let Some(queue_input) = queue_input.upgrade() else {
            return Err(MoqConnectionError::InputUnregistered);
        };
        queue_input.queue_new_track(QueueTrackOptions {
            video: handler.has_video(),
            audio: handler.has_audio(),
            offset: QueueTrackOffset::Pts(Duration::ZERO),
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
    handler.track_ctx.input_sync.flush();
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
        video: Option<VideoTrack>,
        audio: Option<AudioTrack>,
        broadcast_ctx: BroadcastCtx,
    ) -> Self {
        let BroadcastCtx {
            broadcast,
            decoders,
            buffer,
            should_close,
            endpoint_kind,
        } = broadcast_ctx;

        let (min, desired, max) = resolve_buffer_options(buffer);
        let input_sync = Arc::new(LiveSync::new(
            LiveSyncOptions {
                buffering_strategy: BufferingStrategy::Range { min, max, desired },
                stabilization_period: Duration::from_millis(500),
                stabilization_tolerance: Duration::from_millis(100),
                max_wait: desired * 2,
            },
            ctx.queue_ctx.sync_point,
        ));
        let decoder_buffer_size = Duration::max(Duration::from_secs(60), max * 2);

        let stats_sender =
            MoqStatsSender::new(input_ref.clone(), ctx.stats_sender.clone(), endpoint_kind);

        let track_ctx = TrackCtx {
            ctx,
            input_ref,
            broadcast,
            decoders,
            input_sync,
            group_latency: desired,
            decoder_buffer_size,
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
        input_sync,
        group_latency,
        decoder_buffer_size,
        should_close,
        stats_sender,
    } = track_ctx;

    let decoder_handle = spawn_video_decoder(
        &ctx,
        &input_ref,
        &decoders,
        &video,
        frame_sender,
        decoder_buffer_size,
    )?;
    let track = broadcast.subscribe_track(&Track::new(&video.name))?;

    // .with_latency() defines how long we wait for a stalled group. Group delay is a difference between
    // group start timestamp and highest received timestamp.
    let mut consumer = ContainerConsumer::new(track, video.container).with_latency(group_latency);

    let sink = MoqTrackSink::new(decoder_handle, ctx.queue_ctx.sync_point);
    let mut track_sync = input_sync.add_track(TrackKind::Video, Box::new(sink));

    loop {
        if should_close.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        };
        let Some(frame) = consumer.read().await? else {
            break;
        };
        stats_sender.bytes_received_event(frame.payload.len(), StatsTrackKind::Video);

        trace!(pts=?frame.timestamp, "Video chunk received.");
        let chunk = EncodedInputChunk {
            data: frame.payload,
            pts: frame.timestamp.into(),
            dts: None,
            kind: MediaKind::Video(video.codec),
            decode_only: false,
        };
        if write_chunk(&mut track_sync, chunk, frame.keyframe).is_err() {
            debug!("Failed to send video chunk, channel closed.");
            break;
        }
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
        input_sync,
        group_latency,
        decoder_buffer_size,
        should_close,
        stats_sender,
    } = track_ctx;

    let decoder_handle =
        spawn_audio_decoder(&ctx, &input_ref, &audio, sample_sender, decoder_buffer_size)?;
    let track = broadcast.subscribe_track(&Track::new(&audio.name))?;

    // .with_latency() defines how long we wait for a stalled group. Group delay is a difference between
    // group start timestamp and highest received timestamp.
    let mut consumer = ContainerConsumer::new(track, audio.container).with_latency(group_latency);

    let sink = MoqTrackSink::new(decoder_handle, ctx.queue_ctx.sync_point);
    let mut track_sync = input_sync.add_track(TrackKind::Audio, Box::new(sink));

    loop {
        if should_close.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        };
        let Some(frame) = consumer.read().await? else {
            break;
        };
        stats_sender.bytes_received_event(frame.payload.len(), StatsTrackKind::Audio);

        trace!(pts=?frame.timestamp, "Audio chunk received.");
        let chunk = EncodedInputChunk {
            data: frame.payload,
            pts: frame.timestamp.into(),
            dts: None,
            kind: MediaKind::Audio(audio.codec),
            decode_only: false,
        };
        if write_chunk(&mut track_sync, chunk, frame.keyframe).is_err() {
            debug!("Failed to send audio chunk, channel closed.");
            break;
        }
    }

    Ok(())
}

fn write_chunk(
    track_sync: &mut LiveSyncTrack<MoqBuffer>,
    chunk: EncodedInputChunk,
    keyframe: bool,
) -> Result<(), TrackClosedError> {
    track_sync.write_chunk(MoqChunk { chunk, keyframe })
}

/// Forwards the chunks of a track to its decoder thread. Chunks the channel
/// cannot take are dropped, and decoding resumes on the next keyframe.
struct MoqTrackSink {
    decoder_handle: DecoderThreadHandle,
    waiting_for_keyframe: bool,
    pending_discontinuity: bool,
    /// Set once the decoder side is gone.
    closed: bool,
    /// Used to calculate stats
    sync_point: Instant,
}

impl MoqTrackSink {
    fn new(decoder_handle: DecoderThreadHandle, sync_point: Instant) -> Self {
        Self {
            decoder_handle,
            waiting_for_keyframe: false,
            pending_discontinuity: false,
            closed: false,
            sync_point,
        }
    }

    fn maybe_handle_discontinuity(&mut self) {
        if !self.pending_discontinuity {
            return;
        }
        self.pending_discontinuity = !self.send_to_decoder(EncodedInputEvent::Discontinuity);
    }

    /// Pushes an event to the decoder thread; `false` when it did not fit or
    /// the decoder is gone.
    fn send_to_decoder(&mut self, event: EncodedInputEvent) -> bool {
        match self
            .decoder_handle
            .chunk_sender
            .try_send(PipelineEvent::Data(event))
        {
            Ok(()) => true,
            Err(TrySendError::Full(_)) => {
                debug!("Dropping chunk; decoder is not keeping up");
                self.waiting_for_keyframe = true;
                false
            }
            Err(TrySendError::Disconnected(_)) => {
                self.closed = true;
                false
            }
        }
    }
}

impl TrackSink<MoqChunk> for MoqTrackSink {
    fn on_event(&mut self, event: TrackEvent<MoqChunk>) {
        let MoqChunk { chunk, keyframe } = match event {
            TrackEvent::Chunk(chunk) => chunk,
            TrackEvent::Discontinuity => {
                self.pending_discontinuity = true;
                self.waiting_for_keyframe = true;
                self.maybe_handle_discontinuity();
                return;
            }
        };

        self.maybe_handle_discontinuity();
        if self.pending_discontinuity {
            return;
        }
        if self.waiting_for_keyframe {
            if !keyframe {
                debug!("Waiting for keyframe");
                return;
            }
            self.waiting_for_keyframe = false;
        }
        trace!(
            pts = ?chunk.pts,
            buffered = ?chunk.pts.saturating_sub(self.sync_point.elapsed()),
            "Chunk released"
        );
        self.send_to_decoder(EncodedInputEvent::Chunk(chunk));
    }

    fn is_closed(&self) -> bool {
        self.closed
    }
}

fn spawn_video_decoder(
    ctx: &Arc<PipelineCtx>,
    input_ref: &Ref<InputId>,
    decoders: &MoqInputDecoders,
    video: &VideoTrack,
    frame_sender: QueueSender<Frame>,
    buffer_size: Duration,
) -> Result<DecoderThreadHandle, MoqConnectionError> {
    let handle = match &video.codec {
        VideoCodec::H264 => {
            spawn_h264_video_decoder(ctx, input_ref, decoders, video, frame_sender, buffer_size)?
        }
        VideoCodec::Vp8 => VideoDecoderThread::<FfmpegVp8Decoder, _>::spawn(
            input_ref.clone(),
            VideoDecoderThreadOptions::<H264AvccToAnnexB> {
                ctx: ctx.clone(),
                transformer: None,
                frame_sender,
                input_buffer_size: buffer_size,
            },
        )?,
        VideoCodec::Vp9 => VideoDecoderThread::<FfmpegVp9Decoder, _>::spawn(
            input_ref.clone(),
            VideoDecoderThreadOptions::<H264AvccToAnnexB> {
                ctx: ctx.clone(),
                transformer: None,
                frame_sender,
                input_buffer_size: buffer_size,
            },
        )?,
    };
    Ok(handle)
}

fn spawn_h264_video_decoder(
    ctx: &Arc<PipelineCtx>,
    input_ref: &Ref<InputId>,
    decoders: &MoqInputDecoders,
    video: &VideoTrack,
    frame_sender: QueueSender<Frame>,
    buffer_size: Duration,
) -> Result<DecoderThreadHandle, MoqConnectionError> {
    let config = match &video.description {
        Some(desc) => Some(H264AvcDecoderConfig::parse(desc.clone())?),
        None => match &video.container {
            Container::Cmaf(_) => return Err(MoqConnectionError::MissingAvcc),
            _ => None,
        },
    };

    let options = VideoDecoderThreadOptions {
        ctx: ctx.clone(),
        transformer: config.map(H264AvccToAnnexB::new),
        frame_sender,
        input_buffer_size: buffer_size,
    };

    let default_decoder = match ctx.graphics_context.has_vulkan_decoder_support() {
        true => VideoDecoderOptions::VulkanH264,
        false => VideoDecoderOptions::FfmpegH264,
    };
    let handle = match decoders.h264.unwrap_or(default_decoder) {
        VideoDecoderOptions::VulkanH264 => {
            VideoDecoderThread::<VulkanH264Decoder, _>::spawn(input_ref.clone(), options)?
        }
        _ => VideoDecoderThread::<FfmpegH264Decoder, _>::spawn(input_ref.clone(), options)?,
    };
    Ok(handle)
}

fn spawn_audio_decoder(
    ctx: &Arc<PipelineCtx>,
    input_ref: &Ref<InputId>,
    audio: &AudioTrack,
    sample_sender: QueueSender<InputAudioSamples>,
    buffer_size: Duration,
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
                input_buffer_size: buffer_size,
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
                input_buffer_size: buffer_size,
            };
            Ok(AudioDecoderThread::<OpusDecoder>::spawn(
                input_ref.clone(),
                options,
            )?)
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum MoqConnectionError {
    #[error("MoQ track error")]
    TrackError(#[from] MoqError),

    #[error("MoQ catalog error: {0}")]
    CatalogError(#[from] MoqCatalogError),

    #[error("Failed to initialize decoder: {0}")]
    InitDecoder(#[from] DecoderInitError),

    #[error("Invalid H264 decoder config.")]
    InvalidAvcc(#[from] H264AvcDecoderConfigError),

    #[error("Missing H264 decoder config.")]
    MissingAvcc,

    #[error("Missing AAC decoder config.")]
    MissingAsc,

    #[error("Container read error")]
    ContainerError(#[from] moq_mux::Error),

    #[error("Input unregistered")]
    InputUnregistered,
}

#[derive(Clone)]
pub(crate) enum MoqEndpointKind {
    Server,
    Client,
}

#[derive(Clone)]
struct MoqStatsSender {
    input_ref: Ref<InputId>,
    stats_sender: StatsSender,
    endpoint_kind: MoqEndpointKind,
}

impl MoqStatsSender {
    fn new(
        input_ref: Ref<InputId>,
        stats_sender: StatsSender,
        endpoint_kind: MoqEndpointKind,
    ) -> Self {
        Self {
            input_ref,
            stats_sender,
            endpoint_kind,
        }
    }

    fn bytes_received_event(&self, size: usize, track_kind: StatsTrackKind) {
        let event = match self.endpoint_kind {
            MoqEndpointKind::Server => MoqServerInputTrackStatsEvent::BytesReceived(size)
                .into_event(&self.input_ref, track_kind),
            MoqEndpointKind::Client => MoqClientInputTrackStatsEvent::BytesReceived(size)
                .into_event(&self.input_ref, track_kind),
        };
        self.stats_sender.send(event);
    }
}
