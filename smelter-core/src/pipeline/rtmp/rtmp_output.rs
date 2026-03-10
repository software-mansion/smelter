use std::sync::Arc;

use bytes::Bytes;
use crossbeam_channel::{Receiver, bounded, select};
use smelter_render::error::ErrorStack;
use tracing::{debug, warn};

use rtmp::{
    AacAudioConfig, AacAudioData, H264VideoConfig, H264VideoData, RtmpClient, RtmpClientConfig,
    RtmpStreamError,
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
    channels: AudioChannels,
    chunks_receiver: Receiver<EncodedOutputEvent>,
}

struct VideoConfig {
    extradata: Bytes,
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
            client.send(H264VideoConfig {
                data: config.extradata.clone(),
            })?;
        }
        if let Some(config) = audio_config {
            let config = AacAudioConfig::try_from(config.extradata.clone())?;
            client.send(config)?;
        }
        Ok(client)
    }

    fn init_video_encoder(
        ctx: &Arc<PipelineCtx>,
        output_id: &Ref<OutputId>,
        options: VideoEncoderOptions,
    ) -> Result<(VideoEncoderThreadHandle, VideoConfig), OutputInitError> {
        let (chunks_sender, chunks_receiver) = bounded(1000);

        let encoder = match &options {
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
            VideoEncoderOptions::FfmpegVp8(_) => {
                return Err(OutputInitError::UnsupportedVideoCodec(VideoCodec::Vp8));
            }
            VideoEncoderOptions::FfmpegVp9(_) => {
                return Err(OutputInitError::UnsupportedVideoCodec(VideoCodec::Vp9));
            }
        };

        let Some(extradata) = encoder.encoder_context() else {
            return Err(RtmpClientError::MissingH264DecoderConfig.into());
        };
        Ok((
            encoder,
            VideoConfig {
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
        let encoder = match options {
            AudioEncoderOptions::FdkAac(options) => AudioEncoderThread::<FdkAacEncoder>::spawn(
                output_id.clone(),
                AudioEncoderThreadOptions {
                    ctx: ctx.clone(),
                    encoder_options: options,
                    chunks_sender,
                },
            )?,
            AudioEncoderOptions::Opus(_) => {
                return Err(OutputInitError::UnsupportedAudioCodec(AudioCodec::Opus));
            }
        };
        let Some(extradata) = encoder.encoder_context() else {
            return Err(RtmpClientError::MissingAacDecoderConfig.into());
        };

        Ok((
            encoder,
            AudioConfig {
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

fn video_chunk_to_event(chunk: EncodedOutputChunk) -> H264VideoData {
    H264VideoData {
        pts: chunk.pts,
        dts: chunk.dts.unwrap_or(chunk.pts),
        data: chunk.data,
        is_keyframe: chunk.is_keyframe,
    }
}

fn audio_chunk_to_event(chunk: EncodedOutputChunk, channels: rtmp::AudioChannels) -> AacAudioData {
    AacAudioData {
        pts: chunk.pts,
        channels,
        data: chunk.data,
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

    match (video_config, audio_config) {
        (Some(video), Some(audio)) => run_synced_av(
            &mut client,
            channels,
            &video.chunks_receiver,
            &audio.chunks_receiver,
            stats_sender,
        ),
        (Some(video), None) => {
            while let Ok(EncodedOutputEvent::Data(chunk)) = video.chunks_receiver.recv() {
                let data_size = chunk.len();
                client.send(video_chunk_to_event(chunk))?;
                stats_sender.bytes_sent_event(data_size as u64, StatsTrackKind::Video);
            }
            Ok(())
        }
        (None, Some(audio)) => {
            while let Ok(EncodedOutputEvent::Data(chunk)) = audio.chunks_receiver.recv() {
                let data_size = chunk.len();
                client.send(audio_chunk_to_event(chunk, channels))?;
                stats_sender.bytes_sent_event(data_size as u64, StatsTrackKind::Audio);
            }
            Ok(())
        }
        (None, None) => Ok(()),
    }
}

fn run_synced_av(
    client: &mut RtmpClient,
    channels: rtmp::AudioChannels,
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
                        let data_size = video.len();
                        client.send(video_chunk_to_event(pending_video.take().unwrap()))?;
                        rtmp_stats_sender.bytes_sent_event(data_size as u64, StatsTrackKind::Video);
                    } else {
                        let data_size = audio.len();
                        client.send(audio_chunk_to_event(
                            pending_audio.take().unwrap(),
                            channels,
                        ))?;
                        rtmp_stats_sender.bytes_sent_event(data_size as u64, StatsTrackKind::Audio);
                    }
                }
                (Some(video), None) => {
                    let data_size = video.len();
                    client.send(video_chunk_to_event(pending_video.take().unwrap()))?;
                    rtmp_stats_sender.bytes_sent_event(data_size as u64, StatsTrackKind::Video);
                }
                (None, Some(audio)) => {
                    let data_size = audio.len();
                    client.send(audio_chunk_to_event(
                        pending_audio.take().unwrap(),
                        channels,
                    ))?;
                    rtmp_stats_sender.bytes_sent_event(data_size as u64, StatsTrackKind::Audio);
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
    fn bytes_sent_event(&self, size: u64, track_kind: StatsTrackKind) {
        self.stats_sender.send(
            RtmpOutputTrackStatsEvent::BytesSent(size).into_event(&self.output_ref, track_kind),
        );
    }
}
