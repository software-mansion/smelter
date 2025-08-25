use itertools::Itertools;
use tracing::warn;

use crate::common_pipeline::prelude as pipeline;
use crate::*;

const ENCODER_DEPRECATION_MSG: &str = "Field 'encoder' is deprecated. The codec will now be set automatically based on WHIP negotiation; manual specification is no longer needed.";

const CHANNEL_DEPRECATION_MSG: &str = "The 'channels' field within the encoder options is deprecated and will be removed in future releases. Please use the 'channels' field in the audio options for setting the audio channels.";

impl TryFrom<WhipClient> for pipeline::RegisterOutputOptions {
    type Error = TypeError;

    fn try_from(request: WhipClient) -> Result<Self, Self::Error> {
        let WhipClient {
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

        if let Some(OutputWhipClientVideoOptions {
            encoder: Some(_encoder),
            ..
        }) = &video
        {
            warn!(ENCODER_DEPRECATION_MSG)
        }

        if let Some(OutputWhipClientAudioOptions {
            encoder: Some(_encoder),
            ..
        }) = &audio
        {
            warn!(ENCODER_DEPRECATION_MSG)
        }

        let (output_video_options, video_whip_options) = if let Some(options) = video {
            let output_options = pipeline::RegisterOutputVideoOptions {
                initial: options.initial.try_into()?,
                end_condition: options.send_eos_when.unwrap_or_default().try_into()?,
            };
            let encoder_preferences = match options.encoder_preferences.as_deref() {
                Some([]) | None => vec![WhipClientVideoEncoderOptions::Any],
                Some(v) => v.to_vec(),
            };

            let encoder_preferences: Vec<pipeline::VideoEncoderOptions> = encoder_preferences
                .into_iter()
                .flat_map(|codec| match codec {
                    WhipClientVideoEncoderOptions::FfmpegH264 {
                        preset,
                        pixel_format,
                        ffmpeg_options,
                    } => {
                        vec![pipeline::VideoEncoderOptions::FfmpegH264(
                            pipeline::FfmpegH264EncoderOptions {
                                preset: preset.unwrap_or(H264EncoderPreset::Fast).into(),
                                resolution: options.resolution.clone().into(),
                                pixel_format: pixel_format.unwrap_or(PixelFormat::Yuv420p).into(),
                                raw_options: ffmpeg_options
                                    .unwrap_or_default()
                                    .into_iter()
                                    .collect(),
                            },
                        )]
                    }
                    WhipClientVideoEncoderOptions::FfmpegVp8 { ffmpeg_options } => {
                        vec![pipeline::VideoEncoderOptions::FfmpegVp8(
                            pipeline::FfmpegVp8EncoderOptions {
                                resolution: options.resolution.clone().into(),
                                raw_options: ffmpeg_options
                                    .unwrap_or_default()
                                    .into_iter()
                                    .collect(),
                            },
                        )]
                    }
                    WhipClientVideoEncoderOptions::FfmpegVp9 {
                        pixel_format,
                        ffmpeg_options,
                    } => {
                        vec![pipeline::VideoEncoderOptions::FfmpegVp9(
                            pipeline::FfmpegVp9EncoderOptions {
                                resolution: options.resolution.clone().into(),
                                pixel_format: pixel_format.unwrap_or(PixelFormat::Yuv420p).into(),
                                raw_options: ffmpeg_options
                                    .unwrap_or_default()
                                    .into_iter()
                                    .collect(),
                            },
                        )]
                    }
                    WhipClientVideoEncoderOptions::Any => {
                        vec![
                            pipeline::VideoEncoderOptions::FfmpegVp9(
                                pipeline::FfmpegVp9EncoderOptions {
                                    resolution: options.resolution.clone().into(),
                                    pixel_format: pipeline::OutputPixelFormat::YUV420P,
                                    raw_options: Vec::new(),
                                },
                            ),
                            pipeline::VideoEncoderOptions::FfmpegVp8(
                                pipeline::FfmpegVp8EncoderOptions {
                                    resolution: options.resolution.clone().into(),
                                    raw_options: Vec::new(),
                                },
                            ),
                            pipeline::VideoEncoderOptions::FfmpegH264(
                                pipeline::FfmpegH264EncoderOptions {
                                    preset: H264EncoderPreset::Fast.into(),
                                    resolution: options.resolution.clone().into(),
                                    pixel_format: pipeline::OutputPixelFormat::YUV420P,
                                    raw_options: Vec::new(),
                                },
                            ),
                        ]
                    }
                })
                .unique()
                .collect();

            let video_whip_options = pipeline::VideoWhipOptions {
                encoder_preferences,
            };
            (Some(output_options), Some(video_whip_options))
        } else {
            (None, None)
        };

        let (output_audio_options, audio_whip_options) = match audio {
            Some(OutputWhipClientAudioOptions {
                mixing_strategy,
                send_eos_when,
                encoder,
                channels,
                encoder_preferences,
                initial,
            }) => {
                let resolved_channels = match encoder {
                    Some(WhipClientAudioEncoderOptions::Opus {
                        channels: channels_deprecated,
                        ..
                    }) => {
                        if channels_deprecated.is_some() {
                            warn!(CHANNEL_DEPRECATION_MSG);
                        }
                        channels
                            .or(channels_deprecated)
                            .unwrap_or(AudioChannels::Stereo)
                    }
                    _ => channels.unwrap_or(AudioChannels::Stereo),
                };
                let output_audio_options = pipeline::RegisterOutputAudioOptions {
                    initial: initial.try_into()?,
                    end_condition: send_eos_when.unwrap_or_default().try_into()?,
                    mixing_strategy: mixing_strategy
                        .unwrap_or(AudioMixingStrategy::SumClip)
                        .into(),
                    channels: resolved_channels.clone().into(),
                };

                let encoder_preferences = match encoder_preferences.as_deref() {
                    Some([]) | None => vec![WhipClientAudioEncoderOptions::Any],
                    Some(v) => v.to_vec(),
                };

                let encoder_preferences: Vec<pipeline::AudioEncoderOptions> = encoder_preferences
                    .into_iter()
                    .flat_map(|codec| match codec {
                        WhipClientAudioEncoderOptions::Opus {
                            preset,
                            sample_rate,
                            forward_error_correction,
                            ..
                        } => {
                            vec![pipeline::AudioEncoderOptions::Opus(
                                pipeline::OpusEncoderOptions {
                                    channels: resolved_channels.clone().into(),
                                    preset: preset.unwrap_or(OpusEncoderPreset::Voip).into(),
                                    sample_rate: sample_rate.unwrap_or(48000),
                                    forward_error_correction: forward_error_correction
                                        .unwrap_or(false),
                                    packet_loss: 0,
                                },
                            )]
                        }
                        WhipClientAudioEncoderOptions::Any => {
                            vec![
                                pipeline::AudioEncoderOptions::Opus(pipeline::OpusEncoderOptions {
                                    channels: resolved_channels.clone().into(),
                                    preset: OpusEncoderPreset::Voip.into(),
                                    sample_rate: 48000,
                                    forward_error_correction: true,
                                    packet_loss: 0,
                                }),
                                pipeline::AudioEncoderOptions::Opus(pipeline::OpusEncoderOptions {
                                    channels: resolved_channels.clone().into(),
                                    preset: OpusEncoderPreset::Voip.into(),
                                    sample_rate: 48000,
                                    forward_error_correction: false,
                                    packet_loss: 0,
                                }),
                            ]
                        }
                    })
                    .unique()
                    .collect();

                let audio_whip_options = pipeline::AudioWhipOptions {
                    encoder_preferences,
                };
                (Some(output_audio_options), Some(audio_whip_options))
            }
            None => {
                // even if audio field is unregistered add opus codec in order to make Twitch work
                let audio_whip_options = pipeline::AudioWhipOptions {
                    encoder_preferences: vec![pipeline::AudioEncoderOptions::Opus(
                        pipeline::OpusEncoderOptions {
                            channels: compositor_pipeline::AudioChannels::Stereo,
                            preset: OpusEncoderPreset::Voip.into(),
                            sample_rate: 48000,
                            forward_error_correction: false,
                            packet_loss: 0,
                        },
                    )],
                };
                (None, Some(audio_whip_options))
            }
        };

        let output_options =
            pipeline::ProtocolOutputOptions::Whip(pipeline::WhipClientOutputOptions {
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
