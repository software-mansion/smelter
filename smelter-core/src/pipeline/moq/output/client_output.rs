use std::sync::Arc;

use crossbeam_channel::{Receiver, bounded};
use hang::moq_net::{Broadcast, BroadcastProducer, Origin, OriginProducer, Track};
use moq_mux::{
    catalog::hang::Container,
    container::{Frame, Producer as ContainerProducer, Timestamp},
};
use moq_native::ClientConfig;
use smelter_render::error::ErrorStack;
use tracing::{Level, Span, debug, error, info, span, warn};
use url::Url;

use crate::{
    event::Event,
    pipeline::{
        encoder::{
            encoder_thread_audio::{
                AudioEncoderThread, AudioEncoderThreadHandle, AudioEncoderThreadOptions,
            },
            encoder_thread_video::{
                VideoEncoderThread, VideoEncoderThreadHandle, VideoEncoderThreadOptions,
            },
            ffmpeg_h264::FfmpegH264Encoder,
            ffmpeg_vp8::FfmpegVp8Encoder,
            ffmpeg_vp9::FfmpegVp9Encoder,
            libopus::OpusEncoder,
            vulkan_h264::VulkanH264Encoder,
        },
        moq::{MoqSession, output::track},
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

        // Both encoders share one channel so the writer thread sees video and
        // audio chunks in the order they were produced.
        let (chunks_sender, chunks_receiver) = bounded(1000);

        let video = options
            .video
            .as_ref()
            .map(|video| Self::init_video_encoder(&ctx, &output_ref, video, chunks_sender.clone()))
            .transpose()?;
        let audio = options
            .audio
            .as_ref()
            .map(|audio| Self::init_audio_encoder(&ctx, &output_ref, audio, chunks_sender))
            .transpose()?;

        let video_track = match (&options.video, &video) {
            (Some(options_video), Some(handle)) => Some(track::video(
                options_video,
                handle.config.resolution,
                handle.config.output_format,
                ctx.output_framerate,
                handle.encoder_context(),
                options.container,
            )?),
            _ => None,
        };
        let audio_track = match &options.audio {
            Some(AudioEncoderOptions::Opus(opus)) => Some(track::audio(opus, options.container)?),
            _ => None,
        };

        let state = Self::publish(&ctx, &options, video_track, audio_track)?;

        let span = Span::current();
        std::thread::Builder::new()
            .name(format!("MoQ sender thread for output {output_ref}"))
            .spawn(move || {
                let _span = span.entered();
                // moq-net spawns tasks (e.g. group cleanup) from the producer calls below.
                let _guard = ctx.tokio_rt.enter();

                let stats_sender = MoqClientOutputStatsSender {
                    stats_sender: ctx.stats_sender.clone(),
                    output_ref: output_ref.clone(),
                };
                run_moq_output_thread(&ctx, &output_ref, state, chunks_receiver, stats_sender);

                ctx.event_emitter
                    .emit(Event::OutputDone(output_ref.id().clone()));
                debug!("Closing MoQ sender thread.");
            })
            .unwrap();

        Ok(Self { video, audio })
    }

    /// Connect to the relay and announce the broadcast with its catalog.
    fn publish(
        ctx: &Arc<PipelineCtx>,
        options: &MoqClientOutputOptions,
        video_track: Option<(hang::catalog::VideoConfig, Container)>,
        audio_track: Option<(hang::catalog::AudioConfig, Container)>,
    ) -> Result<BroadcastState, MoqClientError> {
        let (session, origin) = Self::connect(ctx, &options.endpoint_url)?;

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
        chunks_sender: crossbeam_channel::Sender<EncodedOutputEvent>,
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
        chunks_sender: crossbeam_channel::Sender<EncodedOutputEvent>,
    ) -> Result<AudioEncoderThreadHandle, OutputInitError> {
        let AudioEncoderOptions::Opus(options) = options else {
            return Err(OutputInitError::UnsupportedAudioCodec(AudioCodec::Aac));
        };
        let handle = AudioEncoderThread::<OpusEncoder>::spawn(
            output_id.clone(),
            AudioEncoderThreadOptions {
                ctx: ctx.clone(),
                encoder_options: options.clone(),
                chunks_sender,
            },
        )?;
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
        self.video.as_ref().map(|video| OutputVideo {
            resolution: video.config.resolution,
            frame_format: video.config.output_format,
            frame_sender: &video.frame_sender,
            keyframe_request_sender: &video.keyframe_request_sender,
        })
    }

    fn kind(&self) -> OutputProtocolKind {
        OutputProtocolKind::MoqClient
    }
}

fn run_moq_output_thread(
    ctx: &Arc<PipelineCtx>,
    output_ref: &Ref<OutputId>,
    mut state: BroadcastState,
    chunks_receiver: Receiver<EncodedOutputEvent>,
    stats_sender: MoqClientOutputStatsSender,
) {
    let mut eos_state = EosState::new(state.video.is_some(), state.audio.is_some());
    let mut timestamp_offset = None;

    for event in chunks_receiver {
        match event {
            EncodedOutputEvent::Data(chunk) => {
                let offset = *timestamp_offset.get_or_insert(chunk.pts);
                stats_sender.bytes_sent_event(chunk.data.len(), chunk.kind.into());

                if let Err(err) = write_chunk(&mut state, chunk, offset) {
                    ctx.event_emitter.emit(Event::OutputError {
                        output_id: output_ref.id().clone(),
                        err: err.into(),
                        severity: ErrorSeverity::Critical,
                    });
                    break;
                }
            }
            EncodedOutputEvent::VideoEOS => {
                eos_state.on_video_eos();
                finish(state.video.as_mut(), "video");
            }
            EncodedOutputEvent::AudioEOS => {
                eos_state.on_audio_eos();
                finish(state.audio.as_mut(), "audio");
            }
        }

        if eos_state.is_complete() {
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

fn write_chunk(
    state: &mut BroadcastState,
    chunk: EncodedOutputChunk,
    timestamp_offset: std::time::Duration,
) -> Result<(), OutputMoqClientRuntimeError> {
    let timestamp =
        Timestamp::from_micros(chunk.pts.saturating_sub(timestamp_offset).as_micros() as u64)
            .map_err(|err| OutputMoqClientRuntimeError::InvalidTimestamp(format!("{err}")))?;

    match chunk.kind {
        MediaKind::Video(_) => {
            let Some(producer) = state.video.as_mut() else {
                error!("Received unexpected video chunk.");
                return Ok(());
            };
            producer
                .write(Frame {
                    timestamp,
                    payload: chunk.data,
                    keyframe: chunk.is_keyframe,
                })
                .map_err(write_error)?;
        }
        MediaKind::Audio(_) => {
            let Some(producer) = state.audio.as_mut() else {
                error!("Received unexpected audio chunk.");
                return Ok(());
            };
            // Audio has no keyframes, so every frame starts a group of its own.
            // This matches what moq-mux's own audio importers do.
            producer
                .write(Frame {
                    timestamp,
                    payload: chunk.data,
                    keyframe: true,
                })
                .map_err(write_error)?;
            producer.finish_group().map_err(write_error)?;
        }
    }
    Ok(())
}

fn write_error(err: moq_mux::Error) -> OutputMoqClientRuntimeError {
    OutputMoqClientRuntimeError::WriteError(ErrorStack::new(&err).into_string())
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

struct EosState {
    received_video_eos: Option<bool>,
    received_audio_eos: Option<bool>,
}

impl EosState {
    fn new(has_video: bool, has_audio: bool) -> Self {
        Self {
            received_video_eos: has_video.then_some(false),
            received_audio_eos: has_audio.then_some(false),
        }
    }

    fn on_audio_eos(&mut self) {
        match self.received_audio_eos {
            Some(false) => self.received_audio_eos = Some(true),
            Some(true) => error!("Received multiple audio EOS events."),
            None => error!("Received audio EOS event on non audio output."),
        }
    }

    fn on_video_eos(&mut self) {
        match self.received_video_eos {
            Some(false) => self.received_video_eos = Some(true),
            Some(true) => error!("Received multiple video EOS events."),
            None => error!("Received video EOS event on non video output."),
        }
    }

    fn is_complete(&self) -> bool {
        self.received_video_eos.unwrap_or(true) && self.received_audio_eos.unwrap_or(true)
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
