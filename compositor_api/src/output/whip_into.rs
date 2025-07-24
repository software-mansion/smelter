use compositor_pipeline::{
    audio_mixer,
    pipeline::{
        self,
        encoder::{self, ffmpeg_h264, ffmpeg_vp8, ffmpeg_vp9, opus},
        output::{self, whip},
    },
};
use itertools::Itertools;
use tracing::warn;

use crate::*;

const ENCODER_DEPRECATION_MSG: &str = "Field 'encoder' is deprecated. The codec will now be set automatically based on WHIP negotiation; manual specification is no longer needed.";

const CHANNEL_DEPRECATION_MSG: &str = "The 'channels' field within the encoder options is deprecated and will be removed in future releases. Please use the 'channels' field in the audio options for setting the audio channels.";

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
            warn!(ENCODER_DEPRECATION_MSG)
        }

        if let Some(OutputWhipAudioOptions {
            encoder: Some(_encoder),
            ..
        }) = &audio
        {
            warn!(ENCODER_DEPRECATION_MSG)
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
                            pixel_format,
                            ffmpeg_options,
                        } => {
                            vec![pipeline::encoder::VideoEncoderOptions::H264(
                                ffmpeg_h264::Options {
                                    preset: preset.unwrap_or(H264EncoderPreset::Fast).into(),
                                    resolution: options.resolution.clone().into(),
                                    pixel_format: pixel_format
                                        .unwrap_or(PixelFormat::Yuv420p)
                                        .into(),
                                    raw_options: ffmpeg_options
                                        .unwrap_or_default()
                                        .into_iter()
                                        .collect(),
                                },
                            )]
                        }
                        WhipVideoEncoderOptions::FfmpegVp8 { ffmpeg_options } => {
                            vec![pipeline::encoder::VideoEncoderOptions::Vp8(
                                ffmpeg_vp8::Options {
                                    resolution: options.resolution.clone().into(),
                                    raw_options: ffmpeg_options
                                        .unwrap_or_default()
                                        .into_iter()
                                        .collect(),
                                },
                            )]
                        }
                        WhipVideoEncoderOptions::FfmpegVp9 {
                            pixel_format,
                            ffmpeg_options,
                        } => {
                            vec![pipeline::encoder::VideoEncoderOptions::Vp9(
                                ffmpeg_vp9::Options {
                                    resolution: options.resolution.clone().into(),
                                    pixel_format: pixel_format
                                        .unwrap_or(PixelFormat::Yuv420p)
                                        .into(),
                                    raw_options: ffmpeg_options
                                        .unwrap_or_default()
                                        .into_iter()
                                        .collect(),
                                },
                            )]
                        }
                        WhipVideoEncoderOptions::Any => {
                            vec![
                                pipeline::encoder::VideoEncoderOptions::Vp9(ffmpeg_vp9::Options {
                                    resolution: options.resolution.clone().into(),
                                    pixel_format: encoder::OutputPixelFormat::YUV420P,
                                    raw_options: Vec::new(),
                                }),
                                pipeline::encoder::VideoEncoderOptions::Vp8(ffmpeg_vp8::Options {
                                    resolution: options.resolution.clone().into(),
                                    raw_options: Vec::new(),
                                }),
                                pipeline::encoder::VideoEncoderOptions::H264(
                                    ffmpeg_h264::Options {
                                        preset: H264EncoderPreset::Fast.into(),
                                        resolution: options.resolution.clone().into(),
                                        pixel_format: encoder::OutputPixelFormat::YUV420P,
                                        raw_options: Vec::new(),
                                    },
                                ),
                            ]
                        }
                    })
                    .unique()
                    .collect();

            let video_whip_options = output::whip::VideoWhipOptions {
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
                            warn!(CHANNEL_DEPRECATION_MSG);
                        }
                        channels
                            .or(channels_deprecated)
                            .unwrap_or(AudioChannels::Stereo)
                    }
                    _ => channels.unwrap_or(AudioChannels::Stereo),
                };
                let output_audio_options = pipeline::OutputAudioOptions {
                    initial: initial.try_into()?,
                    end_condition: send_eos_when.unwrap_or_default().try_into()?,
                    mixing_strategy: mixing_strategy
                        .unwrap_or(AudioMixingStrategy::SumClip)
                        .into(),
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
                                forward_error_correction,
                                ..
                            } => {
                                vec![pipeline::encoder::AudioEncoderOptions::Opus(
                                    opus::OpusEncoderOptions {
                                        channels: resolved_channels.clone().into(),
                                        preset: preset.unwrap_or(OpusEncoderPreset::Voip).into(),
                                        sample_rate: sample_rate.unwrap_or(48000),
                                        forward_error_correction: forward_error_correction
                                            .unwrap_or(false),
                                        // Default
                                        packet_loss: 0,
                                    },
                                )]
                            }
                            WhipAudioEncoderOptions::Any => {
                                vec![
                                    pipeline::encoder::AudioEncoderOptions::Opus(
                                        opus::OpusEncoderOptions {
                                            channels: resolved_channels.clone().into(),
                                            preset: OpusEncoderPreset::Voip.into(),
                                            sample_rate: 48000,
                                            forward_error_correction: true,
                                            // Default
                                            packet_loss: 0,
                                        },
                                    ),
                                    pipeline::encoder::AudioEncoderOptions::Opus(
                                        opus::OpusEncoderOptions {
                                            channels: resolved_channels.clone().into(),
                                            preset: OpusEncoderPreset::Voip.into(),
                                            sample_rate: 48000,
                                            forward_error_correction: false,
                                            // Default
                                            packet_loss: 0,
                                        },
                                    ),
                                ]
                            }
                        })
                        .unique()
                        .collect();

                let audio_whip_options = whip::AudioWhipOptions {
                    encoder_preferences,
                };
                (Some(output_audio_options), Some(audio_whip_options))
            }
            None => {
                // even if audio field is unregistered add opus codec in order to make Twitch work
                let audio_whip_options = whip::AudioWhipOptions {
                    encoder_preferences: vec![pipeline::encoder::AudioEncoderOptions::Opus(
                        opus::OpusEncoderOptions {
                            channels: audio_mixer::AudioChannels::Stereo,
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
            output::OutputOptions::Whip(pipeline::output::whip::WhipSenderOptions {
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
