use crate::common_core::prelude as core;
use crate::*;

impl TryFrom<MoqClientOutput> for core::RegisterOutputOptions {
    type Error = TypeError;

    fn try_from(request: MoqClientOutput) -> Result<Self, Self::Error> {
        let MoqClientOutput {
            endpoint_url,
            broadcast_path,
            container,
            video,
            audio,
        } = request;

        if video.is_none() && audio.is_none() {
            return Err(TypeError::new(
                "At least one of the \"video\" and \"audio\" fields have to be specified.",
            ));
        }

        let container = container.unwrap_or(MoqOutputContainer::Cmaf);

        let (video_encoder_options, output_video_options) = match video {
            Some(OutputMoqClientVideoOptions {
                resolution,
                send_eos_when,
                encoder,
                initial,
            }) => {
                let encoder_options = encoder.to_pipeline_options(resolution, container)?;
                let output_options = core::RegisterOutputVideoOptions {
                    initial: initial.try_into()?,
                    end_condition: send_eos_when.unwrap_or_default().try_into()?,
                };
                (Some(encoder_options), Some(output_options))
            }
            None => (None, None),
        };

        let (audio_encoder_options, output_audio_options) = match audio {
            Some(OutputMoqClientAudioOptions {
                mixing_strategy,
                send_eos_when,
                encoder,
                channels,
                initial,
            }) => {
                let channels = channels.unwrap_or(AudioChannels::Stereo);
                let encoder_options = encoder.to_pipeline_options(channels, container)?;
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

        let output_options = core::ProtocolOutputOptions::MoqClient(core::MoqClientOutputOptions {
            endpoint_url,
            broadcast_path,
            container: container.into(),
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

impl From<MoqOutputContainer> for core::MoqOutputContainer {
    fn from(value: MoqOutputContainer) -> Self {
        match value {
            MoqOutputContainer::Legacy => core::MoqOutputContainer::Legacy,
            MoqOutputContainer::Cmaf => core::MoqOutputContainer::Cmaf,
            MoqOutputContainer::Loc => core::MoqOutputContainer::Loc,
        }
    }
}

impl MoqClientVideoEncoderOptions {
    fn to_pipeline_options(
        &self,
        resolution: Resolution,
        container: MoqOutputContainer,
    ) -> Result<core::VideoEncoderOptions, TypeError> {
        let bitstream_format = match container {
            MoqOutputContainer::Cmaf => core::H264BitstreamFormat::Avcc,
            MoqOutputContainer::Legacy | MoqOutputContainer::Loc => {
                core::H264BitstreamFormat::AnnexB
            }
        };

        let encoder_options = match self {
            MoqClientVideoEncoderOptions::FfmpegH264 {
                preset,
                bitrate,
                keyframe_interval_ms,
                pixel_format,
                ffmpeg_options,
            } => core::VideoEncoderOptions::FfmpegH264(core::FfmpegH264EncoderOptions {
                preset: preset.unwrap_or(H264EncoderPreset::Fast).into(),
                bitrate: bitrate.map(|b| b.try_into()).transpose()?,
                keyframe_interval: duration_from_keyframe_interval(keyframe_interval_ms)?,
                resolution: resolution.into(),
                pixel_format: pixel_format.unwrap_or(PixelFormat::Yuv420p).into(),
                raw_options: ffmpeg_options
                    .clone()
                    .unwrap_or_default()
                    .into_iter()
                    .collect(),
                bitstream_format,
            }),
            MoqClientVideoEncoderOptions::VulkanH264 {
                bitrate,
                keyframe_interval_ms,
            } => core::VideoEncoderOptions::VulkanH264(core::VulkanH264EncoderOptions {
                resolution: resolution.into(),
                bitrate: bitrate
                    .map(|bitrate| {
                        Ok(core::VulkanH264EncoderRateControl::VariableBitrate(
                            bitrate.try_into()?,
                        ))
                    })
                    .transpose()?,
                keyframe_interval: duration_from_keyframe_interval(keyframe_interval_ms)?,
                preset: core::VulkanH264EncoderPreset::HighQuality,
                bitstream_format,
            }),
            MoqClientVideoEncoderOptions::FfmpegVp8 {
                bitrate,
                keyframe_interval_ms,
                ffmpeg_options,
            } => core::VideoEncoderOptions::FfmpegVp8(core::FfmpegVp8EncoderOptions {
                resolution: resolution.into(),
                bitrate: bitrate.map(|b| b.try_into()).transpose()?,
                keyframe_interval: duration_from_keyframe_interval(keyframe_interval_ms)?,
                raw_options: ffmpeg_options
                    .clone()
                    .unwrap_or_default()
                    .into_iter()
                    .collect(),
            }),
            MoqClientVideoEncoderOptions::FfmpegVp9 {
                bitrate,
                keyframe_interval_ms,
                pixel_format,
                ffmpeg_options,
            } => core::VideoEncoderOptions::FfmpegVp9(core::FfmpegVp9EncoderOptions {
                resolution: resolution.into(),
                bitrate: bitrate.map(|b| b.try_into()).transpose()?,
                keyframe_interval: duration_from_keyframe_interval(keyframe_interval_ms)?,
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

impl MoqClientAudioEncoderOptions {
    fn to_pipeline_options(
        &self,
        channels: AudioChannels,
        container: MoqOutputContainer,
    ) -> Result<core::AudioEncoderOptions, TypeError> {
        let audio_encoder_options = match self {
            MoqClientAudioEncoderOptions::Aac { sample_rate } => {
                let bitstream_format = match container {
                    MoqOutputContainer::Cmaf => core::AacBitstreamFormat::Raw,
                    MoqOutputContainer::Legacy | MoqOutputContainer::Loc => {
                        core::AacBitstreamFormat::Adts
                    }
                };
                core::AudioEncoderOptions::FdkAac(core::FdkAacEncoderOptions {
                    channels: channels.into(),
                    sample_rate: sample_rate.unwrap_or(44100),
                    bitstream_format,
                })
            }
            MoqClientAudioEncoderOptions::Opus {
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
                    sample_rate: sample_rate.unwrap_or(48000),
                    forward_error_correction: forward_error_correction.unwrap_or(false),
                    packet_loss,
                })
            }
        };
        Ok(audio_encoder_options)
    }
}
