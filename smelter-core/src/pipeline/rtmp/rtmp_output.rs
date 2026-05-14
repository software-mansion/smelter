use std::{collections::HashMap, sync::Arc, time::Duration};

use bytes::Bytes;
use crossbeam_channel::{Receiver, bounded, select};
use smelter_render::error::ErrorStack;
use tracing::{debug, warn};

use rtmp::{
    AudioData, RtmpAudioCodec, RtmpClient, RtmpClientConfig, RtmpSerializationMode,
    RtmpStreamError, RtmpVideoCodec, TrackId, VideoData, AmfValue,
};

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
            fdk_aac::FdkAacEncoder,
            ffmpeg_h264::FfmpegH264Encoder,
            ffmpeg_vp8::FfmpegVp8Encoder,
            ffmpeg_vp9::FfmpegVp9Encoder,
            libopus::OpusEncoder,
            vulkan_h264::VulkanH264Encoder,
        },
        output::{Output, OutputAudio, OutputVideo},
    },
    utils::InitializableThread,
};

use crate::prelude::*;

pub struct RtmpClientOutput {
    video: Option<VideoEncoderThreadHandle>,
    audio: Option<AudioEncoderThreadHandle>,
}

struct AudioConfig {
    extradata: Bytes,
    codec: RtmpAudioCodec,
    channels: AudioChannels,
    sample_rate: u32,
    chunks_receiver: Receiver<EncodedOutputEvent>,
}

struct VideoConfig {
    extradata: Bytes,
    codec: RtmpVideoCodec,
    resolution: Resolution,
    chunks_receiver: Receiver<EncodedOutputEvent>,
}

impl RtmpClientOutput {
    pub fn new(
        ctx: Arc<PipelineCtx>,
        output_ref: Ref<OutputId>,
        options: RtmpOutputOptions,
    ) -> Result<Self, OutputInitError> {
        let (video_encoder, video_config) = match &options.video {
            Some(video) => {
                let (encoder, config) = Self::init_video_encoder(&ctx, &output_ref, video.clone())?;
                (Some(encoder), Some(config))
            }
            None => (None, None),
        };

        let (audio_encoder, audio_config) = match &options.audio {
            Some(audio) => {
                let (encoder, config) = Self::init_audio_encoder(&ctx, &output_ref, audio.clone())?;
                (Some(encoder), Some(config))
            }
            None => (None, None),
        };

        ctx.stats_sender.send(StatsEvent::NewOutput {
            output_ref: output_ref.clone(),
            kind: OutputProtocolKind::Rtmp,
        });

        let client = Self::establish_connection(
            &ctx,
            options.connection,
            options.force_enhanced_rtmp,
            &video_config,
            &audio_config,
        )?;
        std::thread::Builder::new()
            .name(format!("RTMP sender thread for output {output_ref}"))
            .spawn(move || {
                let _span = tracing::info_span!("RTMP sender", output_id = output_ref.to_string())
                    .entered();

                let stats_sender = RtmpOutputStatsSender {
                    stats_sender: ctx.stats_sender.clone(),
                    output_ref: output_ref.clone(),
                };
                let result =
                    run_rtmp_output_thread(client, video_config, audio_config, stats_sender);
                if let Err(err) = result {
                    warn!("{}", ErrorStack::new(&err).into_string())
                }

                ctx.event_emitter
                    .emit(Event::OutputDone(output_ref.id().clone()));
                debug!("Closing RTMP sender thread.");
            })
            .unwrap();

        Ok(Self {
            video: video_encoder,
            audio: audio_encoder,
        })
    }

    fn establish_connection(
        ctx: &Arc<PipelineCtx>,
        connection_opts: RtmpConnectionOptions,
        force_enhanced_rtmp: bool,
        video_config: &Option<VideoConfig>,
        audio_config: &Option<AudioConfig>,
    ) -> Result<RtmpClient, RtmpClientError> {
        let mut client = RtmpClient::connect(RtmpClientConfig {
            host: connection_opts.host,
            port: connection_opts.port,
            app: connection_opts.app,
            stream_key: connection_opts.stream_key,
            use_tls: connection_opts.use_tls,
            serialization_mode: if force_enhanced_rtmp {
                RtmpSerializationMode::Enhanced
            } else {
                RtmpSerializationMode::Auto
            },
        })?;

        if let Some(metadata) = output_metadata(ctx, video_config, audio_config) {
            client.send(rtmp::RtmpEvent::Metadata(metadata))?;
        }

        if let Some(config) = video_config {
            client.send(rtmp::VideoConfig {
                track_id: TrackId::PRIMARY,
                codec: config.codec,
                data: config.extradata.clone(),
            })?;
        }
        if let Some(config) = audio_config {
            client.send(rtmp::AudioConfig {
                track_id: TrackId::PRIMARY,
                codec: config.codec,
                data: config.extradata.clone(),
                channels: match config.channels {
                    AudioChannels::Mono => rtmp::AudioChannels::Mono,
                    AudioChannels::Stereo => rtmp::AudioChannels::Stereo,
                },
            })?;
        }
        Ok(client)
    }

    fn init_video_encoder(
        ctx: &Arc<PipelineCtx>,
        output_id: &Ref<OutputId>,
        options: VideoEncoderOptions,
    ) -> Result<(VideoEncoderThreadHandle, VideoConfig), OutputInitError> {
        let (chunks_sender, chunks_receiver) = bounded(1000);

        let (encoder, codec) = match &options {
            VideoEncoderOptions::FfmpegH264(options) => {
                let encoder = VideoEncoderThread::<FfmpegH264Encoder>::spawn(
                    output_id.clone(),
                    VideoEncoderThreadOptions {
                        ctx: ctx.clone(),
                        encoder_options: options.clone(),
                        chunks_sender,
                    },
                )?;
                (encoder, RtmpVideoCodec::H264)
            }
            VideoEncoderOptions::VulkanH264(options) => {
                if !ctx.graphics_context.has_vulkan_encoder_support() {
                    return Err(OutputInitError::EncoderError(
                        EncoderInitError::VulkanContextRequiredForVulkanEncoder,
                    ));
                }
                let encoder = VideoEncoderThread::<VulkanH264Encoder>::spawn(
                    output_id.clone(),
                    VideoEncoderThreadOptions {
                        ctx: ctx.clone(),
                        encoder_options: options.clone(),
                        chunks_sender,
                    },
                )?;
                (encoder, RtmpVideoCodec::H264)
            }
            VideoEncoderOptions::FfmpegVp8(options) => {
                let encoder = VideoEncoderThread::<FfmpegVp8Encoder>::spawn(
                    output_id.clone(),
                    VideoEncoderThreadOptions {
                        ctx: ctx.clone(),
                        encoder_options: options.clone(),
                        chunks_sender,
                    },
                )?;
                (encoder, RtmpVideoCodec::Vp8)
            }
            VideoEncoderOptions::FfmpegVp9(options) => {
                let encoder = VideoEncoderThread::<FfmpegVp9Encoder>::spawn(
                    output_id.clone(),
                    VideoEncoderThreadOptions {
                        ctx: ctx.clone(),
                        encoder_options: options.clone(),
                        chunks_sender,
                    },
                )?;
                (encoder, RtmpVideoCodec::Vp9)
            }
        };

        let extradata = encoder.encoder_context().unwrap_or_default();
        Ok((
            encoder,
            VideoConfig {
                extradata,
                codec,
                resolution: options.resolution(),
                chunks_receiver,
            },
        ))
    }

    fn init_audio_encoder(
        ctx: &Arc<PipelineCtx>,
        output_id: &Ref<OutputId>,
        options: AudioEncoderOptions,
    ) -> Result<(AudioEncoderThreadHandle, AudioConfig), OutputInitError> {
        let channels = options.channels();
        let sample_rate = options.sample_rate();

        let (chunks_sender, chunks_receiver) = bounded(1000);
        let (encoder, codec) = match options {
            AudioEncoderOptions::FdkAac(options) => {
                let encoder = AudioEncoderThread::<FdkAacEncoder>::spawn(
                    output_id.clone(),
                    AudioEncoderThreadOptions {
                        ctx: ctx.clone(),
                        encoder_options: options,
                        chunks_sender,
                    },
                )?;
                (encoder, RtmpAudioCodec::Aac)
            }
            AudioEncoderOptions::Opus(options) => {
                let encoder = AudioEncoderThread::<OpusEncoder>::spawn(
                    output_id.clone(),
                    AudioEncoderThreadOptions {
                        ctx: ctx.clone(),
                        encoder_options: options,
                        chunks_sender,
                    },
                )?;
                (encoder, RtmpAudioCodec::Opus)
            }
        };
        let extradata = encoder.encoder_context().unwrap_or_default();

        Ok((
            encoder,
            AudioConfig {
                extradata,
                codec,
                channels,
                sample_rate,
                chunks_receiver,
            },
        ))
    }
}

impl Output for RtmpClientOutput {
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
        OutputProtocolKind::Rtmp
    }
}

fn video_chunk_to_event(
    chunk: EncodedOutputChunk,
    codec: RtmpVideoCodec,
    timestamp_offset: Duration,
) -> VideoData {
    VideoData {
        track_id: TrackId::PRIMARY,
        codec,
        pts: chunk.pts.saturating_sub(timestamp_offset),
        dts: chunk
            .dts
            .unwrap_or(chunk.pts)
            .saturating_sub(timestamp_offset),
        data: chunk.data,
        is_keyframe: chunk.is_keyframe,
    }
}

fn audio_chunk_to_event(
    chunk: EncodedOutputChunk,
    codec: RtmpAudioCodec,
    timestamp_offset: Duration,
) -> AudioData {
    AudioData {
        track_id: TrackId::PRIMARY,
        codec,
        pts: chunk.pts.saturating_sub(timestamp_offset),
        data: chunk.data,
    }
}

fn video_chunk_start_timestamp(chunk: &EncodedOutputChunk) -> Duration {
    chunk.dts.unwrap_or(chunk.pts)
}

fn audio_chunk_start_timestamp(chunk: &EncodedOutputChunk) -> Duration {
    chunk.pts
}

fn output_metadata(
    ctx: &Arc<PipelineCtx>,
    video_config: &Option<VideoConfig>,
    audio_config: &Option<AudioConfig>,
) -> Option<HashMap<String, AmfValue>> {
    let mut metadata = HashMap::new();

    if let Some(video) = video_config {
        metadata.insert("width".into(), AmfValue::Number(video.resolution.width as f64));
        metadata.insert("height".into(), AmfValue::Number(video.resolution.height as f64));
        metadata.insert(
            "framerate".into(),
            AmfValue::Number(ctx.output_framerate.num as f64 / ctx.output_framerate.den as f64),
        );
        metadata.insert(
            "videocodecid".into(),
            match video.codec {
                RtmpVideoCodec::H264 => "avc1",
                RtmpVideoCodec::Vp8 => "vp08",
                RtmpVideoCodec::Vp9 => "vp09",
            }
            .into(),
        );
    }

    if let Some(audio) = audio_config {
        metadata.insert(
            "audiocodecid".into(),
            match audio.codec {
                RtmpAudioCodec::Aac => "mp4a",
                RtmpAudioCodec::Opus => "Opus",
            }
            .into(),
        );
        metadata.insert(
            "audiosamplerate".into(),
            AmfValue::Number(audio.sample_rate as f64),
        );
        metadata.insert(
            "stereo".into(),
            AmfValue::Boolean(matches!(audio.channels, AudioChannels::Stereo)),
        );
        metadata.insert(
            "audiochannels".into(),
            AmfValue::Number(match audio.channels {
                AudioChannels::Mono => 1.0,
                AudioChannels::Stereo => 2.0,
            }),
        );
    }

    (!metadata.is_empty()).then_some(metadata)
}

fn run_rtmp_output_thread(
    mut client: RtmpClient,
    video_config: Option<VideoConfig>,
    audio_config: Option<AudioConfig>,
    stats_sender: RtmpOutputStatsSender,
) -> Result<(), RtmpStreamError> {
    match (video_config, audio_config) {
        (Some(video), Some(audio)) => run_synced_av(
            &mut client,
            &video.chunks_receiver,
            video.codec,
            &audio.chunks_receiver,
            audio.codec,
            stats_sender,
        ),
        (Some(video), None) => {
            let codec = video.codec;
            let mut timestamp_offset = None;
            while let Ok(EncodedOutputEvent::Data(chunk)) = video.chunks_receiver.recv() {
                let timestamp_offset =
                    *timestamp_offset.get_or_insert(video_chunk_start_timestamp(&chunk));
                stats_sender.bytes_sent_event(chunk.data.len(), StatsTrackKind::Video);
                client.send(video_chunk_to_event(chunk, codec, timestamp_offset))?;
            }
            Ok(())
        }
        (None, Some(audio)) => {
            let codec = audio.codec;
            let mut timestamp_offset = None;
            while let Ok(EncodedOutputEvent::Data(chunk)) = audio.chunks_receiver.recv() {
                let timestamp_offset =
                    *timestamp_offset.get_or_insert(audio_chunk_start_timestamp(&chunk));
                stats_sender.bytes_sent_event(chunk.data.len(), StatsTrackKind::Audio);
                client.send(audio_chunk_to_event(chunk, codec, timestamp_offset))?;
            }
            Ok(())
        }
        (None, None) => Ok(()),
    }
}

fn run_synced_av(
    client: &mut RtmpClient,
    video_rx: &Receiver<EncodedOutputEvent>,
    video_codec: RtmpVideoCodec,
    audio_rx: &Receiver<EncodedOutputEvent>,
    audio_codec: RtmpAudioCodec,
    rtmp_stats_sender: RtmpOutputStatsSender,
) -> Result<(), RtmpStreamError> {
    let mut pending_video: Option<EncodedOutputChunk> = None;
    let mut pending_audio: Option<EncodedOutputChunk> = None;
    let mut timestamp_offset: Option<Duration> = None;
    let mut video_eos = false;
    let mut audio_eos = false;

    // Each iteration can either send or receive. It will never do both
    // in the same iteration.
    loop {
        let need_video = pending_video.is_none() && !video_eos;
        let need_audio = pending_audio.is_none() && !audio_eos;

        match (need_video, need_audio) {
            //
            // Receive phase
            //
            (true, true) => {
                select! {
                    recv(video_rx) -> msg => {
                        match msg {
                            Ok(EncodedOutputEvent::Data(chunk)) => pending_video = Some(chunk),
                            _ => video_eos = true,
                        }
                    }
                    recv(audio_rx) -> msg => {
                        match msg {
                            Ok(EncodedOutputEvent::Data(chunk)) => pending_audio = Some(chunk),
                            _ => audio_eos = true,
                        }
                    }
                }
            }
            (true, false) => match video_rx.recv() {
                Ok(EncodedOutputEvent::Data(chunk)) => pending_video = Some(chunk),
                _ => video_eos = true,
            },
            (false, true) => match audio_rx.recv() {
                Ok(EncodedOutputEvent::Data(chunk)) => pending_audio = Some(chunk),
                _ => audio_eos = true,
            },

            //
            // Send phase
            //
            (false, false) => match (&pending_video, &pending_audio) {
                (Some(video), Some(audio)) => {
                    let timestamp_offset = *timestamp_offset.get_or_insert_with(|| {
                        video_chunk_start_timestamp(video).min(audio_chunk_start_timestamp(audio))
                    });

                    if video.pts <= audio.pts {
                        rtmp_stats_sender.bytes_sent_event(video.data.len(), StatsTrackKind::Video);
                        client.send(video_chunk_to_event(
                            pending_video.take().unwrap(),
                            video_codec,
                            timestamp_offset,
                        ))?;
                    } else {
                        rtmp_stats_sender.bytes_sent_event(audio.data.len(), StatsTrackKind::Audio);
                        client.send(audio_chunk_to_event(
                            pending_audio.take().unwrap(),
                            audio_codec,
                            timestamp_offset,
                        ))?;
                    }
                }
                (Some(video), None) => {
                    let timestamp_offset =
                        *timestamp_offset.get_or_insert(video_chunk_start_timestamp(video));
                    rtmp_stats_sender.bytes_sent_event(video.data.len(), StatsTrackKind::Video);
                    client.send(video_chunk_to_event(
                        pending_video.take().unwrap(),
                        video_codec,
                        timestamp_offset,
                    ))?;
                }
                (None, Some(audio)) => {
                    let timestamp_offset =
                        *timestamp_offset.get_or_insert(audio_chunk_start_timestamp(audio));
                    rtmp_stats_sender.bytes_sent_event(audio.data.len(), StatsTrackKind::Audio);
                    client.send(audio_chunk_to_event(
                        pending_audio.take().unwrap(),
                        audio_codec,
                        timestamp_offset,
                    ))?;
                }
                (None, None) => break,
            },
        };
    }

    Ok(())
}

struct RtmpOutputStatsSender {
    stats_sender: StatsSender,
    output_ref: Ref<OutputId>,
}

impl RtmpOutputStatsSender {
    fn bytes_sent_event(&self, size: usize, track_kind: StatsTrackKind) {
        self.stats_sender.send(
            RtmpOutputTrackStatsEvent::BytesSent(size).into_event(&self.output_ref, track_kind),
        );
    }
}
