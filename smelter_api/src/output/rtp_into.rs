use crate::common_pipeline::prelude as pipeline;
use crate::*;

impl TryFrom<RtpOutput> for pipeline::RegisterOutputOptions {
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
                        forward_error_correction,
                        expected_packet_loss,
                    } => {
                        let resolved_channels = channels.unwrap_or(AudioChannels::Stereo);

                        let packet_loss = match expected_packet_loss {
                            Some(x) if x > 100 => {
                                return Err(TypeError::new(
                                    "Expected packet loss value must be from [0, 100] range.",
                                ))
                            }
                            Some(x) => x as i32,
                            None => 0,
                        };

                        (
                            pipeline::AudioEncoderOptions::Opus(pipeline::OpusEncoderOptions {
                                channels: resolved_channels.clone().into(),
                                preset: preset.unwrap_or(OpusEncoderPreset::Voip).into(),
                                sample_rate: sample_rate.unwrap_or(48000),
                                forward_error_correction: forward_error_correction.unwrap_or(false),
                                packet_loss,
                            }),
                            resolved_channels,
                        )
                    }
                };
                let output_audio_options = pipeline::RegisterOutputAudioOptions {
                    initial: initial.try_into()?,
                    end_condition: send_eos_when.unwrap_or_default().try_into()?,
                    mixing_strategy: mixing_strategy
                        .unwrap_or(AudioMixingStrategy::SumClip)
                        .into(),
                    channels: resolved_channels.into(),
                };

                (Some(audio_encoder_options), Some(output_audio_options))
            }
            None => (None, None),
        };

        let connection_options = match transport_protocol.unwrap_or(TransportProtocol::Udp) {
            TransportProtocol::Udp => {
                let pipeline::PortOrRange::Exact(port) = port.try_into()? else {
                    return Err(TypeError::new(
                        "Port range can not be used with UDP output stream (transport_protocol=\"udp\").",
                    ));
                };
                let Some(ip) = ip else {
                    return Err(TypeError::new(
                        "\"ip\" field is required when registering output UDP stream (transport_protocol=\"udp\").",
                    ));
                };
                pipeline::RtpOutputConnectionOptions::Udp {
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

                pipeline::RtpOutputConnectionOptions::TcpServer {
                    port: port.try_into()?,
                }
            }
        };

        let output_options = pipeline::ProtocolOutputOptions::Rtp(pipeline::RtpOutputOptions {
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

fn maybe_video_options(
    options: Option<OutputVideoOptions>,
) -> Result<
    (
        Option<pipeline::VideoEncoderOptions>,
        Option<pipeline::RegisterOutputVideoOptions>,
    ),
    TypeError,
> {
    let Some(options) = options else {
        return Ok((None, None));
    };

    let encoder_options = match options.encoder {
        VideoEncoderOptions::FfmpegH264 {
            preset,
            pixel_format,
            ffmpeg_options,
        } => pipeline::VideoEncoderOptions::FfmpegH264(pipeline::FfmpegH264EncoderOptions {
            preset: preset.unwrap_or(H264EncoderPreset::Fast).into(),
            resolution: options.resolution.into(),
            pixel_format: pixel_format.unwrap_or(PixelFormat::Yuv420p).into(),
            raw_options: ffmpeg_options.unwrap_or_default().into_iter().collect(),
        }),
        #[cfg(feature = "vk-video")]
        VideoEncoderOptions::VulkanH264 { bitrate } => {
            pipeline::VideoEncoderOptions::VulkanH264(pipeline::VulkanH264EncoderOptions {
                resolution: options.resolution.into(),
                bitrate: bitrate.map(|bitrate| bitrate.try_into()).transpose()?,
            })
        }
        #[cfg(not(feature = "vk-video"))]
        VideoEncoderOptions::VulkanH264 { .. } => {
            return Err(TypeError::new(super::NO_VULKAN_VIDEO));
        }
        VideoEncoderOptions::FfmpegVp8 { ffmpeg_options } => {
            pipeline::VideoEncoderOptions::FfmpegVp8(pipeline::FfmpegVp8EncoderOptions {
                resolution: options.resolution.into(),
                raw_options: ffmpeg_options.unwrap_or_default().into_iter().collect(),
            })
        }
        VideoEncoderOptions::FfmpegVp9 {
            pixel_format,
            ffmpeg_options,
        } => pipeline::VideoEncoderOptions::FfmpegVp9(pipeline::FfmpegVp9EncoderOptions {
            resolution: options.resolution.into(),
            pixel_format: pixel_format.unwrap_or(PixelFormat::Yuv420p).into(),
            raw_options: ffmpeg_options.unwrap_or_default().into_iter().collect(),
        }),
    };

    let output_options = pipeline::RegisterOutputVideoOptions {
        initial: options.initial.try_into()?,
        end_condition: options.send_eos_when.unwrap_or_default().try_into()?,
    };

    Ok((Some(encoder_options), Some(output_options)))
}
