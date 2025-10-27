use crate::common_core::prelude as core;
use crate::*;

impl TryFrom<WhipOutput> for core::RegisterOutputOptions {
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
        let (output_video_options, video_whip_options) = match video {
            Some(options) => {
                let resolution = options.resolution;

                let output_options = core::RegisterOutputVideoOptions {
                    initial: options.initial.try_into()?,
                    end_condition: options.send_eos_when.unwrap_or_default().try_into()?,
                };

                let encoder_preferences = match options.encoder_preferences.as_deref() {
                    Some([]) | None => {
                        vec![core::WhipVideoEncoderOptions::Any(resolution.into())]
                    }
                    Some(prefs) => prefs
                        .iter()
                        .map(|p| p.to_pipeline_options(resolution))
                        .collect::<Result<_, _>>()?,
                };

                let video_whip_options = core::VideoWhipOptions {
                    encoder_preferences,
                };

                (Some(output_options), Some(video_whip_options))
            }
            None => (None, None),
        };

        let (output_audio_options, audio_whip_options) = match audio {
            Some(OutputWhipAudioOptions {
                mixing_strategy,
                send_eos_when,
                channels,
                encoder_preferences,
                initial,
            }) => {
                let channels = channels.unwrap_or(AudioChannels::Stereo);
                let output_audio_options = core::RegisterOutputAudioOptions {
                    initial: initial.try_into()?,
                    end_condition: send_eos_when.unwrap_or_default().try_into()?,
                    mixing_strategy: mixing_strategy
                        .unwrap_or(AudioMixingStrategy::SumClip)
                        .into(),
                    channels: channels.into(),
                };

                let encoder_preferences = match encoder_preferences.as_deref() {
                    Some([]) | None => {
                        vec![core::WhipAudioEncoderOptions::Any(channels.into())]
                    }
                    Some(prefs) => prefs
                        .iter()
                        .map(|opts| opts.to_pipeline_options(channels))
                        .collect(),
                };

                let audio_whip_options = core::AudioWhipOptions {
                    encoder_preferences,
                };
                (Some(output_audio_options), Some(audio_whip_options))
            }
            None => (None, None),
        };

        let output_options = core::ProtocolOutputOptions::Whip(core::WhipOutputOptions {
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

impl WhipVideoEncoderOptions {
    fn to_pipeline_options(
        &self,
        resolution: Resolution,
    ) -> Result<core::WhipVideoEncoderOptions, TypeError> {
        let encoder_options: core::WhipVideoEncoderOptions = match self {
            WhipVideoEncoderOptions::FfmpegH264 {
                preset,
                pixel_format,
                ffmpeg_options,
            } => core::WhipVideoEncoderOptions::FfmpegH264(core::FfmpegH264EncoderOptions {
                preset: preset.unwrap_or(H264EncoderPreset::Fast).into(),
                resolution: resolution.into(),
                pixel_format: pixel_format.unwrap_or(PixelFormat::Yuv420p).into(),
                raw_options: ffmpeg_options
                    .clone()
                    .unwrap_or_default()
                    .into_iter()
                    .collect(),
            }),
            WhipVideoEncoderOptions::VulkanH264 { bitrate } => {
                core::WhipVideoEncoderOptions::VulkanH264(core::VulkanH264EncoderOptions {
                    resolution: resolution.into(),
                    bitrate: bitrate.map(|b| b.try_into()).transpose()?,
                })
            }
            WhipVideoEncoderOptions::FfmpegVp8 { ffmpeg_options } => {
                core::WhipVideoEncoderOptions::FfmpegVp8(core::FfmpegVp8EncoderOptions {
                    resolution: resolution.into(),
                    raw_options: ffmpeg_options
                        .clone()
                        .unwrap_or_default()
                        .into_iter()
                        .collect(),
                })
            }
            WhipVideoEncoderOptions::FfmpegVp9 {
                pixel_format,
                ffmpeg_options,
            } => core::WhipVideoEncoderOptions::FfmpegVp9(core::FfmpegVp9EncoderOptions {
                resolution: resolution.into(),
                pixel_format: pixel_format.unwrap_or(PixelFormat::Yuv420p).into(),
                raw_options: ffmpeg_options
                    .clone()
                    .unwrap_or_default()
                    .into_iter()
                    .collect(),
            }),
            WhipVideoEncoderOptions::Any => core::WhipVideoEncoderOptions::Any(resolution.into()),
        };

        Ok(encoder_options)
    }
}

impl WhipAudioEncoderOptions {
    fn to_pipeline_options(&self, channels: AudioChannels) -> core::WhipAudioEncoderOptions {
        let encoder_options: core::WhipAudioEncoderOptions = match self {
            WhipAudioEncoderOptions::Opus {
                preset,
                sample_rate,
                forward_error_correction,
            } => core::WhipAudioEncoderOptions::Opus(core::OpusEncoderOptions {
                channels: channels.into(),
                preset: preset.unwrap_or(OpusEncoderPreset::Voip).into(),
                sample_rate: sample_rate.unwrap_or(48000),
                forward_error_correction: forward_error_correction.unwrap_or(true),
                packet_loss: 0,
            }),
            WhipAudioEncoderOptions::Any => core::WhipAudioEncoderOptions::Any(channels.into()),
        };
        encoder_options
    }
}
