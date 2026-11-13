use std::sync::{Arc, OnceLock};

use hang::catalog::{AudioConfig, PRIORITY, VideoConfig};
use moq_mux::{
    catalog::{Reserved, hang::Container},
    container::{Frame, Producer as ContainerProducer},
};
use moq_net::{TimeOverflow, Timescale, Timestamp, broadcast, origin};
use smelter_render::error::ErrorStack;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tracing::{Instrument, Level, Span, error, info, span, trace, warn};

use crate::{
    event::Event,
    pipeline::{
        encoder::{
            fdk_aac::FdkAacEncoder, ffmpeg_h264::FfmpegH264Encoder, ffmpeg_vp8::FfmpegVp8Encoder,
            ffmpeg_vp9::FfmpegVp9Encoder, libopus::OpusEncoder, vulkan_h264::VulkanH264Encoder,
        },
        moq::{
            client::MoqConnection,
            output::{
                audio_encoder_thread::{
                    AudioEncoderThread, AudioEncoderThreadHandle, AudioEncoderThreadOptions,
                },
                catalog::{audio_catalog_entry, video_catalog_entry},
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

/// Everything the writer thread needs to publish one broadcast. The client,
/// origin and broadcast aren't used directly, but dropping any of them tears the
/// broadcast down, so the thread owns them for as long as it's writing.
struct BroadcastState {
    _client: MoqConnection,
    _origin: origin::Producer,
    _broadcast: broadcast::Producer,
    catalog: moq_mux::catalog::Producer,
    video: Option<ContainerProducer<Container, VideoConfig>>,
    audio: Option<ContainerProducer<Container, AudioConfig>>,
}

#[derive(thiserror::Error, Debug)]
enum BroadcastError {
    #[error("Write to broadcast failed.")]
    Write(#[from] moq_mux::Error),

    #[error("Invalid timestamp.")]
    InvalidTimestamp(#[from] TimeOverflow),
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

        let (client, origin) = MoqConnection::new_publisher(&ctx, &options.endpoint_url)?;
        let state = Self::publish(
            &ctx,
            &options,
            video_track_config,
            audio_track_config,
            client,
            origin,
        )?;

        ctx.spawn_async_task(|ctx| {
            async move {
                run_moq_output_task(&ctx, &output_ref, state, video_receiver, audio_receiver).await;

                ctx.event_emitter
                    .emit(Event::OutputDone(output_ref.id().clone()));
            }
            .instrument(Span::current())
        });

        Ok(Self {
            video: video_encoder_handle,
            audio: audio_encoder_handle,
        })
    }

    /// Announce the broadcast with its catalog.
    fn publish(
        ctx: &Arc<PipelineCtx>,
        options: &MoqClientOutputOptions,
        video_config: Option<(VideoConfig, Container)>,
        audio_config: Option<(AudioConfig, Container)>,
        client: MoqConnection,
        origin: origin::Producer,
    ) -> Result<BroadcastState, MoqClientError> {
        fn init_err(err: impl std::fmt::Display) -> MoqClientError {
            MoqClientError::BroadcastInitFailed(format!("{err}"))
        }
        let _guard = ctx.tokio_rt.enter();

        // Relay paths are absolute; a leading slash would make it a different path.
        let path = options.broadcast_path.trim_start_matches('/');
        let mut broadcast = origin
            .create_broadcast(path)
            .map_err(|_| MoqClientError::PublishFailed(options.broadcast_path.clone()))?;
        let catalog = moq_mux::catalog::Producer::new(&mut broadcast, Default::default())
            .map_err(init_err)?;

        // The catalog is withheld until the reservation drops, so its first snapshot
        // describes every rendition.
        let reserved = catalog.reserve();
        let video = match video_config {
            Some((config, container)) => {
                let info = Self::track_info(&reserved, PRIORITY.video, &container)?;
                let track = broadcast
                    .create_track(VIDEO_TRACK_NAME, info)
                    .map_err(init_err)?;
                Some(reserved.video(track, container, config).map_err(init_err)?)
            }
            None => None,
        };
        let audio = match audio_config {
            Some((config, container)) => {
                let info = Self::track_info(&reserved, PRIORITY.audio, &container)?;
                let track = broadcast
                    .create_track(AUDIO_TRACK_NAME, info)
                    .map_err(init_err)?;
                Some(reserved.audio(track, container, config).map_err(init_err)?)
            }
            None => None,
        };
        drop(reserved);

        broadcast
            .announce(Default::default())
            .map_err(|_| MoqClientError::PublishFailed(options.broadcast_path.clone()))?;
        info!(broadcast_path = path, "Publishing MoQ broadcast.");

        Ok(BroadcastState {
            _client: client,
            _origin: origin,
            _broadcast: broadcast,
            catalog,
            video,
            audio,
        })
    }

    /// CMAF timestamps are carried in the timescale of its init segment.
    fn track_info(
        reserved: &Reserved,
        priority: u8,
        container: &Container,
    ) -> Result<moq_net::track::Info, MoqClientError> {
        let info = reserved.track_info(priority);
        let Container::Cmaf(wire) = container else {
            return Ok(info);
        };
        let timescale = Timescale::new(wire.trak().mdia.mdhd.timescale as u64)
            .map_err(|err| MoqClientError::InitSegmentError(format!("{err}")))?;
        Ok(info.with_timescale(timescale))
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
    let mut packet_resolver = InterleavedPacketResolver::new(video, audio);

    loop {
        let Some(chunk) = packet_resolver.resolve_next_chunk(&mut state).await else {
            break;
        };
        ctx.stats_sender.send(
            MoqClientOutputTrackStatsEvent::BytesSent(chunk.data.len())
                .into_event(output_ref, chunk.kind.into()),
        );

        let offset = *timestamp_offset.get_or_insert(chunk.pts);
        if let Err(err) = send_chunk(&mut state, chunk, offset) {
            warn!(
                "MoQ broadcast failed: {}",
                ErrorStack::new(&err).into_string()
            );
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

struct InterleavedPacketResolver {
    video_receiver: Option<Receiver<EncodedOutputEvent>>,
    audio_receiver: Option<Receiver<EncodedOutputEvent>>,
    next_video: Option<EncodedOutputChunk>,
    next_audio: Option<EncodedOutputChunk>,
}

impl InterleavedPacketResolver {
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
        video_producer: Option<&mut ContainerProducer<Container, VideoConfig>>,
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
        audio_producer: Option<&mut ContainerProducer<Container, AudioConfig>>,
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

fn send_chunk(
    state: &mut BroadcastState,
    chunk: EncodedOutputChunk,
    timestamp_offset: crate::Timestamp,
) -> Result<(), BroadcastError> {
    let pts = chunk.pts - timestamp_offset;

    match chunk.kind {
        MediaKind::Video(_) => {
            let Some(producer) = state.video.as_mut() else {
                error!("Received unexpected video chunk.");
                return Ok(());
            };
            trace!(?pts, "MoQ video frame.");
            let timestamp = Timestamp::from_micros(pts.as_micros_saturating())?;
            let result = producer.write(Frame {
                timestamp,
                duration: None,
                payload: chunk.data,
                keyframe: chunk.is_keyframe,
            });
            match result {
                // The encoder may hand us deltas before the first keyframe; drop them
                // rather than treating them as a protocol violation.
                Err(moq_mux::Error::MissingKeyframe(_)) => {}
                result => result?,
            }
        }
        MediaKind::Audio(_) => {
            let Some(producer) = state.audio.as_mut() else {
                error!("Received unexpected audio chunk.");
                return Ok(());
            };
            // Audio has no keyframes, so every frame starts a group of its own.
            trace!(?pts, "MoQ audio frame.");
            let timestamp = Timestamp::from_micros(pts.as_micros_saturating())?;
            producer.write(Frame {
                timestamp,
                duration: None,
                payload: chunk.data,
                keyframe: true,
            })?;
            producer.cut(None)?;
        }
    }
    Ok(())
}
