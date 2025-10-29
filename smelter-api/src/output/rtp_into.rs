use crate::common_core::prelude as core;
use crate::*;

impl TryFrom<RtpOutput> for core::RegisterOutputOptions {
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

        let (video_encoder_options, output_video_options) = match video {
            Some(OutputRtpVideoOptions {
                resolution,
                send_eos_when,
                encoder,
                initial,
            }) => {
                let encoder_options = encoder.to_pipeline_options(resolution)?;
                let output_options = core::RegisterOutputVideoOptions {
                    initial: initial.try_into()?,
                    end_condition: send_eos_when.unwrap_or_default().try_into()?,
                };
                (Some(encoder_options), Some(output_options))
            }
            None => (None, None),
        };

        let (audio_encoder_options, output_audio_options) = match audio {
            Some(OutputRtpAudioOptions {
                mixing_strategy,
                send_eos_when,
                encoder,
                channels,
                initial,
            }) => {
                let channels = channels.unwrap_or(AudioChannels::Stereo);
                let encoder_options = encoder.to_pipeline_options(channels)?;
                let output_options = core::RegisterOutputAudioOptions {
                    initial: initial.try_into()?,
                    end_condition: send_eos_when.unwrap_or_default().try_into()?,
                    mixing_strategy: mixing_strategy
                        .unwrap_or(AudioMixingStrategy::SumClip)
                        .into(),
                    channels: channels.into(),
                };

                (Some(encoder_options), Some(output_options))
            }
            None => (None, None),
        };

        let connection_options = match transport_protocol.unwrap_or(TransportProtocol::Udp) {
            TransportProtocol::Udp => {
                let core::PortOrRange::Exact(port) = port.try_into()? else {
                    return Err(TypeError::new(
                        "Port range can not be used with UDP output stream (transport_protocol=\"udp\").",
                    ));
                };
                let Some(ip) = ip else {
                    return Err(TypeError::new(
                        "\"ip\" field is required when registering output UDP stream (transport_protocol=\"udp\").",
                    ));
                };
                core::RtpOutputConnectionOptions::Udp {
                    port: core::Port(port),
                    ip,
                }
            }
            TransportProtocol::TcpServer => {
                if ip.is_some() {
                    return Err(TypeError::new(
                        "\"ip\" field is not allowed when registering TCP server connection (transport_protocol=\"tcp_server\").",
                    ));
                }

                core::RtpOutputConnectionOptions::TcpServer {
                    port: port.try_into()?,
                }
            }
        };

        let output_options = core::ProtocolOutputOptions::Rtp(core::RtpOutputOptions {
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

impl RtpVideoEncoderOptions {
    fn to_pipeline_options(
        &self,
        resolution: Resolution,
    ) -> Result<core::VideoEncoderOptions, TypeError> {
        let encoder_options = match self {
            RtpVideoEncoderOptions::FfmpegH264 {
                preset,
                pixel_format,
                ffmpeg_options,
            } => core::VideoEncoderOptions::FfmpegH264(core::FfmpegH264EncoderOptions {
                preset: preset.unwrap_or(H264EncoderPreset::Fast).into(),
                resolution: resolution.into(),
                pixel_format: pixel_format.unwrap_or(PixelFormat::Yuv420p).into(),
                raw_options: ffmpeg_options
                    .clone()
                    .unwrap_or_default()
                    .into_iter()
                    .collect(),
            }),
            RtpVideoEncoderOptions::VulkanH264 { bitrate } => {
                core::VideoEncoderOptions::VulkanH264(core::VulkanH264EncoderOptions {
                    resolution: resolution.into(),
                    bitrate: bitrate.map(|bitrate| bitrate.try_into()).transpose()?,
                })
            }
            RtpVideoEncoderOptions::FfmpegVp8 { ffmpeg_options } => {
                core::VideoEncoderOptions::FfmpegVp8(core::FfmpegVp8EncoderOptions {
                    resolution: resolution.into(),
                    raw_options: ffmpeg_options
                        .clone()
                        .unwrap_or_default()
                        .into_iter()
                        .collect(),
                })
            }
            RtpVideoEncoderOptions::FfmpegVp9 {
                pixel_format,
                ffmpeg_options,
            } => core::VideoEncoderOptions::FfmpegVp9(core::FfmpegVp9EncoderOptions {
                resolution: resolution.into(),
                pixel_format: pixel_format.unwrap_or(PixelFormat::Yuv420p).into(),
                raw_options: ffmpeg_options
                    .clone()
                    .unwrap_or_default()
                    .into_iter()
                    .collect(),
            }),
        };
        Ok(encoder_options)
    }
}

impl RtpAudioEncoderOptions {
    fn to_pipeline_options(
        &self,
        channels: AudioChannels,
    ) -> Result<core::AudioEncoderOptions, TypeError> {
        let audio_encoder_options = match self {
            RtpAudioEncoderOptions::Opus {
                preset,
                sample_rate,
                forward_error_correction,
                expected_packet_loss,
            } => {
                let packet_loss = match expected_packet_loss {
                    Some(x) if *x > 100 => {
                        return Err(TypeError::new(
                            "Expected packet loss value must be from [0, 100] range.",
                        ));
                    }
                    Some(x) => *x as i32,
                    None => 0,
                };
                core::AudioEncoderOptions::Opus(core::OpusEncoderOptions {
                    channels: channels.into(),
                    preset: preset.unwrap_or(OpusEncoderPreset::Voip).into(),
                    sample_rate: sample_rate.unwrap_or(48000),
                    forward_error_correction: forward_error_correction.unwrap_or(false),
                    packet_loss,
                })
            }
        };
        Ok(audio_encoder_options)
    }
}
