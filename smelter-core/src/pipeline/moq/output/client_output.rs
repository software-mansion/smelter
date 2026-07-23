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
                track::{audio_catalog_entry, video_catalog_entry},
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

        let video_track_config = match (&options.video, &video_encoder_handle) {
            (Some(video_options), Some(handle)) => Some(video_catalog_entry(
                video_options,
                handle.config.resolution,
                handle.config.output_format,
                ctx.output_framerate,
                handle.encoder_context(),
                options.container,
            )?),
            _ => None,
        };
        let audio_track_config = match (&options.audio, &audio_encoder_handle) {
            (Some(audio_options), Some(handle)) => Some(audio_catalog_entry(
                audio_options,
                handle.encoder_context(),
                options.container,
            )?),
            _ => None,
        };

        let (session, origin) = Self::connect(&ctx, &options.endpoint_url)?;
        let state = Self::publish(
            &options,
            video_track_config,
            audio_track_config,
            session,
            origin,
        )?;

        let tokio_rt = ctx.tokio_rt.clone();
        tokio_rt.spawn(
            async move {
                run_moq_output_task(&ctx, &output_ref, state, video_receiver, audio_receiver).await;

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

    /// Connect to the relay and announce the broadcast with its catalog.
    fn publish(
        options: &MoqClientOutputOptions,
        video_config: Option<(hang::catalog::VideoConfig, Container)>,
        audio_config: Option<(hang::catalog::AudioConfig, Container)>,
        session: MoqSession,
        origin: OriginProducer,
    ) -> Result<BroadcastState, MoqClientError> {
        let mut broadcast = Broadcast::new().produce();
        let mut catalog = moq_mux::catalog::Producer::new(&mut broadcast)
            .map_err(|err| MoqClientError::BroadcastInitFailed(format!("{err}")))?;

        let (video_config, video) = match video_config {
            Some((config, container)) => {
                let producer = Self::create_track(&mut broadcast, VIDEO_TRACK_NAME, container)?;
                (Some(config), Some(producer))
            }
            None => (None, None),
        };

        let (audio_config, audio) = match audio_config {
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
    video: Option<Receiver<EncodedOutputEvent>>,
    audio: Option<Receiver<EncodedOutputEvent>>,
) {
    let mut timestamp_offset = None;
    let mut packet_sender = InterleavedPacketSender::new(video, audio);

    loop {
        let Some(chunk) = packet_sender.resolve_next_chunk(&mut state).await else {
            break;
        };
        ctx.stats_sender.send(
            MoqClientOutputTrackStatsEvent::BytesSent(chunk.data.len())
                .into_event(output_ref, chunk.kind.into()),
        );

        let offset = *timestamp_offset.get_or_insert(chunk.pts);
        if let Err(err) = write(&mut state, chunk, offset) {
            ctx.event_emitter.emit(Event::OutputError {
                output_id: output_ref.id().clone(),
                err: err.into(),
                severity: ErrorSeverity::Critical,
            });
            break;
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

struct InterleavedPacketSender {
    video_receiver: Option<Receiver<EncodedOutputEvent>>,
    audio_receiver: Option<Receiver<EncodedOutputEvent>>,
    next_video: Option<EncodedOutputChunk>,
    next_audio: Option<EncodedOutputChunk>,
}

impl InterleavedPacketSender {
    fn new(
        video_receiver: Option<Receiver<EncodedOutputEvent>>,
        audio_receiver: Option<Receiver<EncodedOutputEvent>>,
    ) -> Self {
        Self {
            video_receiver,
            audio_receiver,
            next_video: None,
            next_audio: None,
        }
    }

    async fn resolve_next_chunk(
        &mut self,
        state: &mut BroadcastState,
    ) -> Option<EncodedOutputChunk> {
        // Each iteration either receives one chunk or writes the pending one with the
        // lower PTS; it never does both.
        loop {
            let need_video = self.video_receiver.is_some() && self.next_video.is_none();
            let need_audio = self.audio_receiver.is_some() && self.next_audio.is_none();

            match (need_video, need_audio) {
                // Receive phase. When `need_*` is true the matching channel is present.
                (true, true) => {
                    tokio::select! {
                        event = self.video_receiver.as_mut().unwrap().recv() => self.handle_video_read(event, state.video.as_mut()),
                        event = self.audio_receiver.as_mut().unwrap().recv() => self.handle_audio_read(event, state.audio.as_mut()),
                    }
                }
                (true, false) => {
                    let event = self.video_receiver.as_mut().unwrap().recv().await;
                    self.handle_video_read(event, state.video.as_mut());
                }
                (false, true) => {
                    let event = self.audio_receiver.as_mut().unwrap().recv().await;
                    self.handle_audio_read(event, state.audio.as_mut());
                }

                // Write phase. Write the lower-PTS chunk (audio first on ties).
                (false, false) => return self.resolve_from_state(),
            }
        }
    }

    fn handle_video_read(
        &mut self,
        event: Option<EncodedOutputEvent>,
        video_producer: Option<&mut ContainerProducer<Container>>,
    ) {
        match event {
            Some(EncodedOutputEvent::Data(chunk)) => self.next_video = Some(chunk),
            Some(EncodedOutputEvent::AudioEOS) => error!("Unexpected audio EOS on video track."),
            Some(EncodedOutputEvent::VideoEOS) | None => {
                info!("Received video EOS.");
                self.video_receiver = None;
                if let Some(producer) = video_producer
                    && let Err(err) = producer.finish()
                {
                    warn!(%err, "Failed to close video producer.");
                }
            }
        }
    }

    fn handle_audio_read(
        &mut self,
        event: Option<EncodedOutputEvent>,
        audio_producer: Option<&mut ContainerProducer<Container>>,
    ) {
        match event {
            Some(EncodedOutputEvent::Data(chunk)) => self.next_audio = Some(chunk),
            Some(EncodedOutputEvent::VideoEOS) => error!("Unexpected video EOS on audio track."),
            Some(EncodedOutputEvent::AudioEOS) | None => {
                info!("Received audio EOS.");
                self.audio_receiver = None;
                if let Some(producer) = audio_producer
                    && let Err(err) = producer.finish()
                {
                    warn!(%err, "Failed to close audio producer.");
                }
            }
        }
    }

    fn resolve_from_state(&mut self) -> Option<EncodedOutputChunk> {
        match (&self.next_video, &self.next_audio) {
            (Some(video_chunk), Some(audio_chunk)) => {
                if video_chunk.pts < audio_chunk.pts {
                    self.next_video.take()
                } else {
                    self.next_audio.take()
                }
            }
            (Some(_), None) => self.next_video.take(),
            (None, Some(_)) => self.next_audio.take(),
            (None, None) => None,
        }
    }
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
                .map_err(|err| OutputMoqClientRuntimeError::WriteError(Arc::from(err)))?;
        }
        MediaKind::Audio(_) => {
            let Some(producer) = state.audio.as_mut() else {
                error!("Received unexpected audio chunk.");
                return Ok(());
            };
            // Audio has no keyframes, so every frame starts a group of its own.
            trace!(?pts, "MoQ audio frame.");
            let timestamp = Timestamp::from_micros(pts.as_micros() as u64)
                .map_err(|err| OutputMoqClientRuntimeError::InvalidTimestamp(format!("{err}")))?;
            producer
                .write(Frame {
                    timestamp,
                    payload: chunk.data,
                    keyframe: true,
                })
                .map_err(|err| OutputMoqClientRuntimeError::WriteError(Arc::from(err)))?;
            producer
                .finish_group()
                .map_err(|err| OutputMoqClientRuntimeError::WriteError(Arc::from(err)))?;
        }
    }
    Ok(())
}
