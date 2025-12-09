use crate::common_core::prelude as core;
use crate::*;

impl TryFrom<WhepOutput> for core::RegisterOutputOptions {
    type Error = TypeError;

    fn try_from(request: WhepOutput) -> Result<Self, Self::Error> {
        let WhepOutput {
            bearer_token,
            video,
            audio,
        } = request;

        if video.is_none() && audio.is_none() {
            return Err(TypeError::new(
                "At least one of \"video\" and \"audio\" fields have to be specified.",
            ));
        }

        let (video_encoder_options, output_video_options) = match video {
            Some(OutputWhepVideoOptions {
                resolution,
                send_eos_when,
                encoder,
                initial,
            }) => {
                let encoder_options = encoder.to_pipeline_options(resolution)?;
                let output_options = core::RegisterOutputVideoOptions {
                    initial: initial.try_into()?,
                    end_condition: send_eos_when.unwrap_or_default().try_into()?,
                };
                (Some(encoder_options), Some(output_options))
            }
            None => (None, None),
        };

        let (audio_encoder_options, output_audio_options) = match audio {
            Some(OutputWhepAudioOptions {
                mixing_strategy,
                send_eos_when,
                encoder,
                channels,
                initial,
            }) => {
                let channels = channels.unwrap_or(AudioChannels::Stereo);
                let encoder_options = encoder.to_pipeline_options(channels)?;
                let output_options = core::RegisterOutputAudioOptions {
                    initial: initial.try_into()?,
                    end_condition: send_eos_when.unwrap_or_default().try_into()?,
                    mixing_strategy: mixing_strategy
                        .unwrap_or(AudioMixingStrategy::SumClip)
                        .into(),
                    channels: channels.into(),
                };

                (Some(encoder_options), Some(output_options))
            }
            None => (None, None),
        };

        Ok(Self {
            output_options: core::ProtocolOutputOptions::Whep(core::WhepOutputOptions {
                bearer_token,
                video: video_encoder_options,
                audio: audio_encoder_options,
            }),
            video: output_video_options,
            audio: output_audio_options,
        })
    }
}

impl WhepVideoEncoderOptions {
    fn to_pipeline_options(
        &self,
        resolution: Resolution,
    ) -> Result<core::VideoEncoderOptions, TypeError> {
        let encoder_options = match self {
            WhepVideoEncoderOptions::FfmpegH264 {
                preset,
                bitrate,
                pixel_format,
                ffmpeg_options,
            } => core::VideoEncoderOptions::FfmpegH264(core::FfmpegH264EncoderOptions {
                preset: preset.unwrap_or(H264EncoderPreset::Fast).into(),
                bitrate: bitrate.map(|b| b.try_into()).transpose()?,
                resolution: resolution.into(),
                pixel_format: pixel_format.unwrap_or(PixelFormat::Yuv420p).into(),
                raw_options: ffmpeg_options
                    .clone()
                    .unwrap_or_default()
                    .into_iter()
                    .collect(),
            }),
            WhepVideoEncoderOptions::VulkanH264 { bitrate } => {
                core::VideoEncoderOptions::VulkanH264(core::VulkanH264EncoderOptions {
                    resolution: resolution.into(),
                    bitrate: bitrate.map(|bitrate| bitrate.try_into()).transpose()?,
                })
            }
            WhepVideoEncoderOptions::FfmpegVp8 {
                bitrate,
                ffmpeg_options,
            } => core::VideoEncoderOptions::FfmpegVp8(core::FfmpegVp8EncoderOptions {
                resolution: resolution.into(),
                bitrate: bitrate.map(|b| b.try_into()).transpose()?,
                raw_options: ffmpeg_options
                    .clone()
                    .unwrap_or_default()
                    .into_iter()
                    .collect(),
            }),
            WhepVideoEncoderOptions::FfmpegVp9 {
                bitrate,
                pixel_format,
                ffmpeg_options,
            } => core::VideoEncoderOptions::FfmpegVp9(core::FfmpegVp9EncoderOptions {
                resolution: resolution.into(),
                bitrate: bitrate.map(|b| b.try_into()).transpose()?,
                pixel_format: pixel_format.unwrap_or(PixelFormat::Yuv420p).into(),
                raw_options: ffmpeg_options
                    .clone()
                    .unwrap_or_default()
                    .into_iter()
                    .collect(),
            }),
        };
        Ok(encoder_options)
    }
}

impl WhepAudioEncoderOptions {
    fn to_pipeline_options(
        &self,
        channels: AudioChannels,
    ) -> Result<core::AudioEncoderOptions, TypeError> {
        let audio_encoder_options = match self {
            WhepAudioEncoderOptions::Opus {
                preset,
                sample_rate,
                forward_error_correction,
                expected_packet_loss,
            } => {
                let packet_loss = match expected_packet_loss {
                    Some(x) if *x > 100 => {
                        return Err(TypeError::new(
                            "Expected packet loss value must be from [0, 100] range.",
                        ));
                    }
                    Some(x) => *x as i32,
                    None => 0,
                };
                core::AudioEncoderOptions::Opus(core::OpusEncoderOptions {
                    channels: channels.into(),
                    preset: preset.unwrap_or(OpusEncoderPreset::Voip).into(),
                    sample_rate: sample_rate.unwrap_or(48_000),
                    forward_error_correction: forward_error_correction.unwrap_or(true),
                    packet_loss,
                })
            }
        };
        Ok(audio_encoder_options)
    }
}
