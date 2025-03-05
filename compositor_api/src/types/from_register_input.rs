use std::time::Duration;

use bytes::Bytes;
use compositor_pipeline::{
    pipeline::{
        self, decoder,
        input::{self, rtp, whip},
    },
    queue,
};

use super::register_input::*;
use super::util::*;

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

impl TryFrom<InputRtpAudioOptions> for rtp::InputAudioStream {
    type Error = TypeError;

    fn try_from(audio: InputRtpAudioOptions) -> Result<Self, Self::Error> {
        match audio {
            InputRtpAudioOptions::Opus {
                forward_error_correction,
            } => {
                let forward_error_correction = forward_error_correction.unwrap_or(false);
                Ok(input::rtp::InputAudioStream {
                    options: decoder::AudioDecoderOptions::Opus(decoder::OpusDecoderOptions {
                        forward_error_correction,
                    }),
                })
            }
            InputRtpAudioOptions::Aac {
                audio_specific_config,
                rtp_mode,
            } => {
                let depayloader_mode = match rtp_mode {
                    Some(AacRtpMode::LowBitrate) => Some(decoder::AacDepayloaderMode::LowBitrate),
                    Some(AacRtpMode::HighBitrate) | None => {
                        Some(decoder::AacDepayloaderMode::HighBitrate)
                    }
                };

                let asc = parse_hexadecimal_octet_string(&audio_specific_config)?;

                const EMPTY_ASC: &str = "The AudioSpecificConfig field is empty.";
                if asc.is_empty() {
                    return Err(TypeError::new(EMPTY_ASC));
                }

                Ok(input::rtp::InputAudioStream {
                    options: decoder::AudioDecoderOptions::Aac(decoder::AacDecoderOptions {
                        depayloader_mode,
                        asc: Some(asc),
                    }),
                })
            }
        }
    }
}

#[cfg(not(feature = "vk-video"))]
const NO_VULKAN_VIDEO: &str =
    "Requested `vulkan_video` decoder, but this binary was compiled without the `vk-video` feature.";

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

        let rtp_stream = input::rtp::RtpStream {
            video: video
                .as_ref()
                .map(|video| {
                    Ok(input::rtp::InputVideoStream {
                        options: match video.decoder {
                            VideoDecoder::FfmpegH264 => decoder::VideoDecoderOptions {
                                decoder: pipeline::VideoDecoder::FFmpegH264,
                            },

                            VideoDecoder::FfmpegVp8 => decoder::VideoDecoderOptions {
                                decoder: pipeline::VideoDecoder::FFmpegVp8,
                            },

                            #[cfg(feature = "vk-video")]
                            VideoDecoder::VulkanVideo => decoder::VideoDecoderOptions {
                                decoder: pipeline::VideoDecoder::VulkanVideoH264,
                            },

                            #[cfg(not(feature = "vk-video"))]
                            VideoDecoder::VulkanVideo => {
                                return Err(TypeError::new(NO_VULKAN_VIDEO))
                            }
                        },
                    })
                })
                .transpose()?,
            audio: audio.map(TryFrom::try_from).transpose()?,
        };

        let input_options = input::InputOptions::Rtp(input::rtp::RtpReceiverOptions {
            port: port.try_into()?,
            stream: rtp_stream,
            transport_protocol: transport_protocol.unwrap_or(TransportProtocol::Udp).into(),
        });

        let queue_options = queue::QueueInputOptions {
            required: required.unwrap_or(false),
            offset: offset_ms.map(|offset_ms| Duration::from_secs_f64(offset_ms / 1000.0)),
            buffer_duration: None,
        };

        Ok(pipeline::RegisterInputOptions {
            input_options,
            queue_options,
        })
    }
}

impl TryFrom<InputWhipAudioOptions> for whip::InputAudioStream {
    type Error = TypeError;

    fn try_from(audio: InputWhipAudioOptions) -> Result<Self, Self::Error> {
        match audio {
            InputWhipAudioOptions::Opus {
                forward_error_correction,
            } => {
                let forward_error_correction = forward_error_correction.unwrap_or(false);
                Ok(input::whip::InputAudioStream {
                    options: decoder::OpusDecoderOptions {
                        forward_error_correction,
                    },
                })
            }
        }
    }
}

impl TryFrom<WhipInput> for pipeline::RegisterInputOptions {
    type Error = TypeError;

    fn try_from(value: WhipInput) -> Result<Self, Self::Error> {
        let WhipInput {
            video,
            audio,
            required,
            offset_ms,
        } = value;

        const NO_VIDEO_AUDIO_SPEC: &str =
            "At least one of `video` and `audio` has to be specified in `register_input` request.";

        if video.is_none() && audio.is_none() {
            return Err(TypeError::new(NO_VIDEO_AUDIO_SPEC));
        }

        let whip_receiver_options = input::whip::WhipReceiverOptions {
            video: video
                .as_ref()
                .map(|video| {
                    Ok(input::whip::InputVideoStream {
                        options: match video.decoder {
                            VideoDecoder::FfmpegH264 => decoder::VideoDecoderOptions {
                                decoder: pipeline::VideoDecoder::FFmpegH264,
                            },
                            VideoDecoder::FfmpegVp8 => decoder::VideoDecoderOptions {
                                decoder: pipeline::VideoDecoder::FFmpegVp8,
                            },
                            #[cfg(feature = "vk-video")]
                            VideoDecoder::VulkanVideo => decoder::VideoDecoderOptions {
                                decoder: pipeline::VideoDecoder::VulkanVideoH264,
                            },
                            #[cfg(not(feature = "vk-video"))]
                            VideoDecoder::VulkanVideo => {
                                return Err(TypeError::new(NO_VULKAN_VIDEO))
                            }
                        },
                    })
                })
                .transpose()?,
            audio: audio.map(TryFrom::try_from).transpose()?,
        };

        let input_options = input::InputOptions::Whip(whip_receiver_options);

        let queue_options = queue::QueueInputOptions {
            required: required.unwrap_or(false),
            offset: offset_ms.map(|offset_ms| Duration::from_secs_f64(offset_ms / 1000.0)),
            buffer_duration: None,
        };

        Ok(pipeline::RegisterInputOptions {
            input_options,
            queue_options,
        })
    }
}

impl TryFrom<Mp4Input> for pipeline::RegisterInputOptions {
    type Error = TypeError;

    fn try_from(value: Mp4Input) -> Result<Self, Self::Error> {
        let Mp4Input {
            url,
            path,
            required,
            offset_ms,
            should_loop,
            video_decoder,
        } = value;

        const BAD_URL_PATH_SPEC: &str =
            "Exactly one of `url` or `path` has to be specified in a register request for an mp4 input.";

        let source = match (url, path) {
            (Some(_), Some(_)) | (None, None) => {
                return Err(TypeError::new(BAD_URL_PATH_SPEC));
            }
            (Some(url), None) => input::mp4::Source::Url(url),
            (None, Some(path)) => input::mp4::Source::File(path.into()),
        };

        let queue_options = queue::QueueInputOptions {
            required: required.unwrap_or(false),
            offset: offset_ms.map(|offset_ms| Duration::from_secs_f64(offset_ms / 1000.0)),
            buffer_duration: None,
        };

        let video_decoder = match video_decoder.unwrap_or(VideoDecoder::FfmpegH264) {
            VideoDecoder::FfmpegH264 => pipeline::VideoDecoder::FFmpegH264,
            VideoDecoder::FfmpegVp8 => unimplemented!("MP4 VP8 input"),

            #[cfg(feature = "vk-video")]
            VideoDecoder::VulkanVideo => pipeline::VideoDecoder::VulkanVideoH264,

            #[cfg(not(feature = "vk-video"))]
            VideoDecoder::VulkanVideo => return Err(TypeError::new(NO_VULKAN_VIDEO)),
        };

        Ok(pipeline::RegisterInputOptions {
            input_options: input::InputOptions::Mp4(input::mp4::Mp4Options {
                source,
                should_loop: should_loop.unwrap_or(false),
                video_decoder,
            }),
            queue_options,
        })
    }
}

impl TryFrom<DeckLink> for pipeline::RegisterInputOptions {
    type Error = TypeError;

    #[cfg(feature = "decklink")]
    fn try_from(value: DeckLink) -> Result<Self, Self::Error> {
        use compositor_pipeline::pipeline::input::decklink;

        const ID_PARSE_ERROR_MESSAGE: &str =
            "\"persistent_id\" has to be a valid 32-bit hexadecimal number";

        let persistent_id = match value.persistent_id {
            Some(persistent_id) => {
                let Ok(persistent_id) = u32::from_str_radix(&persistent_id, 16) else {
                    return Err(TypeError::new(ID_PARSE_ERROR_MESSAGE));
                };
                Some(persistent_id)
            }
            None => None,
        };

        Ok(pipeline::RegisterInputOptions {
            input_options: input::InputOptions::DeckLink(input::decklink::DeckLinkOptions {
                subdevice_index: value.subdevice_index,
                display_name: value.display_name,
                persistent_id,
                enable_audio: value.enable_audio.unwrap_or(true),
                pixel_format: Some(decklink::PixelFormat::Format8BitYUV),
            }),
            queue_options: queue::QueueInputOptions {
                required: value.required.unwrap_or(false),
                offset: None,
                buffer_duration: Some(Duration::from_millis(5)),
            },
        })
    }

    #[cfg(not(feature = "decklink"))]
    fn try_from(_value: DeckLink) -> Result<Self, Self::Error> {
        Err(TypeError::new(
            "This Smelter binary was build without DeckLink support. Rebuilt it with \"decklink\" feature enabled.",
        ))
    }
}
