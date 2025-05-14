use axum::http::HeaderValue;
use compositor_pipeline::{
    audio_mixer::AudioChannels,
    pipeline::{
        self,
        encoder::{
            self,
            fdk_aac::AacEncoderOptions,
            ffmpeg_h264::{self},
            ffmpeg_vp8, ffmpeg_vp9,
            opus::OpusEncoderOptions,
            AudioEncoderOptions,
        },
        output::{
            self,
            mp4::Mp4OutputOptions,
            rtmp::RtmpSenderOptions,
            whip::{AudioWhipOptions, VideoWhipOptions},
        },
    },
};
use itertools::Itertools;
use tracing::warn;

use super::register_output::*;
use super::util::*;
use super::*;

impl TryFrom<RtpOutput> for pipeline::RegisterOutputOptions<output::OutputOptions> {
    type Error = TypeError;

    fn try_from(request: RtpOutput) -> Result<Self, Self::Error> {
        let RtpOutput {
            port,
            ip,
            transport_protocol,
            video,
            audio,
        } = request;

        if video.is_none() && audio.is_none() {
            return Err(TypeError::new(
                "At least one of \"video\" and \"audio\" fields have to be specified.",
            ));
        }

        let (video_encoder_options, output_video_options) = maybe_video_options(video)?;
        let (audio_encoder_options, output_audio_options) = match audio {
            Some(OutputRtpAudioOptions {
                mixing_strategy,
                send_eos_when,
                encoder,
                channels,
                initial,
            }) => {
                let (audio_encoder_options, resolved_channels) = match encoder {
                    RtpAudioEncoderOptions::Opus {
                        preset,
                        sample_rate,
                        channels: channels_deprecated,
                    } => {
                        if channels_deprecated.is_some() {
                            warn!("The 'channels' field within the encoder options is deprecated and will be removed in future releases. Please use the 'channels' field in the audio options for setting the audio channels.");
                        }
                        let resolved_channels = channels
                            .or(channels_deprecated)
                            .unwrap_or(audio::AudioChannels::Stereo);

                        (
                            AudioEncoderOptions::Opus(OpusEncoderOptions {
                                channels: resolved_channels.clone().into(),
                                preset: preset.unwrap_or(OpusEncoderPreset::Voip).into(),
                                sample_rate: sample_rate.unwrap_or(48000),
                            }),
                            resolved_channels,
                        )
                    }
                };
                let output_audio_options = pipeline::OutputAudioOptions {
                    initial: initial.try_into()?,
                    end_condition: send_eos_when.unwrap_or_default().try_into()?,
                    mixing_strategy: mixing_strategy.unwrap_or(MixingStrategy::SumClip).into(),
                    channels: resolved_channels.into(),
                };

                (Some(audio_encoder_options), Some(output_audio_options))
            }
            None => (None, None),
        };

        let connection_options = match transport_protocol.unwrap_or(TransportProtocol::Udp) {
            TransportProtocol::Udp => {
                let pipeline::rtp::RequestedPort::Exact(port) = port.try_into()? else {
                    return Err(TypeError::new(
                        "Port range can not be used with UDP output stream (transport_protocol=\"udp\").",
                    ));
                };
                let Some(ip) = ip else {
                    return Err(TypeError::new(
                        "\"ip\" field is required when registering output UDP stream (transport_protocol=\"udp\").",
                    ));
                };
                output::rtp::RtpConnectionOptions::Udp {
                    port: pipeline::Port(port),
                    ip,
                }
            }
            TransportProtocol::TcpServer => {
                if ip.is_some() {
                    return Err(TypeError::new(
                        "\"ip\" field is not allowed when registering TCP server connection (transport_protocol=\"tcp_server\").",
                    ));
                }

                output::rtp::RtpConnectionOptions::TcpServer {
                    port: port.try_into()?,
                }
            }
        };

        let output_options = output::OutputOptions::Rtp(output::rtp::RtpSenderOptions {
            connection_options,
            video: video_encoder_options,
            audio: audio_encoder_options,
        });

        Ok(Self {
            output_options,
            video: output_video_options,
            audio: output_audio_options,
        })
    }
}

impl TryFrom<Mp4Output> for pipeline::RegisterOutputOptions<output::OutputOptions> {
    type Error = TypeError;

    fn try_from(request: Mp4Output) -> Result<Self, Self::Error> {
        let Mp4Output { path, video, audio } = request;

        if video.is_none() && audio.is_none() {
            return Err(TypeError::new(
                "At least one of \"video\" and \"audio\" fields have to be specified.",
            ));
        }

        let (video_encoder_options, output_video_options) = maybe_video_options_h264_only(video)?;
        let (audio_encoder_options, output_audio_options) = match audio {
            Some(OutputMp4AudioOptions {
                mixing_strategy,
                send_eos_when,
                encoder,
                channels,
                initial,
            }) => {
                let (audio_encoder_options, resolved_channels) = match encoder {
                    Mp4AudioEncoderOptions::Aac {
                        sample_rate,
                        channels: channels_deprecated,
                    } => {
                        if channels_deprecated.is_some() {
                            warn!("The 'channels' field within the encoder options is deprecated and will be removed in future releases. Please use the 'channels' field in the audio options for setting the audio channels.");
                        }
                        let resolved_channels = channels
                            .or(channels_deprecated)
                            .unwrap_or(audio::AudioChannels::Stereo);
                        (
                            AudioEncoderOptions::Aac(AacEncoderOptions {
                                channels: resolved_channels.clone().into(),
                                sample_rate: sample_rate.unwrap_or(44100),
                            }),
                            resolved_channels,
                        )
                    }
                };
                let output_audio_options = pipeline::OutputAudioOptions {
                    initial: initial.try_into()?,
                    end_condition: send_eos_when.unwrap_or_default().try_into()?,
                    mixing_strategy: mixing_strategy.unwrap_or(MixingStrategy::SumClip).into(),
                    channels: resolved_channels.into(),
                };

                (Some(audio_encoder_options), Some(output_audio_options))
            }
            None => (None, None),
        };

        let output_options = output::OutputOptions::Mp4(Mp4OutputOptions {
            output_path: path.into(),
            video: video_encoder_options,
            audio: audio_encoder_options,
        });

        Ok(Self {
            output_options,
            video: output_video_options,
            audio: output_audio_options,
        })
    }
}

impl TryFrom<WhipOutput> for pipeline::RegisterOutputOptions<output::OutputOptions> {
    type Error = TypeError;

    fn try_from(request: WhipOutput) -> Result<Self, Self::Error> {
        let WhipOutput {
            endpoint_url,
            bearer_token,
            video,
            audio,
        } = request;

        if video.is_none() && audio.is_none() {
            return Err(TypeError::new(
                "At least one of \"video\" and \"audio\" fields have to be specified.",
            ));
        }

        if let Some(OutputWhipVideoOptions {
            encoder: Some(_encoder),
            ..
        }) = &video
        {
            warn!("Field 'encoder' is deprecated. The codec will now be set automatically based on WHIP negotiation; manual specification is no longer needed.")
        }

        if let Some(OutputWhipAudioOptions {
            encoder: Some(_encoder),
            ..
        }) = &audio
        {
            warn!("Field 'encoder' is deprecated. The codec will now be set automatically based on WHIP negotiation; manual specification is no longer needed.")
        }

        if let Some(token) = &bearer_token {
            if HeaderValue::from_str(format!("Bearer {token}").as_str()).is_err() {
                return Err(TypeError::new("Bearer token string is not valid. It must contain only 32-127 ASCII characters"));
            };
        }

        let (output_video_options, video_whip_options) = if let Some(options) = video {
            let output_options = pipeline::OutputVideoOptions {
                initial: options.initial.try_into()?,
                end_condition: options.send_eos_when.unwrap_or_default().try_into()?,
            };
            let encoder_preferences = match options.encoder_preferences.as_deref() {
                Some([]) | None => vec![WhipVideoEncoderOptions::Any],
                Some(v) => v.to_vec(),
            };

            let encoder_preferences: Vec<pipeline::encoder::VideoEncoderOptions> =
                encoder_preferences
                    .into_iter()
                    .flat_map(|codec| match codec {
                        WhipVideoEncoderOptions::FfmpegH264 {
                            preset,
                            ffmpeg_options,
                        } => {
                            vec![pipeline::encoder::VideoEncoderOptions::H264(
                                ffmpeg_h264::Options {
                                    preset: preset.unwrap_or(H264EncoderPreset::Fast).into(),
                                    resolution: options.resolution.clone().into(),
                                    raw_options: ffmpeg_options
                                        .unwrap_or_default()
                                        .into_iter()
                                        .collect(),
                                },
                            )]
                        }
                        WhipVideoEncoderOptions::FfmpegVp8 { ffmpeg_options } => {
                            vec![pipeline::encoder::VideoEncoderOptions::VP8(
                                ffmpeg_vp8::Options {
                                    resolution: options.resolution.clone().into(),
                                    raw_options: ffmpeg_options
                                        .unwrap_or_default()
                                        .into_iter()
                                        .collect(),
                                },
                            )]
                        }
                        WhipVideoEncoderOptions::FfmpegVp9 { ffmpeg_options } => {
                            vec![pipeline::encoder::VideoEncoderOptions::VP9(
                                ffmpeg_vp9::Options {
                                    resolution: options.resolution.clone().into(),
                                    raw_options: ffmpeg_options
                                        .unwrap_or_default()
                                        .into_iter()
                                        .collect(),
                                },
                            )]
                        }
                        WhipVideoEncoderOptions::Any => {
                            vec![
                                pipeline::encoder::VideoEncoderOptions::VP9(ffmpeg_vp9::Options {
                                    resolution: options.resolution.clone().into(),
                                    raw_options: Vec::new(),
                                }),
                                pipeline::encoder::VideoEncoderOptions::VP8(ffmpeg_vp8::Options {
                                    resolution: options.resolution.clone().into(),
                                    raw_options: Vec::new(),
                                }),
                                pipeline::encoder::VideoEncoderOptions::H264(
                                    ffmpeg_h264::Options {
                                        preset: H264EncoderPreset::Fast.into(),
                                        resolution: options.resolution.clone().into(),
                                        raw_options: Vec::new(),
                                    },
                                ),
                            ]
                        }
                    })
                    .unique()
                    .collect();

            let video_whip_options = VideoWhipOptions {
                encoder_preferences,
            };
            (Some(output_options), Some(video_whip_options))
        } else {
            (None, None)
        };

        let (output_audio_options, audio_whip_options) = match audio {
            Some(OutputWhipAudioOptions {
                mixing_strategy,
                send_eos_when,
                encoder,
                channels,
                encoder_preferences,
                initial,
            }) => {
                let resolved_channels = match encoder {
                    Some(WhipAudioEncoderOptions::Opus {
                        channels: channels_deprecated,
                        ..
                    }) => {
                        if channels_deprecated.is_some() {
                            warn!("The 'channels' field within the encoder options is deprecated and will be removed in future releases. Please use the 'channels' field in the audio options for setting the audio channels.");
                        }
                        channels
                            .or(channels_deprecated)
                            .unwrap_or(audio::AudioChannels::Stereo)
                    }
                    _ => channels.unwrap_or(audio::AudioChannels::Stereo),
                };
                let output_audio_options = pipeline::OutputAudioOptions {
                    initial: initial.try_into()?,
                    end_condition: send_eos_when.unwrap_or_default().try_into()?,
                    mixing_strategy: mixing_strategy.unwrap_or(MixingStrategy::SumClip).into(),
                    channels: resolved_channels.clone().into(),
                };

                let encoder_preferences = match encoder_preferences.as_deref() {
                    Some([]) | None => vec![WhipAudioEncoderOptions::Any],
                    Some(v) => v.to_vec(),
                };

                let encoder_preferences: Vec<pipeline::encoder::AudioEncoderOptions> =
                    encoder_preferences
                        .into_iter()
                        .flat_map(|codec| match codec {
                            WhipAudioEncoderOptions::Opus {
                                preset,
                                sample_rate,
                                ..
                            } => vec![pipeline::encoder::AudioEncoderOptions::Opus(
                                OpusEncoderOptions {
                                    channels: resolved_channels.clone().into(),
                                    preset: preset.unwrap_or(OpusEncoderPreset::Voip).into(),
                                    sample_rate: sample_rate.unwrap_or(48000),
                                },
                            )],
                            WhipAudioEncoderOptions::Any => {
                                vec![pipeline::encoder::AudioEncoderOptions::Opus(
                                    OpusEncoderOptions {
                                        channels: resolved_channels.clone().into(),
                                        preset: OpusEncoderPreset::Voip.into(),
                                        sample_rate: 48000,
                                    },
                                )]
                            }
                        })
                        .unique()
                        .collect();

                let audio_whip_options = AudioWhipOptions {
                    encoder_preferences,
                };
                (Some(output_audio_options), Some(audio_whip_options))
            }
            None => {
                // even if audio field is unregistered add opus codec in order to make Twitch work
                let audio_whip_options = AudioWhipOptions {
                    encoder_preferences: vec![pipeline::encoder::AudioEncoderOptions::Opus(
                        OpusEncoderOptions {
                            channels: AudioChannels::Stereo,
                            preset: OpusEncoderPreset::Voip.into(),
                            sample_rate: 48000,
                        },
                    )],
                };
                (None, Some(audio_whip_options))
            }
        };

        let output_options = output::OutputOptions::Whip(output::whip::WhipSenderOptions {
            endpoint_url,
            bearer_token,
            video: video_whip_options,
            audio: audio_whip_options,
        });

        Ok(Self {
            output_options,
            video: output_video_options,
            audio: output_audio_options,
        })
    }
}

impl TryFrom<RtmpClient> for pipeline::RegisterOutputOptions<output::OutputOptions> {
    type Error = TypeError;

    fn try_from(value: RtmpClient) -> Result<Self, Self::Error> {
        let RtmpClient { url, video, audio } = value;

        if video.is_none() && audio.is_none() {
            return Err(TypeError::new(
                "At least one of \"video\" and \"audio\" fields have to be specified.",
            ));
        }

        let (video_encoder_options, output_video_options) = maybe_video_options_h264_only(video)?;
        let (audio_encoder_options, output_audio_options) = match audio {
            Some(OutputRtmpClientAudioOptions {
                mixing_strategy,
                send_eos_when,
                encoder,
                channels,
                initial,
            }) => {
                let (audio_encoder_options, resolved_channels) = match encoder {
                    RtmpClientAudioEncoderOptions::Aac {
                        sample_rate,
                        channels: channels_deprecated,
                    } => {
                        if channels_deprecated.is_some() {
                            warn!("The 'channels' field within the encoder options is deprecated and will be removed in future releases. Please use the 'channels' field in the audio options for setting the audio channels.");
                        }
                        let resolved_channels = channels
                            .or(channels_deprecated)
                            .unwrap_or(audio::AudioChannels::Stereo);
                        (
                            AudioEncoderOptions::Aac(AacEncoderOptions {
                                channels: resolved_channels.clone().into(),
                                sample_rate: sample_rate.unwrap_or(44100),
                            }),
                            resolved_channels,
                        )
                    }
                };
                let output_audio_options = pipeline::OutputAudioOptions {
                    initial: initial.try_into()?,
                    end_condition: send_eos_when.unwrap_or_default().try_into()?,
                    mixing_strategy: mixing_strategy.unwrap_or(MixingStrategy::SumClip).into(),
                    channels: resolved_channels.into(),
                };

                (Some(audio_encoder_options), Some(output_audio_options))
            }
            None => (None, None),
        };

        let output_options = output::OutputOptions::Rtmp(RtmpSenderOptions {
            url,
            video: video_encoder_options,
            audio: audio_encoder_options,
        });

        Ok(Self {
            output_options,
            video: output_video_options,
            audio: output_audio_options,
        })
    }
}

fn maybe_video_options(
    options: Option<OutputVideoOptions>,
) -> Result<
    (
        Option<pipeline::encoder::VideoEncoderOptions>,
        Option<pipeline::OutputVideoOptions>,
    ),
    TypeError,
> {
    let Some(options) = options else {
        return Ok((None, None));
    };

    let encoder_options = match options.encoder {
        VideoEncoderOptions::FfmpegH264 {
            preset,
            ffmpeg_options,
        } => pipeline::encoder::VideoEncoderOptions::H264(ffmpeg_h264::Options {
            preset: preset.unwrap_or(H264EncoderPreset::Fast).into(),
            resolution: options.resolution.into(),
            raw_options: ffmpeg_options.unwrap_or_default().into_iter().collect(),
        }),
        VideoEncoderOptions::FfmpegVp8 { ffmpeg_options } => {
            pipeline::encoder::VideoEncoderOptions::VP8(ffmpeg_vp8::Options {
                resolution: options.resolution.into(),
                raw_options: ffmpeg_options.unwrap_or_default().into_iter().collect(),
            })
        }
        VideoEncoderOptions::FfmpegVp9 { ffmpeg_options } => {
            pipeline::encoder::VideoEncoderOptions::VP9(ffmpeg_vp9::Options {
                resolution: options.resolution.into(),
                raw_options: ffmpeg_options.unwrap_or_default().into_iter().collect(),
            })
        }
    };

    let output_options = pipeline::OutputVideoOptions {
        initial: options.initial.try_into()?,
        end_condition: options.send_eos_when.unwrap_or_default().try_into()?,
    };

    Ok((Some(encoder_options), Some(output_options)))
}

fn maybe_video_options_h264_only(
    options: Option<OutputVideoOptions>,
) -> Result<
    (
        Option<pipeline::encoder::VideoEncoderOptions>,
        Option<pipeline::OutputVideoOptions>,
    ),
    TypeError,
> {
    let Some(options) = options else {
        return Ok((None, None));
    };

    let encoder_options = match options.encoder {
        VideoEncoderOptions::FfmpegH264 {
            preset,
            ffmpeg_options,
        } => pipeline::encoder::VideoEncoderOptions::H264(ffmpeg_h264::Options {
            preset: preset.unwrap_or(H264EncoderPreset::Fast).into(),
            resolution: options.resolution.into(),
            raw_options: ffmpeg_options.unwrap_or_default().into_iter().collect(),
        }),
        VideoEncoderOptions::FfmpegVp8 { .. } => {
            return Err(TypeError::new(
                "VP8 output not supported for given protocol",
            ));
        }
        VideoEncoderOptions::FfmpegVp9 { .. } => {
            return Err(TypeError::new(
                "VP9 output not supported for given protocol",
            ));
        }
    };

    let output_options = pipeline::OutputVideoOptions {
        initial: options.initial.try_into()?,
        end_condition: options.send_eos_when.unwrap_or_default().try_into()?,
    };

    Ok((Some(encoder_options), Some(output_options)))
}

impl TryFrom<OutputEndCondition> for pipeline::PipelineOutputEndCondition {
    type Error = TypeError;

    fn try_from(value: OutputEndCondition) -> Result<Self, Self::Error> {
        match value {
            OutputEndCondition {
                any_of: Some(any_of),
                all_of: None,
                any_input: None,
                all_inputs: None,
            } => Ok(pipeline::PipelineOutputEndCondition::AnyOf(
                any_of.into_iter().map(Into::into).collect(),
            )),
            OutputEndCondition {
                any_of: None,
                all_of: Some(all_of),
                any_input: None,
                all_inputs: None,
            } => Ok(pipeline::PipelineOutputEndCondition::AllOf(
                all_of.into_iter().map(Into::into).collect(),
            )),
            OutputEndCondition {
                any_of: None,
                all_of: None,
                any_input: Some(true),
                all_inputs: None,
            } => Ok(pipeline::PipelineOutputEndCondition::AnyInput),
            OutputEndCondition {
                any_of: None,
                all_of: None,
                any_input: None,
                all_inputs: Some(true),
            } => Ok(pipeline::PipelineOutputEndCondition::AllInputs),
            OutputEndCondition {
                any_of: None,
                all_of: None,
                any_input: None | Some(false),
                all_inputs: None | Some(false),
            } => Ok(pipeline::PipelineOutputEndCondition::Never),
            _ => Err(TypeError::new(
                "Only one of \"any_of, all_of, any_input or all_inputs\" is allowed.",
            )),
        }
    }
}

impl From<H264EncoderPreset> for encoder::ffmpeg_h264::EncoderPreset {
    fn from(value: H264EncoderPreset) -> Self {
        match value {
            H264EncoderPreset::Ultrafast => ffmpeg_h264::EncoderPreset::Ultrafast,
            H264EncoderPreset::Superfast => ffmpeg_h264::EncoderPreset::Superfast,
            H264EncoderPreset::Veryfast => ffmpeg_h264::EncoderPreset::Veryfast,
            H264EncoderPreset::Faster => ffmpeg_h264::EncoderPreset::Faster,
            H264EncoderPreset::Fast => ffmpeg_h264::EncoderPreset::Fast,
            H264EncoderPreset::Medium => ffmpeg_h264::EncoderPreset::Medium,
            H264EncoderPreset::Slow => ffmpeg_h264::EncoderPreset::Slow,
            H264EncoderPreset::Slower => ffmpeg_h264::EncoderPreset::Slower,
            H264EncoderPreset::Veryslow => ffmpeg_h264::EncoderPreset::Veryslow,
            H264EncoderPreset::Placebo => ffmpeg_h264::EncoderPreset::Placebo,
        }
    }
}

impl From<OpusEncoderPreset> for encoder::AudioEncoderPreset {
    fn from(value: OpusEncoderPreset) -> Self {
        match value {
            OpusEncoderPreset::Quality => encoder::AudioEncoderPreset::Quality,
            OpusEncoderPreset::Voip => encoder::AudioEncoderPreset::Voip,
            OpusEncoderPreset::LowestLatency => encoder::AudioEncoderPreset::LowestLatency,
        }
    }
}
