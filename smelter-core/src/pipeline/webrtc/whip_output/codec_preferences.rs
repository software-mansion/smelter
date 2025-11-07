use std::sync::Arc;

use itertools::Itertools;
use tracing::warn;
use webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecParameters;

use crate::{
    pipeline::webrtc::supported_codec_parameters::{
        h264_codec_params, opus_codec_params, vp8_codec_params, vp9_codec_params,
    },
    prelude::*,
};

pub(super) fn resolve_video_preferences(
    ctx: &Arc<PipelineCtx>,
    options: &WhipOutputOptions,
) -> Result<Option<Vec<VideoEncoderOptions>>, WebrtcClientError> {
    let Some(video_preferences) = options.clone().video.map(|v| v.encoder_preferences) else {
        return Ok(None);
    };

    let vulkan_supported = ctx.graphics_context.has_vulkan_encoder_support();
    let only_vulkan_in_preferences = video_preferences
        .iter()
        .all(|pref| matches!(pref, WhipVideoEncoderOptions::VulkanH264(_)));
    if !vulkan_supported && only_vulkan_in_preferences {
        return Err(WebrtcClientError::EncoderInitError(
            EncoderInitError::VulkanContextRequiredForVulkanEncoder,
        ));
    }

    let video_preferences: Vec<VideoEncoderOptions> = video_preferences
        .into_iter()
        .flat_map(|preference| match preference {
            WhipVideoEncoderOptions::FfmpegH264(opts) => {
                vec![VideoEncoderOptions::FfmpegH264(opts)]
            }
            WhipVideoEncoderOptions::VulkanH264(opts) => {
                if vulkan_supported {
                    vec![VideoEncoderOptions::VulkanH264(opts)]
                } else {
                    warn!("Vulkan is not supported, skipping \"vulkan_h264\" preference");
                    vec![]
                }
            }
            WhipVideoEncoderOptions::FfmpegVp8(opts) => {
                vec![VideoEncoderOptions::FfmpegVp8(opts)]
            }
            WhipVideoEncoderOptions::FfmpegVp9(opts) => {
                vec![VideoEncoderOptions::FfmpegVp9(opts)]
            }
            WhipVideoEncoderOptions::Any(resolution) => {
                vec![
                    VideoEncoderOptions::FfmpegVp9(FfmpegVp9EncoderOptions {
                        resolution,
                        pixel_format: OutputPixelFormat::YUV420P,
                        raw_options: Vec::new(),
                    }),
                    VideoEncoderOptions::FfmpegVp8(FfmpegVp8EncoderOptions {
                        resolution,
                        raw_options: Vec::new(),
                    }),
                    if vulkan_supported {
                        VideoEncoderOptions::VulkanH264(VulkanH264EncoderOptions {
                            resolution,
                            bitrate: None,
                        })
                    } else {
                        VideoEncoderOptions::FfmpegH264(FfmpegH264EncoderOptions {
                            preset: FfmpegH264EncoderPreset::Fast,
                            resolution,
                            pixel_format: OutputPixelFormat::YUV420P,
                            raw_options: Vec::new(),
                        })
                    },
                ]
            }
        })
        .unique()
        .collect();

    Ok(Some(video_preferences))
}

pub(super) fn resolve_audio_preferences(
    options: &WhipOutputOptions,
) -> Option<Vec<AudioEncoderOptions>> {
    let audio_preferences = options.clone().audio.map(|v| v.encoder_preferences)?;

    let audio_preferences = audio_preferences
        .into_iter()
        .flat_map(|preference| match preference {
            WhipAudioEncoderOptions::Opus(opts) => {
                vec![AudioEncoderOptions::Opus(opts)]
            }
            WhipAudioEncoderOptions::Any(channels) => {
                vec![AudioEncoderOptions::Opus(OpusEncoderOptions {
                    channels,
                    preset: OpusEncoderPreset::Voip,
                    sample_rate: 48000,
                    forward_error_correction: true,
                    packet_loss: 0,
                })]
            }
        })
        .unique()
        .collect();

    Some(audio_preferences)
}

pub(super) struct CodecParameters {
    pub video_codecs: Vec<RTCRtpCodecParameters>,
    pub audio_codecs: Vec<RTCRtpCodecParameters>,
}

pub(super) fn codec_params_from_preferences(
    video_preferences: &Option<Vec<VideoEncoderOptions>>,
    audio_preferences: &Option<Vec<AudioEncoderOptions>>,
) -> CodecParameters {
    let video_codecs = match video_preferences {
        Some(video_preferences) => video_preferences
            .iter()
            .flat_map(|pref| match pref {
                VideoEncoderOptions::FfmpegH264(_) | VideoEncoderOptions::VulkanH264(_) => {
                    h264_codec_params()
                }
                VideoEncoderOptions::FfmpegVp8(_) => vp8_codec_params(),
                VideoEncoderOptions::FfmpegVp9(_) => vp9_codec_params(),
            })
            .unique_by(|c| {
                (
                    c.capability.mime_type.clone(),
                    c.capability.sdp_fmtp_line.clone(),
                )
            })
            .collect(),
        None => h264_codec_params(), // default codecs register to make audio-only stream work
    };

    // Opus is the only supported codec. The only negotiable option in AudioEncoderOptions is FEC.
    // Since FEC is the only variant, we can just check the first option’s FEC value
    // and register Opus with/without FEC accordingly, in the preferred order.
    // Channels field is the same for all encoder preferences.
    let (fec_first, channels) = audio_preferences
        .as_ref()
        .and_then(|prefs| prefs.first())
        .and_then(|opt| match opt {
            AudioEncoderOptions::Opus(opts) => Some((opts.forward_error_correction, opts.channels)),
            _ => None,
        })
        .unwrap_or((true, AudioChannels::Stereo));

    let audio_codecs = opus_codec_params(fec_first, channels);

    CodecParameters {
        video_codecs,
        audio_codecs,
    }
}
