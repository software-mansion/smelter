use std::sync::Arc;

use bytes::Bytes;
use crossbeam_channel::{Receiver, bounded, select};
use smelter_render::error::ErrorStack;
use tracing::{debug, warn};

use rtmp::{
    AacAudioConfig, AacAudioData, H264VideoConfig, H264VideoData, OpusAudioData, RtmpClient,
    RtmpClientConfig, RtmpStreamError, Vp9VideoData,
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
    codec: OutputAudioCodec,
    extradata: Option<Bytes>,
    channels: AudioChannels,
    chunks_receiver: Receiver<EncodedOutputEvent>,
}

struct VideoConfig {
    codec: OutputVideoCodec,
    extradata: Option<Bytes>,
    chunks_receiver: Receiver<EncodedOutputEvent>,
}

#[derive(Clone, Copy)]
enum OutputVideoCodec {
    H264,
    Vp9,
}

#[derive(Clone, Copy)]
enum OutputAudioCodec {
    Aac,
    Opus,
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

        let client = Self::establish_connection(options.connection, &video_config, &audio_config)?;
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
        connection_opts: RtmpConnectionOptions,
        video_config: &Option<VideoConfig>,
        audio_config: &Option<AudioConfig>,
    ) -> Result<RtmpClient, RtmpClientError> {
        let mut client = RtmpClient::connect(RtmpClientConfig {
            host: connection_opts.host,
            port: connection_opts.port,
            app: connection_opts.app,
            stream_key: connection_opts.stream_key,
            use_tls: connection_opts.use_tls,
        })?;

        if let Some(config) = video_config {
            match config.codec {
                OutputVideoCodec::H264 => {
                    if let Some(extradata) = &config.extradata {
                        client.send(H264VideoConfig {
                            data: extradata.clone(),
                        })?;
                    }
                }
                OutputVideoCodec::Vp9 => {
                    // VP9 encoder does not produce extradata; skip config event.
                }
            }
        }
        if let Some(config) = audio_config {
            match config.codec {
                OutputAudioCodec::Aac => {
                    if let Some(extradata) = &config.extradata {
                        let config = AacAudioConfig::try_from(extradata.clone())?;
                        client.send(config)?;
                    }
                }
                OutputAudioCodec::Opus => {
                    // Opus encoder does not produce extradata; skip config event.
                }
            }
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
            VideoEncoderOptions::FfmpegH264(options) => (
                VideoEncoderThread::<FfmpegH264Encoder>::spawn(
                    output_id.clone(),
                    VideoEncoderThreadOptions {
                        ctx: ctx.clone(),
                        encoder_options: options.clone(),
                        chunks_sender,
                    },
                )?,
                OutputVideoCodec::H264,
            ),
            VideoEncoderOptions::VulkanH264(options) => {
                if !ctx.graphics_context.has_vulkan_encoder_support() {
                    return Err(OutputInitError::EncoderError(
                        EncoderInitError::VulkanContextRequiredForVulkanEncoder,
                    ));
                }
                (
                    VideoEncoderThread::<VulkanH264Encoder>::spawn(
                        output_id.clone(),
                        VideoEncoderThreadOptions {
                            ctx: ctx.clone(),
                            encoder_options: options.clone(),
                            chunks_sender,
                        },
                    )?,
                    OutputVideoCodec::H264,
                )
            }
            VideoEncoderOptions::FfmpegVp9(options) => (
                VideoEncoderThread::<FfmpegVp9Encoder>::spawn(
                    output_id.clone(),
                    VideoEncoderThreadOptions {
                        ctx: ctx.clone(),
                        encoder_options: options.clone(),
                        chunks_sender,
                    },
                )?,
                OutputVideoCodec::Vp9,
            ),
            VideoEncoderOptions::FfmpegVp8(_) => {
                return Err(OutputInitError::UnsupportedVideoCodec(VideoCodec::Vp8));
            }
        };

        let extradata = encoder.encoder_context();
        Ok((
            encoder,
            VideoConfig {
                codec,
                extradata,
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

        let (chunks_sender, chunks_receiver) = bounded(1000);
        let (encoder, codec) = match options {
            AudioEncoderOptions::FdkAac(options) => (
                AudioEncoderThread::<FdkAacEncoder>::spawn(
                    output_id.clone(),
                    AudioEncoderThreadOptions {
                        ctx: ctx.clone(),
                        encoder_options: options,
                        chunks_sender,
                    },
                )?,
                OutputAudioCodec::Aac,
            ),
            AudioEncoderOptions::Opus(options) => (
                AudioEncoderThread::<OpusEncoder>::spawn(
                    output_id.clone(),
                    AudioEncoderThreadOptions {
                        ctx: ctx.clone(),
                        encoder_options: options,
                        chunks_sender,
                    },
                )?,
                OutputAudioCodec::Opus,
            ),
        };
        let extradata = encoder.encoder_context();

        Ok((
            encoder,
            AudioConfig {
                codec,
                extradata,
                channels,
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

fn video_chunk_to_event(chunk: EncodedOutputChunk, codec: OutputVideoCodec) -> rtmp::RtmpEvent {
    match codec {
        OutputVideoCodec::H264 => H264VideoData {
            pts: chunk.pts,
            dts: chunk.dts.unwrap_or(chunk.pts),
            data: chunk.data,
            is_keyframe: chunk.is_keyframe,
        }
        .into(),
        OutputVideoCodec::Vp9 => Vp9VideoData {
            pts: chunk.pts,
            dts: chunk.dts.unwrap_or(chunk.pts),
            data: chunk.data,
            is_keyframe: chunk.is_keyframe,
        }
        .into(),
    }
}

fn audio_chunk_to_event(
    chunk: EncodedOutputChunk,
    channels: rtmp::AudioChannels,
    codec: OutputAudioCodec,
) -> rtmp::RtmpEvent {
    match codec {
        OutputAudioCodec::Aac => AacAudioData {
            pts: chunk.pts,
            channels,
            data: chunk.data,
        }
        .into(),
        OutputAudioCodec::Opus => OpusAudioData {
            pts: chunk.pts,
            data: chunk.data,
        }
        .into(),
    }
}

fn run_rtmp_output_thread(
    mut client: RtmpClient,
    video_config: Option<VideoConfig>,
    audio_config: Option<AudioConfig>,
    stats_sender: RtmpOutputStatsSender,
) -> Result<(), RtmpStreamError> {
    let channels = match audio_config.as_ref().map(|config| config.channels) {
        Some(AudioChannels::Mono) => rtmp::AudioChannels::Mono,
        Some(AudioChannels::Stereo) | None => rtmp::AudioChannels::Stereo,
    };

    let video_codec = video_config.as_ref().map(|c| c.codec);
    let audio_codec = audio_config.as_ref().map(|c| c.codec);

    match (video_config, audio_config) {
        (Some(video), Some(audio)) => run_synced_av(
            &mut client,
            channels,
            video_codec.unwrap(),
            audio_codec.unwrap(),
            &video.chunks_receiver,
            &audio.chunks_receiver,
            stats_sender,
        ),
        (Some(video), None) => {
            let codec = video_codec.unwrap();
            while let Ok(EncodedOutputEvent::Data(chunk)) = video.chunks_receiver.recv() {
                stats_sender.bytes_sent_event(chunk.data.len(), StatsTrackKind::Video);
                client.send(video_chunk_to_event(chunk, codec))?;
            }
            Ok(())
        }
        (None, Some(audio)) => {
            let codec = audio_codec.unwrap();
            while let Ok(EncodedOutputEvent::Data(chunk)) = audio.chunks_receiver.recv() {
                stats_sender.bytes_sent_event(chunk.data.len(), StatsTrackKind::Audio);
                client.send(audio_chunk_to_event(chunk, channels, codec))?;
            }
            Ok(())
        }
        (None, None) => Ok(()),
    }
}

fn run_synced_av(
    client: &mut RtmpClient,
    channels: rtmp::AudioChannels,
    video_codec: OutputVideoCodec,
    audio_codec: OutputAudioCodec,
    video_rx: &Receiver<EncodedOutputEvent>,
    audio_rx: &Receiver<EncodedOutputEvent>,
    rtmp_stats_sender: RtmpOutputStatsSender,
) -> Result<(), RtmpStreamError> {
    let mut pending_video: Option<EncodedOutputChunk> = None;
    let mut pending_audio: Option<EncodedOutputChunk> = None;
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
                    if video.pts <= audio.pts {
                        rtmp_stats_sender.bytes_sent_event(video.data.len(), StatsTrackKind::Video);
                        client.send(video_chunk_to_event(
                            pending_video.take().unwrap(),
                            video_codec,
                        ))?;
                    } else {
                        rtmp_stats_sender.bytes_sent_event(audio.data.len(), StatsTrackKind::Audio);
                        client.send(audio_chunk_to_event(
                            pending_audio.take().unwrap(),
                            channels,
                            audio_codec,
                        ))?;
                    }
                }
                (Some(video), None) => {
                    rtmp_stats_sender.bytes_sent_event(video.data.len(), StatsTrackKind::Video);
                    client.send(video_chunk_to_event(
                        pending_video.take().unwrap(),
                        video_codec,
                    ))?;
                }
                (None, Some(audio)) => {
                    rtmp_stats_sender.bytes_sent_event(audio.data.len(), StatsTrackKind::Audio);
                    client.send(audio_chunk_to_event(
                        pending_audio.take().unwrap(),
                        channels,
                        audio_codec,
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
