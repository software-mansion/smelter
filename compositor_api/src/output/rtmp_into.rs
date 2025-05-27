use compositor_pipeline::pipeline::{
    self,
    encoder::{fdk_aac::AacEncoderOptions, ffmpeg_h264, AudioEncoderOptions, OutPixelFormat},
    output::{self, rtmp::RtmpSenderOptions},
};
use tracing::warn;

use crate::*;

impl TryFrom<RtmpClient> for pipeline::RegisterOutputOptions<output::OutputOptions> {
    type Error = TypeError;

    fn try_from(value: RtmpClient) -> Result<Self, Self::Error> {
        let RtmpClient { url, video, audio } = value;

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
                    RtmpClientAudioEncoderOptions::Aac {
                        sample_rate,
                        channels: channels_deprecated,
                    } => {
                        if channels_deprecated.is_some() {
                            warn!("The 'channels' field within the encoder options is deprecated and will be removed in future releases. Please use the 'channels' field in the audio options for setting the audio channels.");
                        }
                        let resolved_channels = channels
                            .or(channels_deprecated)
                            .unwrap_or(AudioChannels::Stereo);
                        (
                            AudioEncoderOptions::Aac(AacEncoderOptions {
                                channels: resolved_channels.clone().into(),
                                sample_rate: sample_rate.unwrap_or(44100),
                            }),
                            resolved_channels,
                        )
                    }
                };
                let output_audio_options = pipeline::OutputAudioOptions {
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

        let output_options = output::OutputOptions::Rtmp(RtmpSenderOptions {
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
        Option<pipeline::encoder::VideoEncoderOptions>,
        Option<pipeline::OutputVideoOptions>,
    ),
    TypeError,
> {
    let Some(options) = options else {
        return Ok((None, None));
    };
    let pixel_format: OutPixelFormat = options.pixel_format.unwrap_or(PixelFormat::Yuv420p).into();

    let encoder_options = match options.encoder {
        VideoEncoderOptions::FfmpegH264 {
            preset,
            ffmpeg_options,
        } => pipeline::encoder::VideoEncoderOptions::H264(ffmpeg_h264::Options {
            preset: preset.unwrap_or(H264EncoderPreset::Fast).into(),
            resolution: options.resolution.into(),
            pixel_format,
            raw_options: ffmpeg_options.unwrap_or_default().into_iter().collect(),
        }),
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

    let output_options = pipeline::OutputVideoOptions {
        initial: options.initial.try_into()?,
        end_condition: options.send_eos_when.unwrap_or_default().try_into()?,
    };

    Ok((Some(encoder_options), Some(output_options)))
}
