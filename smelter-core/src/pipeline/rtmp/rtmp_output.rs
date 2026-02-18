use std::sync::Arc;

use bytes::Bytes;
use crossbeam_channel::{Receiver, Sender, bounded};
use smelter_render::error::ErrorStack;
use tracing::{debug, error, warn};

use rtmp::{
    AacAudioConfig, AacAudioData, H264VideoConfig, H264VideoData, RtmpClient, RtmpClientConfig,
    RtmpError,
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
    thread_utils::InitializableThread,
};

use crate::prelude::*;

pub struct RtmpClientOutput {
    video: Option<VideoEncoderThreadHandle>,
    audio: Option<AudioEncoderThreadHandle>,
}

struct AudioConfig {
    extradata: Bytes,
    channels: AudioChannels,
}

struct VideoConfig {
    extradata: Bytes,
}

impl RtmpClientOutput {
    pub fn new(
        ctx: Arc<PipelineCtx>,
        output_ref: Ref<OutputId>,
        options: RtmpOutputOptions,
    ) -> Result<Self, OutputInitError> {
        let (encoded_chunks_sender, encoded_chunks_receiver) = bounded(1000);

        let (video_encoder, video_config) = match &options.video {
            Some(video) => {
                let (encoder, config) = Self::init_video_encoder(
                    &ctx,
                    &output_ref,
                    video.clone(),
                    encoded_chunks_sender.clone(),
                )?;
                (Some(encoder), Some(config))
            }
            None => (None, None),
        };

        let (audio_encoder, audio_config) = match &options.audio {
            Some(audio) => {
                let (encoder, config) = Self::init_audio_encoder(
                    &ctx,
                    &output_ref,
                    audio.clone(),
                    encoded_chunks_sender.clone(),
                )?;
                (Some(encoder), Some(config))
            }
            None => (None, None),
        };

        let client = Self::establish_connection(options.connection, &video_config, &audio_config)?;
        std::thread::Builder::new()
            .name(format!("RTMP sender thread for output {output_ref}"))
            .spawn(move || {
                let _span = tracing::info_span!("RTMP sender", output_id = output_ref.to_string())
                    .entered();

                let result = run_rtmp_output_thread(
                    client,
                    video_config,
                    audio_config,
                    encoded_chunks_receiver,
                );
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
            addr: connection_opts.address,
            app: connection_opts.app,
            stream_key: connection_opts.stream_key,
        })?;

        if let Some(config) = video_config {
            client.send(H264VideoConfig {
                data: config.extradata.clone(),
            })?;
        }

        if let Some(config) = audio_config {
            let config = AacAudioConfig::new(config.extradata.clone());
            client.send(config)?;
        }
        Ok(client)
    }

    fn init_video_encoder(
        ctx: &Arc<PipelineCtx>,
        output_id: &Ref<OutputId>,
        options: VideoEncoderOptions,
        encoded_chunks_sender: Sender<EncodedOutputEvent>,
    ) -> Result<(VideoEncoderThreadHandle, VideoConfig), OutputInitError> {
        let encoder = match &options {
            VideoEncoderOptions::FfmpegH264(options) => {
                VideoEncoderThread::<FfmpegH264Encoder>::spawn(
                    output_id.clone(),
                    VideoEncoderThreadOptions {
                        ctx: ctx.clone(),
                        encoder_options: options.clone(),
                        chunks_sender: encoded_chunks_sender,
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
                        chunks_sender: encoded_chunks_sender,
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
        Ok((encoder, VideoConfig { extradata }))
    }

    fn init_audio_encoder(
        ctx: &Arc<PipelineCtx>,
        output_id: &Ref<OutputId>,
        options: AudioEncoderOptions,
        encoded_chunks_sender: Sender<EncodedOutputEvent>,
    ) -> Result<(AudioEncoderThreadHandle, AudioConfig), OutputInitError> {
        let channels = options.channels();
        let encoder = match options {
            AudioEncoderOptions::FdkAac(options) => AudioEncoderThread::<FdkAacEncoder>::spawn(
                output_id.clone(),
                AudioEncoderThreadOptions {
                    ctx: ctx.clone(),
                    encoder_options: options,
                    chunks_sender: encoded_chunks_sender,
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

fn run_rtmp_output_thread(
    mut client: RtmpClient,
    video_config: Option<VideoConfig>,
    audio_config: Option<AudioConfig>,
    packets_receiver: Receiver<EncodedOutputEvent>,
) -> Result<(), RtmpError> {
    let mut received_video_eos = video_config.as_ref().map(|_| false);
    let mut received_audio_eos = audio_config.as_ref().map(|_| false);

    let channels = match audio_config.as_ref().map(|config| config.channels) {
        Some(AudioChannels::Mono) => rtmp::AudioChannels::Mono,
        Some(AudioChannels::Stereo) | None => rtmp::AudioChannels::Stereo,
    };

    for packet in packets_receiver {
        match packet {
            EncodedOutputEvent::Data(chunk) => match chunk.kind {
                MediaKind::Video(_video) => client.send(H264VideoData {
                    pts: chunk.pts,
                    dts: chunk.dts.unwrap_or(chunk.pts),
                    data: chunk.data,
                    is_keyframe: chunk.is_keyframe,
                })?,
                MediaKind::Audio(_audio) => client.send(AacAudioData {
                    pts: chunk.pts,
                    channels,
                    data: chunk.data,
                })?,
            },
            EncodedOutputEvent::VideoEOS => match received_video_eos {
                Some(false) => {
                    received_video_eos = Some(true);
                }
                Some(true) => {
                    error!("Received multiple video EOS events.");
                }
                None => {
                    error!("Received video EOS event on non video output.");
                }
            },
            EncodedOutputEvent::AudioEOS => match received_audio_eos {
                Some(false) => {
                    received_audio_eos = Some(true);
                }
                Some(true) => {
                    error!("Received multiple audio EOS events.");
                }
                None => {
                    error!("Received audio EOS event on non audio output.");
                }
            },
        };

        if received_video_eos.unwrap_or(true) && received_audio_eos.unwrap_or(true) {
            break;
        }
    }
    Ok(())
}
