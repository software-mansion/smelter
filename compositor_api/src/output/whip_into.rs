use crate::common_pipeline::prelude as pipeline;
use crate::*;
use compositor_pipeline::codecs::OpusEncoderOptions;

impl TryFrom<WhipOutput> for pipeline::RegisterOutputOptions {
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

        let (output_video_options, video_whip_options) = if let Some(options) = video.clone() {
            let output_options = pipeline::RegisterOutputVideoOptions {
                initial: options.initial.try_into()?,
                end_condition: options.send_eos_when.unwrap_or_default().try_into()?,
            };

            let resolution = options.resolution;
            let encoder_preferences = match video {
                Some(options) => match options.encoder_preferences.as_deref() {
                    Some([]) | None => {
                        vec![pipeline::WhipVideoEncoderOptions::Any(resolution.into())]
                    }
                    Some(v) => v
                        .iter()
                        .cloned()
                        .map(|opts| opts.into_pipeline_options(resolution.clone()))
                        .collect::<Result<_, _>>()?,
                },
                None => vec![pipeline::WhipVideoEncoderOptions::Any(resolution.into())],
            };

            let video_whip_options = pipeline::VideoWhipOptions {
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
                channels,
                encoder_preferences,
                initial,
            }) => {
                let channels = channels.unwrap_or(AudioChannels::Stereo);
                let output_audio_options = pipeline::RegisterOutputAudioOptions {
                    initial: initial.try_into()?,
                    end_condition: send_eos_when.unwrap_or_default().try_into()?,
                    mixing_strategy: mixing_strategy
                        .unwrap_or(AudioMixingStrategy::SumClip)
                        .into(),
                    channels: channels.clone().into(),
                };

                let encoder_preferences = match encoder_preferences.as_deref() {
                    Some([]) | None => vec![pipeline::WhipAudioEncoderOptions::Any(
                        channels.clone().into(),
                    )],
                    Some(a) => a
                        .iter()
                        .cloned()
                        .map(|opts| opts.into_pipeline_options(channels.clone()))
                        .collect(),
                };

                let audio_whip_options = pipeline::AudioWhipOptions {
                    encoder_preferences,
                };
                (Some(output_audio_options), Some(audio_whip_options))
            }
            None => (None, None),
        };

        let output_options = pipeline::ProtocolOutputOptions::Whip(pipeline::WhipOutputOptions {
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
    fn into_pipeline_options(
        self,
        resolution: Resolution,
    ) -> Result<pipeline::WhipVideoEncoderOptions, TypeError> {
        let encoder_options: compositor_pipeline::codecs::WhipVideoEncoderOptions = match self {
            WhipVideoEncoderOptions::FfmpegH264 {
                preset,
                pixel_format,
                ffmpeg_options,
            } => {
                pipeline::WhipVideoEncoderOptions::FfmpegH264(pipeline::FfmpegH264EncoderOptions {
                    preset: preset.unwrap_or(H264EncoderPreset::Fast).into(),
                    resolution: resolution.into(),
                    pixel_format: pixel_format.unwrap_or(PixelFormat::Yuv420p).into(),
                    raw_options: ffmpeg_options.unwrap_or_default().into_iter().collect(),
                })
            }
            WhipVideoEncoderOptions::VulkanH264 { bitrate } => {
                pipeline::WhipVideoEncoderOptions::VulkanH264(pipeline::VulkanH264EncoderOptions {
                    resolution: resolution.into(),
                    bitrate: bitrate.map(|b| b.try_into()).transpose()?,
                })
            }
            WhipVideoEncoderOptions::FfmpegVp8 { ffmpeg_options } => {
                pipeline::WhipVideoEncoderOptions::FfmpegVp8(pipeline::FfmpegVp8EncoderOptions {
                    resolution: resolution.into(),
                    raw_options: ffmpeg_options.unwrap_or_default().into_iter().collect(),
                })
            }
            WhipVideoEncoderOptions::FfmpegVp9 {
                pixel_format,
                ffmpeg_options,
            } => pipeline::WhipVideoEncoderOptions::FfmpegVp9(pipeline::FfmpegVp9EncoderOptions {
                resolution: resolution.into(),
                pixel_format: pixel_format.unwrap_or(PixelFormat::Yuv420p).into(),
                raw_options: ffmpeg_options.unwrap_or_default().into_iter().collect(),
            }),
            WhipVideoEncoderOptions::Any => {
                pipeline::WhipVideoEncoderOptions::Any(resolution.into())
            }
        };

        Ok(encoder_options)
    }
}

impl WhipAudioEncoderOptions {
    fn into_pipeline_options(self, channels: AudioChannels) -> pipeline::WhipAudioEncoderOptions {
        let encoder_options: compositor_pipeline::codecs::WhipAudioEncoderOptions = match self {
            WhipAudioEncoderOptions::Opus {
                preset,
                sample_rate,
                forward_error_correction,
            } => pipeline::WhipAudioEncoderOptions::Opus(OpusEncoderOptions {
                channels: channels.into(),
                preset: preset.unwrap_or(OpusEncoderPreset::Voip).into(),
                sample_rate: sample_rate.unwrap_or(48000),
                forward_error_correction: forward_error_correction.unwrap_or(true),
                packet_loss: 0,
            }),
            WhipAudioEncoderOptions::Any => {
                pipeline::WhipAudioEncoderOptions::Opus(OpusEncoderOptions {
                    channels: channels.into(),
                    preset: OpusEncoderPreset::Voip.into(),
                    sample_rate: 48000,
                    forward_error_correction: true,
                    packet_loss: 0,
                })
            }
        };
        encoder_options
    }
}
