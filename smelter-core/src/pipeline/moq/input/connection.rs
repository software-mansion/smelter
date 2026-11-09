use std::{
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};

use bytes::Bytes;
use hang::catalog::{AudioConfig, VideoConfig};
use moq_mux::catalog::hang::Container;
use moq_net::{Error as MoqError, broadcast, track::Subscription};
use smelter_render::error::ErrorStack;
use tokio::task::JoinHandle;
use tracing::{Instrument, Span, debug, info, trace, warn};

use crate::{
    pipeline::decoder::{
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
    queue::{IndependentTrackGroup, QueueSender, QueueTrackGroupOffset, WeakQueueInput},
    utils::{
        H264AvcDecoderConfig, H264AvccToAnnexB, InitializableThread,
        channel::{Sender, TrySendError},
        input_sync::{InputSyncStatsSender, TrackEvent, TrackKind, TrackSink},
        live_sync::{BufferingStrategy, LiveSync, LiveSyncOptions},
    },
};

use crate::prelude::*;

use super::{
    buffer::{MoqChunk, MoqJitterBuffer, resolve_buffer_options},
    catalog::{MoqCatalogError, MoqCatalogReader},
    track_reader::{LateGroupOptions, MoqTrackReader},
};

pub(super) struct VideoTrack {
    pub name: String,
    pub codec: VideoCodec,
    pub container: Container,
    pub description: Option<Bytes>,
    /// Catalog entry the track was built from, used to detect catalog changes.
    pub config: VideoConfig,
}

pub(super) struct AudioTrack {
    pub name: String,
    pub codec: AudioCodec,
    pub container: Container,
    pub description: Option<Bytes>,
    /// Catalog entry the track was built from, used to detect catalog changes.
    pub config: AudioConfig,
}

#[derive(Clone)]
struct TrackCtx {
    ctx: Arc<PipelineCtx>,
    input_ref: Ref<InputId>,
    broadcast: broadcast::Consumer,
    decoders: MoqInputDecoders,
    input_sync: Arc<LiveSync<MoqJitterBuffer>>,
    late_groups: LateGroupOptions,
    decoder_buffer_size: Duration,
    should_close: Arc<AtomicBool>,
}

pub(crate) struct BroadcastCtx {
    pub broadcast: broadcast::Consumer,
    pub decoder_options: MoqInputDecoders,
    pub buffer: LiveInputBufferOptions,
    pub should_close: Arc<AtomicBool>,
}

pub(crate) async fn handle_broadcast(
    ctx: Arc<PipelineCtx>,
    input_ref: Ref<InputId>,
    queue_input: WeakQueueInput,
    broadcast_ctx: BroadcastCtx,
) -> Result<(), MoqConnectionError> {
    info!("MoQ broadcast connection established");

    let catalog = MoqCatalogReader::new(&broadcast_ctx.broadcast).await?;

    let track_group = {
        let Some(queue_input) = queue_input.upgrade() else {
            return Err(MoqConnectionError::InputUnregistered);
        };
        queue_input.queue_new_independent_track_group(QueueTrackGroupOffset::Pts(Timestamp::ZERO))
    };
    let mut handler = BroadcastHandler::new(ctx, input_ref, track_group, broadcast_ctx);

    let result = handler.run(catalog).await;
    handler.stop_tracks().await;
    info!("MoQ broadcast connection closed");
    result
}

/// Track that runs until it ends on its own or the catalog no longer selects it.
struct RunningTrack<C> {
    name: String,
    config: C,
    task: JoinHandle<()>,
}

impl<C: PartialEq> RunningTrack<C> {
    /// Whether the track still matches the catalog selection `(name, config)`.
    fn is_current(running: &Option<Self>, selected: Option<(&String, &C)>) -> bool {
        match (running, selected) {
            (Some(running), Some((name, config))) => {
                running.name == *name && running.config == *config && !running.task.is_finished()
            }
            (None, None) => true,
            _ => false,
        }
    }
}

struct BroadcastHandler {
    track_ctx: TrackCtx,
    track_group: IndependentTrackGroup,
    video: Option<RunningTrack<VideoConfig>>,
    audio: Option<RunningTrack<AudioConfig>>,
}

impl BroadcastHandler {
    fn new(
        ctx: Arc<PipelineCtx>,
        input_ref: Ref<InputId>,
        track_group: IndependentTrackGroup,
        broadcast_ctx: BroadcastCtx,
    ) -> Self {
        let BroadcastCtx {
            broadcast,
            decoder_options: decoders,
            buffer,
            should_close,
        } = broadcast_ctx;

        let (min, desired, max) = resolve_buffer_options(buffer);
        let sync_stats = InputSyncStatsSender::new(&input_ref, &ctx.stats_sender);
        let input_sync = Arc::new(LiveSync::new(
            LiveSyncOptions {
                buffering_strategy: BufferingStrategy::Range { min, max, desired },
                stabilization_period: Duration::from_millis(500),
                stabilization_tolerance: Duration::from_millis(100),
                max_wait: desired * 2,
                stale_estimate_threshold: Duration::from_millis(500),
            },
            ctx.queue_ctx.sync_point,
            sync_stats,
        ));
        let decoder_buffer_size = Duration::max(Duration::from_secs(60), max * 2);

        let track_ctx = TrackCtx {
            ctx,
            input_ref,
            broadcast,
            decoders,
            input_sync,
            late_groups: LateGroupOptions {
                grace: Duration::min(Duration::from_secs(1), desired),
                max_behind: max,
            },
            decoder_buffer_size,
            should_close,
        };
        Self {
            track_ctx,
            track_group,
            video: None,
            audio: None,
        }
    }

    /// Updates tracks on every catalog snapshot until the catalog track ends together with the
    /// broadcast. Restarts tracks whose catalog entry changed or that ended, stops tracks removed
    /// from the catalog and starts the new ones. A new track starts in the queue after the
    /// previous track of the same kind drained.
    async fn run(&mut self, mut catalog: MoqCatalogReader) -> Result<(), MoqConnectionError> {
        while let Some(tracks) = catalog.next().await {
            let (video, audio) = tracks?;

            let selected_video = video.as_ref().map(|video| (&video.name, &video.config));
            if !RunningTrack::is_current(&self.video, selected_video) {
                self.stop_track(TrackKind::Video).await;
                if let Some(video) = video {
                    self.start_video_track_task(video)?;
                }
            }

            let selected_audio = audio.as_ref().map(|audio| (&audio.name, &audio.config));
            if !RunningTrack::is_current(&self.audio, selected_audio) {
                self.stop_track(TrackKind::Audio).await;
                if let Some(audio) = audio {
                    self.start_audio_track_task(audio)?;
                }
            }
        }
        Ok(())
    }

    fn start_video_track_task(&mut self, video: VideoTrack) -> Result<(), MoqConnectionError> {
        let frame_sender = self
            .track_group
            .add_video_track()
            .map_err(|_| MoqConnectionError::InputUnregistered)?;

        info!(track = %video.name, "Starting MoQ video track");
        let name = video.name.clone();
        let config = video.config.clone();
        let ctx = self.track_ctx.clone();
        let task = self.track_ctx.ctx.tokio_rt.spawn(
            async move {
                let input_sync = ctx.input_sync.clone();
                if let Err(error) = run_video_track_task(ctx, video, frame_sender).await {
                    warn!(
                        "MoQ video track error: {}",
                        ErrorStack::new(&error).into_string(),
                    )
                };
                input_sync.remove_track(TrackKind::Video);
            }
            .instrument(Span::current()),
        );
        self.video = Some(RunningTrack { name, config, task });
        Ok(())
    }

    fn start_audio_track_task(&mut self, audio: AudioTrack) -> Result<(), MoqConnectionError> {
        let sample_sender = self
            .track_group
            .add_audio_track()
            .map_err(|_| MoqConnectionError::InputUnregistered)?;

        info!(track = %audio.name, "Starting MoQ audio track");
        let name = audio.name.clone();
        let config = audio.config.clone();
        let ctx = self.track_ctx.clone();
        let task = self.track_ctx.ctx.tokio_rt.spawn(
            async move {
                let input_sync = ctx.input_sync.clone();
                if let Err(error) = run_audio_track_task(ctx, audio, sample_sender).await {
                    warn!(
                        "MoQ audio track error: {}",
                        ErrorStack::new(&error).into_string(),
                    )
                };
                input_sync.remove_track(TrackKind::Audio);
            }
            .instrument(Span::current()),
        );
        self.audio = Some(RunningTrack { name, config, task });
        Ok(())
    }

    /// Stops the track and drops its live sync track, so the decoder drains and the queue gets
    /// EOS.
    async fn stop_track(&mut self, kind: TrackKind) {
        let task = match kind {
            TrackKind::Video => self.video.take().map(|track| track.task),
            TrackKind::Audio => self.audio.take().map(|track| track.task),
        };
        let Some(task) = task else {
            return;
        };
        task.abort();
        _ = task.await;
        self.track_ctx.input_sync.remove_track(kind);
    }

    async fn stop_tracks(&mut self) {
        self.stop_track(TrackKind::Video).await;
        self.stop_track(TrackKind::Audio).await;
    }
}

/// Cancel safety: `BroadcastHandler::stop_track` aborts the task, which drops this future at
/// whichever `.await` it is waiting on. That relies on two things:
/// - moq-net futures (`subscribe`, reads in `MoqTrackReader`) can be dropped mid-await without
///   leaving the session or track in a broken state. moq-net is built around cancel-by-drop,
///   but does not document this per method.
/// - State changes that belong together (decoder spawn + `add_track`, `write_chunk`) are not
///   split by an `.await`. Cleanup that always has to run (`remove_track`) is done by
///   `stop_track` after the task ended, not after an `.await` here.
async fn run_video_track_task(
    track_ctx: TrackCtx,
    video: VideoTrack,
    frame_sender: QueueSender<Frame>,
) -> Result<(), MoqConnectionError> {
    let decoder_handle = spawn_video_decoder(
        &track_ctx.ctx,
        &track_ctx.input_ref,
        &track_ctx.decoders,
        &video,
        frame_sender,
        track_ctx.decoder_buffer_size,
    )?;
    let sink = MoqTrackSink::new(decoder_handle, track_ctx.ctx.queue_ctx.sync_point);
    let mut track_sync = track_ctx
        .input_sync
        .add_track(TrackKind::Video, Box::new(sink));

    // Groups are kept as long as the reader would keep them, so its late group handling decides
    // what is dropped.
    let subscription = Subscription::default().with_max_age(track_ctx.late_groups.max_behind);
    let track = track_ctx
        .broadcast
        .track(&video.name)?
        .subscribe(subscription)
        .await?;
    let mut reader = MoqTrackReader::new(
        track,
        video.container,
        MediaKind::Video(video.codec),
        track_sync.deadline(),
        track_ctx.late_groups,
    );

    loop {
        if track_ctx
            .should_close
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            break;
        };
        let Some(chunk) = reader.read().await? else {
            break;
        };

        trace!(pts = ?chunk.chunk.pts, ?chunk, "Video chunk received.");
        if track_sync.write_chunk(chunk).is_err() {
            debug!("Failed to send video chunk, channel closed.");
            break;
        }
    }

    Ok(())
}

/// Cancel safety: `BroadcastHandler::stop_track` aborts the task, which drops this future at
/// whichever `.await` it is waiting on. That relies on two things:
/// - moq-net futures (`subscribe`, reads in `MoqTrackReader`) can be dropped mid-await without
///   leaving the session or track in a broken state. moq-net is built around cancel-by-drop,
///   but does not document this per method.
/// - State changes that belong together (decoder spawn + `add_track`, `write_chunk`) are not
///   split by an `.await`. Cleanup that always has to run (`remove_track`) is done by
///   `stop_track` after the task ended, not after an `.await` here.
async fn run_audio_track_task(
    track_ctx: TrackCtx,
    audio: AudioTrack,
    sample_sender: QueueSender<InputAudioSamples>,
) -> Result<(), MoqConnectionError> {
    let decoder_handle = spawn_audio_decoder(
        &track_ctx.ctx,
        &track_ctx.input_ref,
        &audio,
        sample_sender,
        track_ctx.decoder_buffer_size,
    )?;
    let sink = MoqTrackSink::new(decoder_handle, track_ctx.ctx.queue_ctx.sync_point);
    let mut track_sync = track_ctx
        .input_sync
        .add_track(TrackKind::Audio, Box::new(sink));

    // Groups are kept as long as the reader would keep them, so its late group handling decides
    // what is dropped.
    let subscription = Subscription::default().with_max_age(track_ctx.late_groups.max_behind);
    let track = track_ctx
        .broadcast
        .track(&audio.name)?
        .subscribe(subscription)
        .await?;
    let mut reader = MoqTrackReader::new(
        track,
        audio.container,
        MediaKind::Audio(audio.codec),
        track_sync.deadline(),
        track_ctx.late_groups,
    );

    loop {
        if track_ctx
            .should_close
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            break;
        };
        let Some(chunk) = reader.read().await? else {
            break;
        };

        trace!(pts = ?chunk.chunk.pts, "Audio chunk received.");
        if track_sync.write_chunk(chunk).is_err() {
            debug!("Failed to send audio chunk, channel closed.");
            break;
        }
    }

    Ok(())
}

/// Forwards the chunks of a track to its decoder thread. Chunks the channel
/// cannot take are dropped.
struct MoqTrackSink {
    chunk_sender: Sender<PipelineEvent<EncodedInputEvent>>,
    /// Set once the decoder side is gone.
    closed: bool,
    /// Used to calculate stats
    sync_point: Instant,
}

impl MoqTrackSink {
    fn new(handle: DecoderThreadHandle, sync_point: Instant) -> Self {
        Self {
            chunk_sender: handle.chunk_sender,
            closed: false,
            sync_point,
        }
    }
}

impl TrackSink<MoqChunk> for MoqTrackSink {
    fn on_event(&mut self, event: TrackEvent<MoqChunk>) {
        let chunk = match event {
            TrackEvent::Chunk(chunk) => chunk.chunk,
            // ignore, assume that decoder does not need reset
            TrackEvent::Discontinuity => return,
        };

        trace!(
            pts = ?chunk.pts,
            buffered = ?(chunk.pts - self.sync_point.elapsed()),
            "Chunk released"
        );
        let event = PipelineEvent::Data(EncodedInputEvent::Chunk(chunk));
        match self.chunk_sender.try_send(event) {
            Ok(()) => (),
            Err(TrySendError::Full(_)) => debug!("Dropping chunk; decoder is not keeping up"),
            Err(TrySendError::Disconnected(_)) => self.closed = true,
        }
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
                input_ref: input_ref.clone(),
                transformer: None,
                frame_sender,
                input_buffer_size: buffer_size,
            },
        )?,
        VideoCodec::Vp9 => VideoDecoderThread::<FfmpegVp9Decoder, _>::spawn(
            input_ref.clone(),
            VideoDecoderThreadOptions::<H264AvccToAnnexB> {
                ctx: ctx.clone(),
                input_ref: input_ref.clone(),
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
        input_ref: input_ref.clone(),
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
                input_ref: input_ref.clone(),
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
                input_ref: input_ref.clone(),
                decoder_options: OpusDecoderOptions { opus_head: None },
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
