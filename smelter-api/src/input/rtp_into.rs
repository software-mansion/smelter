use std::time::Duration;

use smelter_render::error::ErrorStack;

use bytes::Bytes;

use crate::common_core::prelude as core;
use crate::*;

use super::queue_options::new_queue_options;

impl TryFrom<RtpInput> for core::RegisterInputOptions {
    type Error = TypeError;

    fn try_from(value: RtpInput) -> Result<Self, Self::Error> {
        let RtpInput {
            port,
            video,
            audio,
            required,
            offset_ms,
            buffer_size_ms,
            transport_protocol,
            side_channel,
        } = value;

        let (required, offset) = new_queue_options(required, offset_ms)?;
        let side_channel = side_channel.unwrap_or_default();
        let side_channel_delay = side_channel.delay()?;

        let transport_protocol =
            transport_protocol.unwrap_or(TransportProtocol::Udp).into();

        let buffer_duration = buffer_size_ms
            .map(|ms| Duration::try_from_secs_f64(ms / 1000.0))
            .transpose()
            .map_err(|err| TypeError::new(format!("Invalid buffer_size_ms. {err}")))?;

        const NO_VIDEO_AUDIO_SPEC: &str = "At least one of `video` and `audio` has to be specified in `register_input` request.";

        if video.is_none() && audio.is_none() {
            return Err(TypeError::new(NO_VIDEO_AUDIO_SPEC));
        }

        Ok(core::RegisterInputOptions::Rtp(core::RtpInputOptions {
            port: port.try_into()?,
            video: video
                .as_ref()
                .map(|video| {
                    let options = match video.decoder {
                        RtpVideoDecoderOptions::FfmpegH264 => {
                            core::VideoDecoderOptions::FfmpegH264
                        }
                        RtpVideoDecoderOptions::FfmpegVp8 => {
                            core::VideoDecoderOptions::FfmpegVp8
                        }
                        RtpVideoDecoderOptions::FfmpegVp9 => {
                            core::VideoDecoderOptions::FfmpegVp9
                        }
                        RtpVideoDecoderOptions::VulkanH264 => {
                            core::VideoDecoderOptions::VulkanH264
                        }
                    };
                    Ok(options)
                })
                .transpose()?,
            audio: audio.map(TryFrom::try_from).transpose()?,
            transport_protocol,
            buffer_duration,
            queue_options: core::QueueInputOptions {
                required,
                video_side_channel: side_channel.video.unwrap_or(false),
                audio_side_channel: side_channel.audio.unwrap_or(false),
                side_channel_delay,
            },
            offset,
        }))
    }
}

impl TryFrom<InputRtpAudioOptions> for core::RtpAudioOptions {
    type Error = TypeError;

    fn try_from(audio: InputRtpAudioOptions) -> Result<Self, Self::Error> {
        match audio {
            InputRtpAudioOptions::Opus => Ok(core::RtpAudioOptions::Opus),
            InputRtpAudioOptions::Aac { audio_specific_config, rtp_mode } => {
                let depayloader_mode = match rtp_mode {
                    Some(AacRtpMode::LowBitrate) => {
                        core::RtpAacDepayloaderMode::LowBitrate
                    }
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

                Ok(core::RtpAudioOptions::FdkAac { depayloader_mode, raw_asc, asc })
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
