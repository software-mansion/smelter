use smelter_core::protocols::RtpInputTransportProtocol;
use smelter_render::error::ErrorStack;
use std::time::Duration;

use bytes::Bytes;

use crate::common_core::prelude as core;
use crate::*;

impl TryFrom<RtpInput> for core::RegisterInputOptions {
    type Error = TypeError;

    fn try_from(value: RtpInput) -> Result<Self, Self::Error> {
        let RtpInput {
            port,
            video,
            audio,
            required,
            offset_ms,
            transport_protocol,
        } = value;

        let queue_options = smelter_core::QueueInputOptions {
            required: required.unwrap_or(false),
            offset: offset_ms.map(|offset_ms| Duration::from_secs_f64(offset_ms / 1000.0)),
        };

        let transport_protocol = transport_protocol.unwrap_or(TransportProtocol::Udp).into();

        let input_buffer = match &queue_options {
            core::QueueInputOptions {
                required: false,
                offset: None,
            } => core::InputBufferOptions::Const(None),
            _ => core::InputBufferOptions::None,
        };
        let jitter_buffer = match transport_protocol {
            RtpInputTransportProtocol::Udp => match &queue_options {
                core::QueueInputOptions {
                    required: false,
                    offset: None,
                } => core::RtpJitterBufferOptions {
                    mode: core::RtpJitterBufferMode::QueueBased,
                    buffer: input_buffer,
                },
                _ => core::RtpJitterBufferOptions {
                    mode: core::RtpJitterBufferMode::Fixed(Duration::from_millis(200)),
                    buffer: input_buffer,
                },
            },
            RtpInputTransportProtocol::TcpServer => core::RtpJitterBufferOptions {
                mode: core::RtpJitterBufferMode::Disabled,
                buffer: input_buffer,
            },
        };

        const NO_VIDEO_AUDIO_SPEC: &str =
            "At least one of `video` and `audio` has to be specified in `register_input` request.";

        if video.is_none() && audio.is_none() {
            return Err(TypeError::new(NO_VIDEO_AUDIO_SPEC));
        }

        let input_options = core::ProtocolInputOptions::Rtp(core::RtpInputOptions {
            port: port.try_into()?,
            video: video
                .as_ref()
                .map(|video| {
                    let options = match video.decoder {
                        RtpVideoDecoderOptions::FfmpegH264 => core::VideoDecoderOptions::FfmpegH264,
                        RtpVideoDecoderOptions::FfmpegVp8 => core::VideoDecoderOptions::FfmpegVp8,
                        RtpVideoDecoderOptions::FfmpegVp9 => core::VideoDecoderOptions::FfmpegVp9,
                        RtpVideoDecoderOptions::VulkanH264 => core::VideoDecoderOptions::VulkanH264,
                    };
                    Ok(options)
                })
                .transpose()?,
            audio: audio.map(TryFrom::try_from).transpose()?,
            transport_protocol,
            jitter_buffer,
        });

        Ok(core::RegisterInputOptions {
            input_options,
            queue_options,
        })
    }
}

impl TryFrom<InputRtpAudioOptions> for core::RtpAudioOptions {
    type Error = TypeError;

    fn try_from(audio: InputRtpAudioOptions) -> Result<Self, Self::Error> {
        match audio {
            InputRtpAudioOptions::Opus => Ok(core::RtpAudioOptions::Opus),
            InputRtpAudioOptions::Aac {
                audio_specific_config,
                rtp_mode,
            } => {
                let depayloader_mode = match rtp_mode {
                    Some(AacRtpMode::LowBitrate) => core::RtpAacDepayloaderMode::LowBitrate,
                    Some(AacRtpMode::HighBitrate) | None => {
                        core::RtpAacDepayloaderMode::HighBitrate
                    }
                };

                let raw_asc = parse_hexadecimal_octet_string(&audio_specific_config)?;

                const EMPTY_ASC: &str = "The AudioSpecificConfig field is empty.";
                if raw_asc.is_empty() {
                    return Err(TypeError::new(EMPTY_ASC));
                }

                let asc = core::AacAudioSpecificConfig::parse_from(&raw_asc)
                    .map_err(|err| TypeError::new(ErrorStack::new(&err).into_string()))?;

                Ok(core::RtpAudioOptions::FdkAac {
                    depayloader_mode,
                    raw_asc,
                    asc,
                })
            }
        }
    }
}

/// [RFC 3640, section 4.1. MIME Type Registration (`config` subsection)](https://datatracker.ietf.org/doc/html/rfc3640#section-4.1)
fn parse_hexadecimal_octet_string(s: &str) -> Result<Bytes, TypeError> {
    const NOT_ALL_HEX: &str = "Not all of the provided string are hex digits.";
    if !s.chars().all(|c| char::is_ascii_hexdigit(&c)) {
        return Err(TypeError::new(NOT_ALL_HEX));
    }

    s.as_bytes()
        .chunks(2)
        .map(|byte| {
            let byte = match byte {
                &[b1, b2, ..] => [b1, b2],
                &[b1] => [b1, 0],
                [] => [0, 0],
            };

            let byte = String::from_utf8_lossy(&byte);

            const BYTE_PARSE_ERROR: &str =
                "An error occurred while parsing a byte of the octet string";
            u8::from_str_radix(&byte, 16).map_err(|_| TypeError::new(BYTE_PARSE_ERROR))
        })
        .collect()
}
