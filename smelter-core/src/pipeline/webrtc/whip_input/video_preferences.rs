use std::sync::Arc;

use itertools::Itertools;
use tracing::warn;
use webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecParameters;

use crate::{
    PipelineCtx,
    codecs::{VideoDecoderOptions, WebrtcVideoDecoderOptions},
    error::{DecoderInitError, InputInitError},
    pipeline::webrtc::supported_codec_parameters::{
        h264_codec_params, vp8_codec_params, vp9_codec_params,
    },
};

pub(super) fn resolve_video_preferences(
    ctx: &Arc<PipelineCtx>,
    video_preferences: Vec<WebrtcVideoDecoderOptions>,
) -> Result<Vec<VideoDecoderOptions>, InputInitError> {
    let vulkan_supported = ctx.graphics_context.has_vulkan_decoder_support();
    let only_vulkan_in_preferences = video_preferences
        .iter()
        .all(|pref| matches!(pref, WebrtcVideoDecoderOptions::VulkanH264));
    if !vulkan_supported && only_vulkan_in_preferences {
        return Err(InputInitError::DecoderError(
            DecoderInitError::VulkanContextRequiredForVulkanDecoder,
        ));
    };

    let video_preferences: Vec<VideoDecoderOptions> = video_preferences
        .into_iter()
        .flat_map(|preference| match preference {
            WebrtcVideoDecoderOptions::FfmpegH264 => vec![VideoDecoderOptions::FfmpegH264],
            WebrtcVideoDecoderOptions::VulkanH264 => {
                if vulkan_supported {
                    vec![VideoDecoderOptions::VulkanH264]
                } else {
                    warn!("Vulkan is not supported, skipping \"vulkan_h264\" preference");
                    vec![]
                }
            }
            WebrtcVideoDecoderOptions::FfmpegVp8 => vec![VideoDecoderOptions::FfmpegVp8],
            WebrtcVideoDecoderOptions::FfmpegVp9 => vec![VideoDecoderOptions::FfmpegVp9],
            WebrtcVideoDecoderOptions::Any => {
                vec![
                    VideoDecoderOptions::FfmpegVp9,
                    VideoDecoderOptions::FfmpegVp8,
                    if vulkan_supported {
                        VideoDecoderOptions::VulkanH264
                    } else {
                        VideoDecoderOptions::FfmpegH264
                    },
                ]
            }
        })
        .unique()
        .collect();
    Ok(video_preferences)
}

pub(super) fn params_from_video_preferences(
    video_preferences: &[VideoDecoderOptions],
) -> Vec<RTCRtpCodecParameters> {
    video_preferences
        .iter()
        .flat_map(|pref| match pref {
            VideoDecoderOptions::FfmpegH264 | VideoDecoderOptions::VulkanH264 => {
                h264_codec_params()
            }
            VideoDecoderOptions::FfmpegVp8 => vp8_codec_params(),
            VideoDecoderOptions::FfmpegVp9 => vp9_codec_params(),
        })
        .unique_by(|c| {
            (
                c.capability.mime_type.clone(),
                c.capability.sdp_fmtp_line.clone(),
            )
        })
        .collect()
}
