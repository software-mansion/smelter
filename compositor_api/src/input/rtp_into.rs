use compositor_render::error::ErrorStack;
use std::time::Duration;
use tracing::warn;

use bytes::Bytes;

use crate::common_pipeline::prelude as pipeline;
use crate::*;

impl TryFrom<RtpInput> for pipeline::RegisterInputOptions {
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

        const NO_VIDEO_AUDIO_SPEC: &str =
            "At least one of `video` and `audio` has to be specified in `register_input` request.";

        if video.is_none() && audio.is_none() {
            return Err(TypeError::new(NO_VIDEO_AUDIO_SPEC));
        }

        let input_options = pipeline::ProtocolInputOptions::Rtp(pipeline::RtpInputOptions {
            port: port.try_into()?,
            video: video
                .as_ref()
                .map(|video| {
                    let options = match video.decoder {
                        VideoDecoder::FfmpegH264 => pipeline::VideoDecoderOptions::FfmpegH264,
                        VideoDecoder::FfmpegVp8 => pipeline::VideoDecoderOptions::FfmpegVp8,
                        VideoDecoder::FfmpegVp9 => pipeline::VideoDecoderOptions::FfmpegVp9,

                        VideoDecoder::VulkanH264 | VideoDecoder::VulkanVideo
                            if !cfg!(feature = "vk-video") =>
                        {
                            return Err(TypeError::new(super::NO_VULKAN_VIDEO))
                        }
                        VideoDecoder::VulkanH264 => pipeline::VideoDecoderOptions::VulkanH264,
                        VideoDecoder::VulkanVideo => {
                            tracing::warn!(
                                "vulkan_video option is deprecated, use vulkan_h264 instead."
                            );
                            pipeline::VideoDecoderOptions::VulkanH264
                        }
                    };
                    Ok(options)
                })
                .transpose()?,
            audio: audio.map(TryFrom::try_from).transpose()?,
            transport_protocol: transport_protocol.unwrap_or(TransportProtocol::Udp).into(),
            buffer_duration: None,
        });

        let queue_options = compositor_pipeline::QueueInputOptions {
            required: required.unwrap_or(false),
            offset: offset_ms.map(|offset_ms| Duration::from_secs_f64(offset_ms / 1000.0)),
        };

        Ok(pipeline::RegisterInputOptions {
            input_options,
            queue_options,
        })
    }
}

impl TryFrom<InputRtpAudioOptions> for pipeline::RtpAudioOptions {
    type Error = TypeError;

    fn try_from(audio: InputRtpAudioOptions) -> Result<Self, Self::Error> {
        match audio {
            InputRtpAudioOptions::Opus {
                forward_error_correction,
            } => {
                if forward_error_correction.is_some() {
                    warn!("The 'forward_error_correction' field is deprecated!");
                }
                Ok(pipeline::RtpAudioOptions::Opus)
            }
            InputRtpAudioOptions::Aac {
                audio_specific_config,
                rtp_mode,
            } => {
                let depayloader_mode = match rtp_mode {
                    Some(AacRtpMode::LowBitrate) => pipeline::RtpAacDepayloaderMode::LowBitrate,
                    Some(AacRtpMode::HighBitrate) | None => {
                        pipeline::RtpAacDepayloaderMode::HighBitrate
                    }
                };

                let raw_asc = parse_hexadecimal_octet_string(&audio_specific_config)?;

                const EMPTY_ASC: &str = "The AudioSpecificConfig field is empty.";
                if raw_asc.is_empty() {
                    return Err(TypeError::new(EMPTY_ASC));
                }

                let asc = pipeline::AacAudioSpecificConfig::parse_from(&raw_asc)
                    .map_err(|err| TypeError::new(ErrorStack::new(&err).into_string()))?;

                Ok(pipeline::RtpAudioOptions::FdkAac {
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
