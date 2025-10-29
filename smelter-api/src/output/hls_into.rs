use crate::common_core::prelude as core;
use crate::*;

impl TryFrom<HlsOutput> for core::RegisterOutputOptions {
    type Error = TypeError;

    fn try_from(request: HlsOutput) -> Result<Self, Self::Error> {
        let HlsOutput {
            path,
            max_playlist_size,
            video,
            audio,
        } = request;

        if video.is_none() && audio.is_none() {
            return Err(TypeError::new(
                "At least one of \"video\" and \"audio\" fields have to be specified.",
            ));
        }

        let (video_encoder_options, output_video_options) = match video {
            Some(OutputHlsVideoOptions {
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
            Some(OutputHlsAudioOptions {
                mixing_strategy,
                send_eos_when,
                encoder,
                channels,
                initial,
            }) => {
                let channels = channels.unwrap_or(AudioChannels::Stereo);
                let encoder_options = encoder.to_pipeline_options(channels);
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
        let output_options = core::ProtocolOutputOptions::Hls(core::HlsOutputOptions {
            output_path: path.into(),
            max_playlist_size,
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

impl HlsVideoEncoderOptions {
    fn to_pipeline_options(
        &self,
        resolution: Resolution,
    ) -> Result<core::VideoEncoderOptions, TypeError> {
        let encoder_options = match self {
            HlsVideoEncoderOptions::FfmpegH264 {
                preset,
                pixel_format,
                ffmpeg_options,
            } => core::VideoEncoderOptions::FfmpegH264(core::FfmpegH264EncoderOptions {
                preset: preset.unwrap_or(H264EncoderPreset::Fast).into(),
                resolution: resolution.into(),
                pixel_format: pixel_format.unwrap_or(PixelFormat::Yuv420p).into(),
                raw_options: ffmpeg_options
                    .clone()
                    .unwrap_or_default()
                    .into_iter()
                    .collect(),
            }),
            HlsVideoEncoderOptions::VulkanH264 { bitrate } => {
                core::VideoEncoderOptions::VulkanH264(core::VulkanH264EncoderOptions {
                    resolution: resolution.into(),
                    bitrate: bitrate.map(|bitrate| bitrate.try_into()).transpose()?,
                })
            }
        };
        Ok(encoder_options)
    }
}

impl HlsAudioEncoderOptions {
    fn to_pipeline_options(&self, channels: AudioChannels) -> core::AudioEncoderOptions {
        match self {
            HlsAudioEncoderOptions::Aac { sample_rate } => {
                core::AudioEncoderOptions::FdkAac(core::FdkAacEncoderOptions {
                    channels: channels.into(),
                    sample_rate: sample_rate.unwrap_or(44100),
                })
            }
        }
    }
}
