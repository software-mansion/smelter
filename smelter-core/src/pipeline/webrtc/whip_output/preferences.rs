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
        .all(|pref| matches!(pref, WebrtcVideoEncoderOptions::VulkanH264(_)));
    if !vulkan_supported && only_vulkan_in_preferences {
        return Err(WebrtcClientError::EncoderInitError(
            EncoderInitError::VulkanContextRequiredForVulkanEncoder,
        ));
    }

    let video_preferences: Vec<VideoEncoderOptions> = video_preferences
        .into_iter()
        .flat_map(|preference| match preference {
            WebrtcVideoEncoderOptions::FfmpegH264(opts) => {
                vec![VideoEncoderOptions::FfmpegH264(opts)]
            }
            WebrtcVideoEncoderOptions::VulkanH264(opts) => {
                if vulkan_supported {
                    vec![VideoEncoderOptions::VulkanH264(opts)]
                } else {
                    warn!("Vulkan is not supported, skipping \"vulkan_h264\" preference");
                    vec![]
                }
            }
            WebrtcVideoEncoderOptions::FfmpegVp8(opts) => {
                vec![VideoEncoderOptions::FfmpegVp8(opts)]
            }
            WebrtcVideoEncoderOptions::FfmpegVp9(opts) => {
                vec![VideoEncoderOptions::FfmpegVp9(opts)]
            }
            WebrtcVideoEncoderOptions::Any(resolution) => {
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

pub(super) fn params_from_video_preferences(
    video_preferences: &Option<Vec<VideoEncoderOptions>>,
) -> Vec<RTCRtpCodecParameters> {
    // default codecs register to make Twitch work even without video
    let Some(video_preferences) = video_preferences else {
        return h264_codec_params();
    };

    video_preferences
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
        .collect()
}

pub(super) fn resolve_audio_preferences(
    options: &WhipOutputOptions,
) -> Option<Vec<AudioEncoderOptions>> {
    let audio_preferences = options.clone().audio.map(|v| v.encoder_preferences)?;

    let audio_preferences = audio_preferences
        .into_iter()
        .flat_map(|preference| match preference {
            WebrtcAudioEncoderOptions::Opus(opts) => {
                vec![AudioEncoderOptions::Opus(opts)]
            }
            WebrtcAudioEncoderOptions::Any(channels) => {
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

pub(super) fn params_from_audio_preferences(
    audio_preferences: &Option<Vec<AudioEncoderOptions>>,
) -> Vec<RTCRtpCodecParameters> {
    // Opus is the only supported codec. The only negotiable option in AudioEncoderOptions is FEC.
    // Since FEC is the only variant, we can just check the first option’s FEC value
    // and register Opus with/without FEC accordingly, in the preferred order.
    // Channels field is the same for all encoder preferences.
    let fec_first = audio_preferences
        .as_ref()
        .and_then(|prefs| prefs.first())
        .and_then(|opt| match opt {
            AudioEncoderOptions::Opus(opts) => Some(opts.forward_error_correction),
            _ => None,
        })
        .unwrap_or(true);
    opus_codec_params(fec_first)
}
