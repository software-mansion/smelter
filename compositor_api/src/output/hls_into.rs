use compositor_pipeline::pipeline::{
    self,
    encoder::{self, fdk_aac, ffmpeg_h264},
    output::{self, hls},
};

use crate::*;

impl TryFrom<HlsOutput> for pipeline::RegisterOutputOptions<output::OutputOptions> {
    type Error = TypeError;

    fn try_from(request: HlsOutput) -> Result<Self, Self::Error> {
        let HlsOutput { path, video, audio } = request;

        if video.is_none() && audio.is_none() {
            return Err(TypeError::new(
                "At least one of \"video\" and \"audio\" fields have to be specified.",
            ));
        }

        let (video_encoder_options, output_video_options) = maybe_video_options_h264_only(video)?;
        let (audio_encoder_options, output_audio_options) = maybe_audio_options(audio)?;
        let output_options = output::OutputOptions::Hls(hls::HlsOutputOptions {
            output_path: path.into(),
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

fn maybe_audio_options(
    options: Option<OutputHlsAudioOptions>,
) -> Result<
    (
        Option<encoder::AudioEncoderOptions>,
        Option<pipeline::OutputAudioOptions>,
    ),
    TypeError,
> {
    let Some(OutputHlsAudioOptions {
        mixing_strategy,
        send_eos_when,
        encoder,
        channels,
        initial,
    }) = options
    else {
        return Ok((None, None));
    };

    let (audio_encoder_options, resolved_channels) = match encoder {
        HlsAudioEncoderOptions::Aac { sample_rate } => {
            let resolved_channels = channels.unwrap_or(AudioChannels::Stereo);
            (
                encoder::AudioEncoderOptions::Aac(fdk_aac::AacEncoderOptions {
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

    Ok((Some(audio_encoder_options), Some(output_audio_options)))
}

fn maybe_video_options_h264_only(
    options: Option<OutputVideoOptions>,
) -> Result<
    (
        Option<encoder::VideoEncoderOptions>,
        Option<pipeline::OutputVideoOptions>,
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
        } => pipeline::encoder::VideoEncoderOptions::H264(ffmpeg_h264::Options {
            preset: preset.unwrap_or(H264EncoderPreset::Fast).into(),
            resolution: options.resolution.into(),
            pixel_format: pixel_format.unwrap_or(PixelFormat::Yuv420p).into(),
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
