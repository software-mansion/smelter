use std::sync::{Arc, OnceLock};

use hang::moq_net::{Broadcast, BroadcastProducer, Origin, OriginProducer, Track};
use moq_mux::{
    catalog::hang::Container,
    container::{Frame, Producer as ContainerProducer, Timestamp},
};
use moq_native::ClientConfig;
use smelter_render::error::ErrorStack;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tracing::{Instrument, Level, Span, error, info, span, trace, warn};
use url::Url;

use crate::{
    event::Event,
    pipeline::{
        encoder::{
            fdk_aac::FdkAacEncoder, ffmpeg_h264::FfmpegH264Encoder, ffmpeg_vp8::FfmpegVp8Encoder,
            ffmpeg_vp9::FfmpegVp9Encoder, libopus::OpusEncoder, vulkan_h264::VulkanH264Encoder,
        },
        moq::{
            MoqSession,
            output::{
                audio_encoder_thread::{
                    AudioEncoderThread, AudioEncoderThreadHandle, AudioEncoderThreadOptions,
                },
                track,
                video_encoder_thread::{
                    VideoEncoderThread, VideoEncoderThreadHandle, VideoEncoderThreadOptions,
                },
            },
        },
        output::{Output, OutputAudio, OutputVideo},
    },
    utils::InitializableThread,
};

use crate::prelude::*;

const VIDEO_TRACK_NAME: &str = "video0";
const AUDIO_TRACK_NAME: &str = "audio0";

pub struct MoqClientOutput {
    video: Option<VideoEncoderThreadHandle>,
    audio: Option<AudioEncoderThreadHandle>,
}

/// Everything the writer thread needs to publish one broadcast. The session and
/// origin aren't used directly, but dropping either tears the broadcast down, so
/// the thread owns them for as long as it's writing.
struct BroadcastState {
    _session: MoqSession,
    _origin: OriginProducer,
    _broadcast: BroadcastProducer,
    catalog: moq_mux::catalog::Producer,
    video: Option<ContainerProducer<Container>>,
    audio: Option<ContainerProducer<Container>>,
}

impl MoqClientOutput {
    pub fn new(
        ctx: Arc<PipelineCtx>,
        output_ref: Ref<OutputId>,
        options: MoqClientOutputOptions,
    ) -> Result<Self, OutputInitError> {
        let _span = span!(
            Level::INFO,
            "MoQ client output",
            output_id = output_ref.to_string()
        )
        .entered();

        ctx.stats_sender.send(StatsEvent::NewOutput {
            output_ref: output_ref.clone(),
            kind: OutputProtocolKind::MoqClient,
        });

        // Reject unsupported codec/container pairs before spawning any encoder.
        // smelter-api rejects these at registration; this is the last line of defense.
        track::validate(&options.video, options.container)?;

        // A channel per media, so the writer thread can interleave chunks by PTS.
        let (video_encoder_handle, video_receiver) = match options.video.as_ref() {
            Some(video) => {
                let (sender, receiver) = mpsc::channel(1000);
                let handle = Self::init_video_encoder(&ctx, &output_ref, video, sender)?;
                (Some(handle), Some(receiver))
            }
            None => (None, None),
        };
        let (audio_encoder_handle, audio_receiver) = match options.audio.as_ref() {
            Some(audio) => {
                let (sender, receiver) = mpsc::channel(1000);
                let handle = Self::init_audio_encoder(&ctx, &output_ref, audio, sender)?;
                (Some(handle), Some(receiver))
            }
            None => (None, None),
        };

        let video_track = match (&options.video, &video_encoder_handle) {
            (Some(video_options), Some(handle)) => Some(track::video(
                video_options,
                handle.config.resolution,
                handle.config.output_format,
                ctx.output_framerate,
                handle.encoder_context(),
                options.container,
            )?),
            _ => None,
        };
        let audio_track = match (&options.audio, &audio_encoder_handle) {
            (Some(audio_options), Some(handle)) => Some(track::audio(
                audio_options,
                handle.encoder_context(),
                options.container,
            )?),
            _ => None,
        };

        let (session, origin) = Self::connect(&ctx, &options.endpoint_url)?;
        let state = Self::publish(&options, video_track, audio_track, session, origin)?;

        let tokio_rt = ctx.tokio_rt.clone();
        tokio_rt.spawn(
            async move {
                let stats_sender = MoqClientOutputStatsSender {
                    stats_sender: ctx.stats_sender.clone(),
                    output_ref: output_ref.clone(),
                };
                run_moq_output_task(
                    &ctx,
                    &output_ref,
                    state,
                    video_receiver,
                    audio_receiver,
                    stats_sender,
                )
                .await;

                ctx.event_emitter
                    .emit(Event::OutputDone(output_ref.id().clone()));
            }
            .instrument(Span::current()),
        );

        Ok(Self {
            video: video_encoder_handle,
            audio: audio_encoder_handle,
        })
    }

    /// Connect to the relay and announce the broadcast with its catalog.
    fn publish(
        options: &MoqClientOutputOptions,
        video_track: Option<(hang::catalog::VideoConfig, Container)>,
        audio_track: Option<(hang::catalog::AudioConfig, Container)>,
        session: MoqSession,
        origin: OriginProducer,
    ) -> Result<BroadcastState, MoqClientError> {
        let mut broadcast = Broadcast::new().produce();
        let mut catalog = moq_mux::catalog::Producer::new(&mut broadcast)
            .map_err(|err| MoqClientError::BroadcastInitFailed(format!("{err}")))?;

        let (video_config, video) = match video_track {
            Some((config, container)) => {
                let producer = Self::create_track(&mut broadcast, VIDEO_TRACK_NAME, container)?;
                (Some(config), Some(producer))
            }
            None => (None, None),
        };

        let (audio_config, audio) = match audio_track {
            Some((config, container)) => {
                let producer = Self::create_track(&mut broadcast, AUDIO_TRACK_NAME, container)?;
                (Some(config), Some(producer))
            }
            None => (None, None),
        };

        // One guard, so both catalog tracks (hang `catalog.json` and MSF
        // `catalog`) are published once, describing every rendition.
        {
            let mut guard = catalog.lock();
            if let Some(config) = video_config {
                guard
                    .video
                    .insert(VIDEO_TRACK_NAME, config)
                    .map_err(|err| MoqClientError::BroadcastInitFailed(format!("{err}")))?;
            }
            if let Some(config) = audio_config {
                guard
                    .audio
                    .insert(AUDIO_TRACK_NAME, config)
                    .map_err(|err| MoqClientError::BroadcastInitFailed(format!("{err}")))?;
            }
        }

        // Relay paths are absolute; a leading slash would make it a different path.
        let path = options.broadcast_path.trim_start_matches('/');
        if !origin.publish_broadcast(path, broadcast.consume()) {
            return Err(MoqClientError::PublishFailed(
                options.broadcast_path.clone(),
            ));
        }
        info!(broadcast_path = path, "Publishing MoQ broadcast.");

        Ok(BroadcastState {
            _session: session,
            _origin: origin,
            _broadcast: broadcast,
            catalog,
            video,
            audio,
        })
    }

    fn create_track(
        broadcast: &mut BroadcastProducer,
        name: &str,
        container: Container,
    ) -> Result<ContainerProducer<Container>, MoqClientError> {
        let track = broadcast
            .create_track(Track::new(name))
            .map_err(|err| MoqClientError::BroadcastInitFailed(format!("{err}")))?;
        // The encoder may hand us deltas before the first keyframe; drop them
        // rather than treating them as a protocol violation.
        Ok(ContainerProducer::new(track, container).with_lenient_start())
    }

    fn connect(
        ctx: &Arc<PipelineCtx>,
        url: &str,
    ) -> Result<(MoqSession, OriginProducer), MoqClientError> {
        let url = Url::parse(url).map_err(|err| MoqClientError::InvalidUrl(Arc::from(url), err))?;

        if !matches!(url.scheme(), "https" | "http") {
            return Err(MoqClientError::InvalidScheme(url.scheme().to_string()));
        }

        let mut config = ClientConfig::default();
        config.tls.disable_verify = Some(ctx.moq_disable_tls_verification);
        let client = config
            .init()
            .map_err(|err| MoqClientError::ClientInitFailed(format!("{err}")))?;

        let origin = Origin::random().produce();
        let client = client.with_publish(origin.consume());

        let session = ctx
            .tokio_rt
            .block_on(client.connect(url))
            .map_err(|err| MoqClientError::ConnectFailed(format!("{err}")))?;
        let session = MoqSession::new(session, ctx.tokio_rt.clone());
        info!(moq_version = ?session.version(), "MoQ client session established");
        Ok((session, origin))
    }

    fn init_video_encoder(
        ctx: &Arc<PipelineCtx>,
        output_id: &Ref<OutputId>,
        options: &VideoEncoderOptions,
        chunks_sender: Sender<EncodedOutputEvent>,
    ) -> Result<VideoEncoderThreadHandle, OutputInitError> {
        let handle = match options {
            VideoEncoderOptions::FfmpegH264(options) => {
                VideoEncoderThread::<FfmpegH264Encoder>::spawn(
                    output_id.clone(),
                    VideoEncoderThreadOptions {
                        ctx: ctx.clone(),
                        encoder_options: options.clone(),
                        chunks_sender,
                    },
                )?
            }
            VideoEncoderOptions::VulkanH264(options) => {
                if !ctx.graphics_context.has_vulkan_encoder_support() {
                    return Err(OutputInitError::EncoderError(
                        EncoderInitError::VulkanContextRequiredForVulkanEncoder,
                    ));
                }
                VideoEncoderThread::<VulkanH264Encoder>::spawn(
                    output_id.clone(),
                    VideoEncoderThreadOptions {
                        ctx: ctx.clone(),
                        encoder_options: options.clone(),
                        chunks_sender,
                    },
                )?
            }
            VideoEncoderOptions::FfmpegVp8(options) => {
                VideoEncoderThread::<FfmpegVp8Encoder>::spawn(
                    output_id.clone(),
                    VideoEncoderThreadOptions {
                        ctx: ctx.clone(),
                        encoder_options: options.clone(),
                        chunks_sender,
                    },
                )?
            }
            VideoEncoderOptions::FfmpegVp9(options) => {
                VideoEncoderThread::<FfmpegVp9Encoder>::spawn(
                    output_id.clone(),
                    VideoEncoderThreadOptions {
                        ctx: ctx.clone(),
                        encoder_options: options.clone(),
                        chunks_sender,
                    },
                )?
            }
        };
        Ok(handle)
    }

    fn init_audio_encoder(
        ctx: &Arc<PipelineCtx>,
        output_id: &Ref<OutputId>,
        options: &AudioEncoderOptions,
        chunks_sender: Sender<EncodedOutputEvent>,
    ) -> Result<AudioEncoderThreadHandle, OutputInitError> {
        let handle = match options {
            AudioEncoderOptions::Opus(options) => AudioEncoderThread::<OpusEncoder>::spawn(
                output_id.clone(),
                AudioEncoderThreadOptions {
                    ctx: ctx.clone(),
                    encoder_options: options.clone(),
                    chunks_sender,
                },
            )?,
            AudioEncoderOptions::FdkAac(options) => AudioEncoderThread::<FdkAacEncoder>::spawn(
                output_id.clone(),
                AudioEncoderThreadOptions {
                    ctx: ctx.clone(),
                    encoder_options: options.clone(),
                    chunks_sender,
                },
            )?,
        };
        Ok(handle)
    }
}

impl Output for MoqClientOutput {
    fn audio(&self) -> Option<OutputAudio<'_>> {
        self.audio.as_ref().map(|audio| OutputAudio {
            samples_batch_sender: &audio.sample_batch_sender,
        })
    }

    fn video(&self) -> Option<OutputVideo<'_>> {
        static FAKE_SENDER: OnceLock<crossbeam_channel::Sender<()>> = OnceLock::new();
        let keyframe_request_sender = FAKE_SENDER.get_or_init(|| crossbeam_channel::bounded(1).0);

        self.video.as_ref().map(|video| OutputVideo {
            resolution: video.config.resolution,
            frame_format: video.config.output_format,
            frame_sender: &video.frame_sender,
            keyframe_request_sender,
        })
    }

    fn kind(&self) -> OutputProtocolKind {
        OutputProtocolKind::MoqClient
    }
}

/// Interleaves encoded chunks from the two encoders and writes them to the
/// broadcast in PTS order. Mirrors the sync A/V muxing done by the RTMP output.
async fn run_moq_output_task(
    ctx: &Arc<PipelineCtx>,
    output_ref: &Ref<OutputId>,
    mut state: BroadcastState,
    mut video: Option<Receiver<EncodedOutputEvent>>,
    mut audio: Option<Receiver<EncodedOutputEvent>>,
    stats_sender: MoqClientOutputStatsSender,
) {
    let mut timestamp_offset = None;
    let mut pending_video: Option<EncodedOutputChunk> = None;
    let mut pending_audio: Option<EncodedOutputChunk> = None;
    // A missing channel means that media is not produced at all, i.e. already done.
    let mut video_eos = video.is_none();
    let mut audio_eos = audio.is_none();

    // Each iteration either receives one chunk or writes the pending one with the
    // lower PTS; it never does both.
    loop {
        let need_video = pending_video.is_none() && !video_eos;
        let need_audio = pending_audio.is_none() && !audio_eos;

        match (need_video, need_audio) {
            // Receive phase. When `need_*` is true the matching channel is present.
            (true, true) => {
                tokio::select! {
                    msg = video.as_mut().unwrap().recv() => match msg {
                        Some(EncodedOutputEvent::Data(chunk)) => pending_video = Some(chunk),
                        _ => {
                            video_eos = true;
                            finish(state.video.as_mut(), "video");
                        }
                    },
                    msg = audio.as_mut().unwrap().recv() => match msg {
                        Some(EncodedOutputEvent::Data(chunk)) => pending_audio = Some(chunk),
                        _ => {
                            audio_eos = true;
                            finish(state.audio.as_mut(), "audio");
                        }
                    },
                    else => break,
                }
            }
            (true, false) => match video.as_mut().unwrap().recv().await {
                Some(EncodedOutputEvent::Data(chunk)) => pending_video = Some(chunk),
                _ => {
                    video_eos = true;
                    finish(state.video.as_mut(), "video");
                }
            },
            (false, true) => match audio.as_mut().unwrap().recv().await {
                Some(EncodedOutputEvent::Data(chunk)) => pending_audio = Some(chunk),
                _ => {
                    audio_eos = true;
                    finish(state.audio.as_mut(), "audio");
                }
            },

            // Write phase. Write the lower-PTS chunk (video first on ties).
            (false, false) => {
                let Some(chunk) = get_chunk_to_write(&mut pending_video, &mut pending_audio) else {
                    // both tracks finished
                    break;
                };

                if write_chunk(
                    ctx,
                    output_ref,
                    &mut state,
                    &stats_sender,
                    &mut timestamp_offset,
                    chunk,
                )
                .is_err()
                {
                    break;
                }
            }
        }
    }

    if let Err(err) = state.catalog.finish() {
        warn!(
            "Failed to close MoQ catalog: {}",
            ErrorStack::new(&err).into_string()
        );
    }
    // Dropping `state` closes the session, which unannounces the broadcast.
}

fn get_chunk_to_write(
    pending_video: &mut Option<EncodedOutputChunk>,
    pending_audio: &mut Option<EncodedOutputChunk>,
) -> Option<EncodedOutputChunk> {
    match (pending_video.take(), pending_audio.take()) {
        (Some(video), Some(audio)) => {
            if video.pts <= audio.pts {
                *pending_audio = Some(audio);
                Some(video)
            } else {
                *pending_video = Some(video);
                Some(audio)
            }
        }
        (Some(video), None) => Some(video),
        (None, Some(audio)) => Some(audio),
        (None, None) => None,
    }
}

/// Writes one chunk and records its stats. Returns `false` if a critical write
/// error was emitted and the writer thread must stop.
fn write_chunk(
    ctx: &Arc<PipelineCtx>,
    output_ref: &Ref<OutputId>,
    state: &mut BroadcastState,
    stats_sender: &MoqClientOutputStatsSender,
    timestamp_offset: &mut Option<std::time::Duration>,
    chunk: EncodedOutputChunk,
) -> Result<(), ()> {
    let offset = *timestamp_offset.get_or_insert(chunk.pts);
    stats_sender.bytes_sent_event(chunk.data.len(), chunk.kind.into());

    if let Err(err) = write(state, chunk, offset) {
        ctx.event_emitter.emit(Event::OutputError {
            output_id: output_ref.id().clone(),
            err: err.into(),
            severity: ErrorSeverity::Critical,
        });
        return Err(());
    }
    Ok(())
}

fn write(
    state: &mut BroadcastState,
    chunk: EncodedOutputChunk,
    timestamp_offset: std::time::Duration,
) -> Result<(), OutputMoqClientRuntimeError> {
    let pts = chunk.pts.saturating_sub(timestamp_offset);

    match chunk.kind {
        MediaKind::Video(_) => {
            let Some(producer) = state.video.as_mut() else {
                error!("Received unexpected video chunk.");
                return Ok(());
            };
            trace!(?pts, "MoQ video frame.");
            let timestamp = Timestamp::from_micros(pts.as_micros() as u64)
                .map_err(|err| OutputMoqClientRuntimeError::InvalidTimestamp(format!("{err}")))?;
            producer
                .write(Frame {
                    timestamp,
                    payload: chunk.data,
                    keyframe: chunk.is_keyframe,
                })
                .map_err(|err| {
                    OutputMoqClientRuntimeError::WriteError(ErrorStack::new(&err).into_string())
                })?;
        }
        MediaKind::Audio(_) => {
            let Some(producer) = state.audio.as_mut() else {
                error!("Received unexpected audio chunk.");
                return Ok(());
            };
            // Audio has no keyframes, so every frame starts a group of its own.
            // This matches what moq-mux's own audio importers do.
            trace!(?pts, "MoQ audio frame.");
            let timestamp = Timestamp::from_micros(pts.as_micros() as u64)
                .map_err(|err| OutputMoqClientRuntimeError::InvalidTimestamp(format!("{err}")))?;
            producer
                .write(Frame {
                    timestamp,
                    payload: chunk.data,
                    keyframe: true,
                })
                .map_err(|err| {
                    OutputMoqClientRuntimeError::WriteError(ErrorStack::new(&err).into_string())
                })?;
            producer.finish_group().map_err(|err| {
                OutputMoqClientRuntimeError::WriteError(ErrorStack::new(&err).into_string())
            })?;
        }
    }
    Ok(())
}

fn finish(producer: Option<&mut ContainerProducer<Container>>, kind: &str) {
    if let Some(producer) = producer
        && let Err(err) = producer.finish()
    {
        warn!(
            "Failed to close MoQ {kind} track: {}",
            ErrorStack::new(&err).into_string()
        );
    }
}

struct MoqClientOutputStatsSender {
    stats_sender: StatsSender,
    output_ref: Ref<OutputId>,
}

impl MoqClientOutputStatsSender {
    fn bytes_sent_event(&self, size: usize, track_kind: StatsTrackKind) {
        self.stats_sender.send(
            MoqClientOutputTrackStatsEvent::BytesSent(size)
                .into_event(&self.output_ref, track_kind),
        );
    }
}
