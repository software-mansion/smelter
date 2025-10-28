use crate::common_core::prelude as core;
use crate::*;

impl TryFrom<RtmpOutput> for core::RegisterOutputOptions {
    type Error = TypeError;

    fn try_from(value: RtmpOutput) -> Result<Self, Self::Error> {
        let RtmpOutput { url, video, audio } = value;

        if video.is_none() && audio.is_none() {
            return Err(TypeError::new(
                "At least one of \"video\" and \"audio\" fields have to be specified.",
            ));
        }

        let (video_encoder_options, output_video_options) = maybe_video_options_h264_only(video)?;
        let (audio_encoder_options, output_audio_options) = match audio {
            Some(OutputRtmpClientAudioOptions {
                mixing_strategy,
                send_eos_when,
                encoder,
                channels,
                initial,
            }) => {
                let (audio_encoder_options, resolved_channels) = match encoder {
                    RtmpClientAudioEncoderOptions::Aac { sample_rate } => {
                        let resolved_channels = channels.unwrap_or(AudioChannels::Stereo);
                        (
                            core::AudioEncoderOptions::FdkAac(core::FdkAacEncoderOptions {
                                channels: resolved_channels.into(),
                                sample_rate: sample_rate.unwrap_or(44100),
                            }),
                            resolved_channels,
                        )
                    }
                };
                let output_audio_options = core::RegisterOutputAudioOptions {
                    initial: initial.try_into()?,
                    end_condition: send_eos_when.unwrap_or_default().try_into()?,
                    mixing_strategy: mixing_strategy
                        .unwrap_or(AudioMixingStrategy::SumClip)
                        .into(),
                    channels: resolved_channels.into(),
                };

                (Some(audio_encoder_options), Some(output_audio_options))
            }
            None => (None, None),
        };

        let output_options = core::ProtocolOutputOptions::Rtmp(core::RtmpOutputOptions {
            url,
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

fn maybe_video_options_h264_only(
    options: Option<OutputVideoOptions>,
) -> Result<
    (
        Option<core::VideoEncoderOptions>,
        Option<core::RegisterOutputVideoOptions>,
    ),
    TypeError,
> {
    let Some(options) = options else {
        return Ok((None, None));
    };

    let encoder_options = match options.encoder {
        VideoEncoderOptions::FfmpegH264 {
            preset,
            pixel_format,
            ffmpeg_options,
        } => core::VideoEncoderOptions::FfmpegH264(core::FfmpegH264EncoderOptions {
            preset: preset.unwrap_or(H264EncoderPreset::Fast).into(),
            resolution: options.resolution.into(),
            pixel_format: pixel_format.unwrap_or(PixelFormat::Yuv420p).into(),
            raw_options: ffmpeg_options.unwrap_or_default().into_iter().collect(),
        }),
        #[cfg(feature = "vk-video")]
        VideoEncoderOptions::VulkanH264 { bitrate } => {
            core::VideoEncoderOptions::VulkanH264(core::VulkanH264EncoderOptions {
                resolution: options.resolution.into(),
                bitrate: bitrate.map(|bitrate| bitrate.try_into()).transpose()?,
            })
        }
        #[cfg(not(feature = "vk-video"))]
        VideoEncoderOptions::VulkanH264 { .. } => {
            return Err(TypeError::new(super::NO_VULKAN_VIDEO));
        }
        VideoEncoderOptions::FfmpegVp8 { .. } => {
            return Err(TypeError::new(
                "VP8 output not supported for given protocol",
            ));
        }
        VideoEncoderOptions::FfmpegVp9 { .. } => {
            return Err(TypeError::new(
                "VP9 output not supported for given protocol",
            ));
        }
    };

    let output_options = core::RegisterOutputVideoOptions {
        initial: options.initial.try_into()?,
        end_condition: options.send_eos_when.unwrap_or_default().try_into()?,
    };

    Ok((Some(encoder_options), Some(output_options)))
}
