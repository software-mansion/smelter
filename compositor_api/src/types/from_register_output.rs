use axum::http::HeaderValue;
use compositor_pipeline::pipeline::{
    self,
    encoder::{
        self,
        fdk_aac::AacEncoderOptions,
        ffmpeg_h264::{self},
        AudioEncoderOptions,
    },
    output::{
        self,
        mp4::{Mp4AudioTrack, Mp4OutputOptions, Mp4VideoTrack},
        rtmp::{RtmpAudioTrack, RtmpVideoTrack},
        whip::WhipAudioOptions,
    },
};

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
        let video_codec = video.as_ref().map(|v| match v.encoder {
            VideoEncoderOptions::FfmpegH264 { .. } => pipeline::VideoCodec::H264,
        });
        let audio_codec = audio.as_ref().map(|a| match a.encoder {
            RtpAudioEncoderOptions::Opus { .. } => pipeline::AudioCodec::Opus,
        });

        let (video_encoder_options, output_video_options) = maybe_video_options(video)?;
        let (audio_encoder_options, output_audio_options) = match audio {
            Some(OutputRtpAudioOptions {
                mixing_strategy,
                send_eos_when,
                encoder,
                initial,
            }) => {
                let audio_encoder_options: AudioEncoderOptions = encoder.into();
                let output_audio_options = pipeline::OutputAudioOptions {
                    initial: initial.try_into()?,
                    end_condition: send_eos_when.unwrap_or_default().try_into()?,
                    mixing_strategy: mixing_strategy.unwrap_or(MixingStrategy::SumClip).into(),
                    channels: audio_encoder_options.channels(),
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

        let output_options = output::OutputOptions {
            output_protocol: output::OutputProtocolOptions::Rtp(output::rtp::RtpSenderOptions {
                connection_options,
                video: video_codec,
                audio: audio_codec,
            }),
            video: video_encoder_options,
            audio: audio_encoder_options,
        };

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

        let mp4_video = video.as_ref().map(|v| match v.encoder {
            VideoEncoderOptions::FfmpegH264 { .. } => Mp4VideoTrack {
                codec: pipeline::VideoCodec::H264,
                width: v.resolution.width as u32,
                height: v.resolution.height as u32,
            },
        });
        let mp4_audio = audio.as_ref().map(|a| match &a.encoder {
            Mp4AudioEncoderOptions::Aac {
                channels,
                sample_rate,
            } => Mp4AudioTrack {
                channels: channels.clone().into(),
                sample_rate: sample_rate.unwrap_or(44100),
            },
        });

        let (video_encoder_options, output_video_options) = maybe_video_options(video)?;
        let (audio_encoder_options, output_audio_options) = match audio {
            Some(OutputMp4AudioOptions {
                mixing_strategy,
                send_eos_when,
                encoder,
                initial,
            }) => {
                let audio_encoder_options: AudioEncoderOptions = encoder.into();
                let output_audio_options = pipeline::OutputAudioOptions {
                    initial: initial.try_into()?,
                    end_condition: send_eos_when.unwrap_or_default().try_into()?,
                    mixing_strategy: mixing_strategy.unwrap_or(MixingStrategy::SumClip).into(),
                    channels: audio_encoder_options.channels(),
                };

                (Some(audio_encoder_options), Some(output_audio_options))
            }
            None => (None, None),
        };

        let output_options = output::OutputOptions {
            output_protocol: output::OutputProtocolOptions::Mp4(Mp4OutputOptions {
                output_path: path.into(),
                video: mp4_video,
                audio: mp4_audio,
            }),
            video: video_encoder_options,
            audio: audio_encoder_options,
        };

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
        let video_codec = video.as_ref().map(|v| match v.encoder {
            VideoEncoderOptions::FfmpegH264 { .. } => pipeline::VideoCodec::H264,
        });
        let audio_options = audio.as_ref().map(|a| match &a.encoder {
            WhipAudioEncoderOptions::Opus {
                channels,
                preset: _,
                sample_rate: _,
            } => WhipAudioOptions {
                codec: pipeline::AudioCodec::Opus,
                channels: match channels {
                    audio::AudioChannels::Mono => {
                        compositor_pipeline::audio_mixer::AudioChannels::Mono
                    }
                    audio::AudioChannels::Stereo => {
                        compositor_pipeline::audio_mixer::AudioChannels::Stereo
                    }
                },
            },
        });

        if let Some(token) = &bearer_token {
            if HeaderValue::from_str(format!("Bearer {token}").as_str()).is_err() {
                return Err(TypeError::new("Bearer token string is not valid. It must contain only 32-127 ASCII characters"));
            };
        }

        let (video_encoder_options, output_video_options) = maybe_video_options(video)?;
        let (audio_encoder_options, output_audio_options) = match audio {
            Some(OutputWhipAudioOptions {
                mixing_strategy,
                send_eos_when,
                encoder,
                initial,
            }) => {
                let audio_encoder_options: AudioEncoderOptions = encoder.into();
                let output_audio_options = pipeline::OutputAudioOptions {
                    initial: initial.try_into()?,
                    end_condition: send_eos_when.unwrap_or_default().try_into()?,
                    mixing_strategy: mixing_strategy.unwrap_or(MixingStrategy::SumClip).into(),
                    channels: audio_encoder_options.channels(),
                };

                (Some(audio_encoder_options), Some(output_audio_options))
            }
            None => (None, None),
        };

        let output_options = output::OutputOptions {
            output_protocol: output::OutputProtocolOptions::Whip(output::whip::WhipSenderOptions {
                endpoint_url,
                bearer_token,
                video: video_codec,
                audio: audio_options,
            }),
            video: video_encoder_options,
            audio: audio_encoder_options,
        };

        Ok(Self {
            output_options,
            video: output_video_options,
            audio: output_audio_options,
        })
    }
}

impl TryFrom<RtmpOutput> for pipeline::RegisterOutputOptions<output::OutputOptions> {
    type Error = TypeError;

    fn try_from(value: RtmpOutput) -> Result<Self, Self::Error> {
        let RtmpOutput { url, video, audio } = value;
        let video_track = video.as_ref().map(|v| match v.encoder {
            VideoEncoderOptions::FfmpegH264 { .. } => RtmpVideoTrack {
                width: v.resolution.width as u32,
                height: v.resolution.height as u32,
            },
        });
        let audio_track = audio.as_ref().map(|a| match &a.encoder {
            RtmpAudioEncoderOptions::Aac {
                channels,
                sample_rate,
            } => RtmpAudioTrack {
                channels: channels.clone().into(),
                sample_rate: sample_rate.unwrap_or(44100),
            },
        });

        let (video_encoder_options, output_video_options) = maybe_video_options(video)?;
        let (audio_encoder_options, output_audio_options) = match audio {
            Some(OutputRtmpAudioOptions {
                mixing_strategy,
                send_eos_when,
                encoder,
                initial,
            }) => {
                let audio_encoder_options: AudioEncoderOptions = encoder.into();
                let output_audio_options = pipeline::OutputAudioOptions {
                    initial: initial.try_into()?,
                    end_condition: send_eos_when.unwrap_or_default().try_into()?,
                    mixing_strategy: mixing_strategy.unwrap_or(MixingStrategy::SumClip).into(),
                    channels: audio_encoder_options.channels(),
                };

                (Some(audio_encoder_options), Some(output_audio_options))
            }
            None => (None, None),
        };

        let output_options = output::OutputOptions {
            output_protocol: output::OutputProtocolOptions::Rtmp(output::rtmp::RtmpSenderOptions {
                url,
                video: video_track,
                audio: audio_track,
            }),
            video: video_encoder_options,
            audio: audio_encoder_options,
        };
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
    };

    let output_options = pipeline::OutputVideoOptions {
        initial: options.initial.try_into()?,
        end_condition: options.send_eos_when.unwrap_or_default().try_into()?,
    };

    Ok((Some(encoder_options), Some(output_options)))
}

impl From<Mp4AudioEncoderOptions> for pipeline::encoder::AudioEncoderOptions {
    fn from(value: Mp4AudioEncoderOptions) -> Self {
        match value {
            Mp4AudioEncoderOptions::Aac {
                channels,
                sample_rate,
            } => AudioEncoderOptions::Aac(AacEncoderOptions {
                channels: channels.into(),
                sample_rate: sample_rate.unwrap_or(44100),
            }),
        }
    }
}

impl From<RtmpAudioEncoderOptions> for pipeline::encoder::AudioEncoderOptions {
    fn from(value: RtmpAudioEncoderOptions) -> Self {
        match value {
            RtmpAudioEncoderOptions::Aac {
                channels,
                sample_rate,
            } => AudioEncoderOptions::Aac(AacEncoderOptions {
                channels: channels.into(),
                sample_rate: sample_rate.unwrap_or(44100),
            }),
        }
    }
}

impl From<RtpAudioEncoderOptions> for pipeline::encoder::AudioEncoderOptions {
    fn from(value: RtpAudioEncoderOptions) -> Self {
        match value {
            RtpAudioEncoderOptions::Opus {
                channels,
                preset,
                sample_rate,
            } => AudioEncoderOptions::Opus(encoder::opus::OpusEncoderOptions {
                channels: channels.into(),
                preset: preset.unwrap_or(OpusEncoderPreset::Voip).into(),
                sample_rate: sample_rate.unwrap_or(48000),
            }),
        }
    }
}

impl From<WhipAudioEncoderOptions> for pipeline::encoder::AudioEncoderOptions {
    fn from(value: WhipAudioEncoderOptions) -> Self {
        match value {
            WhipAudioEncoderOptions::Opus {
                channels,
                preset,
                sample_rate,
            } => AudioEncoderOptions::Opus(encoder::opus::OpusEncoderOptions {
                channels: channels.into(),
                preset: preset.unwrap_or(OpusEncoderPreset::Voip).into(),
                sample_rate: sample_rate.unwrap_or(48000),
            }),
        }
    }
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
