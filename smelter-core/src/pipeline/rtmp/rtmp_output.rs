use std::{net::ToSocketAddrs, sync::Arc};

use crossbeam_channel::{Receiver, bounded};
use tracing::{debug, error};

use rtmp::{RtmpClient, RtmpClientConfig};

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
        utils::{annexb_to_avcc, build_avc_decoder_config},
    },
    thread_utils::InitializableThread,
};

use crate::prelude::*;

pub struct RtmpClientOutput {
    video: Option<VideoEncoderThreadHandle>,
    audio: Option<AudioEncoderThreadHandle>,
}

impl RtmpClientOutput {
    pub fn new(
        ctx: Arc<PipelineCtx>,
        output_ref: Ref<OutputId>,
        options: RtmpOutputOptions,
    ) -> Result<Self, OutputInitError> {
        let (encoded_chunks_sender, encoded_chunks_receiver) = bounded(1000);

        let video_extradata = match &options.video {
            Some(video) => {
                let (encoder, extradata) = Self::init_video_encoder(
                    &ctx,
                    &output_ref,
                    video.clone(),
                    encoded_chunks_sender.clone(),
                )?;
                Some((encoder, extradata))
            }
            None => None,
        };

        let audio_extradata = match &options.audio {
            Some(audio) => {
                let (encoder, extradata) = Self::init_audio_encoder(
                    &ctx,
                    &output_ref,
                    audio.clone(),
                    encoded_chunks_sender.clone(),
                )?;
                Some((encoder, extradata))
            }
            None => None,
        };

        let (video_encoder, has_video) = match video_extradata {
            Some((encoder, _extradata)) => (Some(encoder), true),
            None => (None, false),
        };
        let (audio_encoder, audio_config) = match audio_extradata {
            Some((encoder, extradata)) => (Some(encoder), Some(extradata)),
            None => (None, None),
        };
        let audio_channels = options
            .audio
            .as_ref()
            .map(|a| a.channels())
            .unwrap_or(AudioChannels::Stereo);
        let audio_sample_rate = options
            .audio
            .as_ref()
            .map(|a| a.sample_rate())
            .unwrap_or(44_100);

        let url = options.url.clone();

        let output_ref_clone = output_ref.clone();
        let ctx_clone = ctx.clone();
        std::thread::Builder::new()
            .name(format!("RTMP sender thread for output {output_ref}"))
            .spawn(move || {
                let _span =
                    tracing::info_span!("RTMP sender", output_id = output_ref_clone.to_string())
                        .entered();

                run_rtmp_output_thread(
                    &url,
                    has_video,
                    audio_config,
                    audio_channels,
                    audio_sample_rate,
                    encoded_chunks_receiver,
                );
                ctx_clone
                    .event_emitter
                    .emit(Event::OutputDone(output_ref_clone.id().clone()));
                debug!("Closing RTMP sender thread.");
            })
            .unwrap();

        Ok(Self {
            video: video_encoder,
            audio: audio_encoder,
        })
    }

    fn init_video_encoder(
        ctx: &Arc<PipelineCtx>,
        output_id: &Ref<OutputId>,
        options: VideoEncoderOptions,
        encoded_chunks_sender: crossbeam_channel::Sender<EncodedOutputEvent>,
    ) -> Result<(VideoEncoderThreadHandle, Option<bytes::Bytes>), OutputInitError> {
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

        let extradata = encoder.encoder_context();
        Ok((encoder, extradata))
    }

    fn init_audio_encoder(
        ctx: &Arc<PipelineCtx>,
        output_id: &Ref<OutputId>,
        options: AudioEncoderOptions,
        encoded_chunks_sender: crossbeam_channel::Sender<EncodedOutputEvent>,
    ) -> Result<(AudioEncoderThreadHandle, Option<bytes::Bytes>), OutputInitError> {
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

        let extradata = encoder.encoder_context();
        Ok((encoder, extradata))
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

fn parse_rtmp_url(url: &str) -> Option<(std::net::SocketAddr, String, String)> {
    let url = url::Url::parse(url).ok()?;
    let host = url.host_str()?;
    let port = url.port().unwrap_or(1935);
    let addr = format!("{host}:{port}").to_socket_addrs().ok()?.next()?;

    let mut path_segments: Vec<&str> = url.path().trim_start_matches('/').splitn(2, '/').collect();
    let stream_key = path_segments.pop().unwrap_or("").to_string();
    let app = path_segments.pop().unwrap_or("").to_string();

    Some((addr, app, stream_key))
}

fn run_rtmp_output_thread(
    url: &str,
    has_video: bool,
    audio_extradata: Option<Option<bytes::Bytes>>,
    audio_channels: AudioChannels,
    audio_sample_rate: u32,
    packets_receiver: Receiver<EncodedOutputEvent>,
) {
    let (addr, app, stream_key) = match parse_rtmp_url(url) {
        Some(parsed) => parsed,
        None => {
            error!("Failed to parse RTMP URL: {url}");
            return;
        }
    };

    let config = RtmpClientConfig {
        addr,
        app,
        stream_key,
    };

    let mut client = match RtmpClient::connect(config) {
        Ok(client) => client,
        Err(err) => {
            error!(%err, "Failed to connect RTMP client");
            return;
        }
    };

    if let Some(Some(extradata)) = &audio_extradata {
        let channels = match audio_channels {
            AudioChannels::Mono => rtmp::AudioChannels::Mono,
            AudioChannels::Stereo => rtmp::AudioChannels::Stereo,
        };
        let config = rtmp::AudioConfig {
            codec: rtmp::AudioCodec::Aac,
            sample_rate: audio_sample_rate,
            channels,
            data: extradata.clone(),
        };
        if let Err(err) = client.send_audio_config(&config) {
            error!(%err, "Failed to send audio config");
            return;
        }
    }

    let mut received_video_eos = if has_video { Some(false) } else { None };
    let mut received_audio_eos = audio_extradata.as_ref().map(|_| false);
    let mut sent_video_config = false;

    for packet in packets_receiver {
        match packet {
            EncodedOutputEvent::Data(chunk) => {
                if let Err(err) = send_chunk(
                    &mut client,
                    chunk,
                    audio_channels,
                    audio_sample_rate,
                    &mut sent_video_config,
                ) {
                    error!(%err, "Failed to send RTMP data");
                    break;
                }
            }
            EncodedOutputEvent::VideoEOS => match received_video_eos {
                Some(false) => received_video_eos = Some(true),
                Some(true) => {
                    error!("Received multiple video EOS events.");
                }
                None => {
                    error!("Received video EOS event on non video output.");
                }
            },
            EncodedOutputEvent::AudioEOS => match received_audio_eos {
                Some(false) => received_audio_eos = Some(true),
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
}

fn send_chunk(
    client: &mut RtmpClient,
    chunk: EncodedOutputChunk,
    audio_channels: AudioChannels,
    audio_sample_rate: u32,
    sent_video_config: &mut bool,
) -> Result<(), rtmp::RtmpError> {
    let dts_ms = chunk.dts.unwrap_or(chunk.pts).as_millis() as i64;
    let pts_ms = chunk.pts.as_millis() as i64;

    match chunk.kind {
        MediaKind::Video(_) => {
            // Encoder produces Annex B. On the first keyframe extract SPS/PPS
            // and send the AVC decoder config before any video data.
            if !*sent_video_config
                && chunk.is_keyframe
                && let Some(avc_config) = build_avc_decoder_config(&chunk.data)
            {
                let config = rtmp::VideoConfig {
                    codec: rtmp::VideoCodec::H264,
                    data: avc_config,
                };
                client.send_video_config(&config)?;
                *sent_video_config = true;
            }

            let frame_type = if chunk.is_keyframe {
                rtmp::VideoFrameType::Keyframe
            } else {
                rtmp::VideoFrameType::Interframe
            };
            let composition_time = Some((pts_ms - dts_ms) as i32);
            let avcc_data = annexb_to_avcc(&chunk.data);
            client.send_video(&rtmp::VideoData {
                pts: pts_ms,
                dts: dts_ms,
                codec: rtmp::VideoCodec::H264,
                frame_type,
                composition_time,
                data: avcc_data,
            })
        }
        MediaKind::Audio(_) => {
            let channels = match audio_channels {
                AudioChannels::Mono => rtmp::AudioChannels::Mono,
                AudioChannels::Stereo => rtmp::AudioChannels::Stereo,
            };
            client.send_audio(&rtmp::AudioData {
                pts: pts_ms,
                dts: dts_ms,
                codec: rtmp::AudioCodec::Aac,
                sample_rate: audio_sample_rate,
                channels,
                data: chunk.data,
            })
        }
    }
}
