use compositor_pipeline::pipeline::{
    self,
    encoder::{self, ffmpeg_h264, ffmpeg_vp8, ffmpeg_vp9, opus},
    output,
};
use tracing::warn;

use crate::*;

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
                        forward_error_correction,
                        expected_packet_loss,
                    } => {
                        if channels_deprecated.is_some() {
                            warn!("The 'channels' field within the encoder options is deprecated and will be removed in future releases. Please use the 'channels' field in the audio options for setting the audio channels.");
                        }
                        let resolved_channels = channels
                            .or(channels_deprecated)
                            .unwrap_or(AudioChannels::Stereo);
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
                            encoder::AudioEncoderOptions::Opus(opus::OpusEncoderOptions {
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
                let output_audio_options = pipeline::OutputAudioOptions {
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
            pixel_format,
            ffmpeg_options,
        } => pipeline::encoder::VideoEncoderOptions::H264(ffmpeg_h264::Options {
            preset: preset.unwrap_or(H264EncoderPreset::Fast).into(),
            resolution: options.resolution.into(),
            pixel_format: pixel_format.unwrap_or(PixelFormat::Yuv420p).into(),
            raw_options: ffmpeg_options.unwrap_or_default().into_iter().collect(),
        }),
        VideoEncoderOptions::FfmpegVp8 { ffmpeg_options } => {
            pipeline::encoder::VideoEncoderOptions::VP8(ffmpeg_vp8::Options {
                resolution: options.resolution.into(),
                raw_options: ffmpeg_options.unwrap_or_default().into_iter().collect(),
            })
        }
        VideoEncoderOptions::FfmpegVp9 {
            pixel_format,
            ffmpeg_options,
        } => pipeline::encoder::VideoEncoderOptions::VP9(ffmpeg_vp9::Options {
            resolution: options.resolution.into(),
            pixel_format: pixel_format.unwrap_or(PixelFormat::Yuv420p).into(),
            raw_options: ffmpeg_options.unwrap_or_default().into_iter().collect(),
        }),
    };

    let output_options = pipeline::OutputVideoOptions {
        initial: options.initial.try_into()?,
        end_condition: options.send_eos_when.unwrap_or_default().try_into()?,
    };

    Ok((Some(encoder_options), Some(output_options)))
}
