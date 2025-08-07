use crate::common_pipeline::prelude as pipeline;
use crate::output::whep::{OutputWhepAudioOptions, WhepAudioEncoderOptions, WhepOutput};
use crate::*;

impl TryFrom<WhepOutput> for pipeline::RegisterOutputOptions {
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

        let (video_encoder_options, output_video_options) = video
            .map(resolve_video_options)
            .transpose()?
            .unwrap_or_default();

        let (output_audio_options, audio_encoder_options) = audio
            .map(resolve_audio_options)
            .transpose()?
            .unwrap_or_default();

        Ok(Self {
            output_options: pipeline::ProtocolOutputOptions::Whep(pipeline::WhepSenderOptions {
                bearer_token,
                video: video_encoder_options,
                audio: audio_encoder_options,
            }),
            video: output_video_options,
            audio: output_audio_options,
        })
    }
}

fn resolve_video_options(
    options: OutputVideoOptions,
) -> Result<
    (
        Option<pipeline::VideoEncoderOptions>,
        Option<pipeline::RegisterOutputVideoOptions>,
    ),
    TypeError,
> {
    let encoder_options = match options.encoder {
        VideoEncoderOptions::FfmpegH264 {
            preset,
            pixel_format,
            ffmpeg_options,
        } => pipeline::VideoEncoderOptions::FfmpegH264(pipeline::FfmpegH264EncoderOptions {
            preset: preset.unwrap_or(H264EncoderPreset::Fast).into(),
            resolution: options.resolution.into(),
            pixel_format: pixel_format.unwrap_or(PixelFormat::Yuv420p).into(),
            raw_options: ffmpeg_options.unwrap_or_default().into_iter().collect(),
        }),
        VideoEncoderOptions::FfmpegVp8 { ffmpeg_options } => {
            pipeline::VideoEncoderOptions::FfmpegVp8(pipeline::FfmpegVp8EncoderOptions {
                resolution: options.resolution.into(),
                raw_options: ffmpeg_options.unwrap_or_default().into_iter().collect(),
            })
        }
        VideoEncoderOptions::FfmpegVp9 {
            pixel_format,
            ffmpeg_options,
        } => pipeline::VideoEncoderOptions::FfmpegVp9(pipeline::FfmpegVp9EncoderOptions {
            resolution: options.resolution.into(),
            pixel_format: pixel_format.unwrap_or(PixelFormat::Yuv420p).into(),
            raw_options: ffmpeg_options.unwrap_or_default().into_iter().collect(),
        }),
    };

    let output_options = pipeline::RegisterOutputVideoOptions {
        initial: options.initial.try_into()?,
        end_condition: options.send_eos_when.unwrap_or_default().try_into()?,
    };

    Ok((Some(encoder_options), Some(output_options)))
}

fn resolve_audio_options(
    options: OutputWhepAudioOptions,
) -> Result<
    (
        Option<pipeline::RegisterOutputAudioOptions>,
        Option<pipeline::AudioEncoderOptions>,
    ),
    TypeError,
> {
    let OutputWhepAudioOptions {
        mixing_strategy,
        send_eos_when,
        encoder,
        channels,
        initial,
    } = options;

    let (audio_encoder_options, resolved_channels) = match encoder {
        WhepAudioEncoderOptions::Opus {
            preset,
            sample_rate,
            forward_error_correction,
            expected_packet_loss,
        } => {
            let channels = channels.unwrap_or(AudioChannels::Stereo);
            let packet_loss = match expected_packet_loss {
                Some(x) if x > 100 => {
                    return Err(TypeError::new(
                        "Expected packet loss value must be from [0, 100] range.",
                    ))
                }
                Some(x) => x as i32,
                None => 0,
            };

            (
                pipeline::AudioEncoderOptions::Opus(pipeline::OpusEncoderOptions {
                    channels: channels.clone().into(),
                    preset: preset.unwrap_or(OpusEncoderPreset::Voip).into(),
                    sample_rate: sample_rate.unwrap_or(48_000),
                    forward_error_correction: forward_error_correction.unwrap_or(false),
                    packet_loss,
                }),
                channels,
            )
        }
    };

    let output_audio_options = pipeline::RegisterOutputAudioOptions {
        initial: initial.try_into()?,
        end_condition: send_eos_when.unwrap_or_default().try_into()?,
        mixing_strategy: mixing_strategy
            .unwrap_or(AudioMixingStrategy::SumClip)
            .into(),
        channels: resolved_channels.into(),
    };

    Ok((Some(output_audio_options), Some(audio_encoder_options)))
}
