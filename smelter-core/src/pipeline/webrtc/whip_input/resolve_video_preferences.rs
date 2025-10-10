use std::sync::Arc;

use itertools::Itertools;
use tracing::warn;
use webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecParameters;

use crate::{
    PipelineCtx,
    codecs::{VideoDecoderOptions, WebrtcVideoDecoderOptions},
    error::{DecoderInitError, InputInitError},
    pipeline::webrtc::supported_codec_parameters::{
        h264_codec_params_default_payload_type, vp8_codec_params_default_payload_type,
        vp9_codec_params_default_payload_type,
    },
};

pub(super) fn resolve_video_preferences(
    ctx: &Arc<PipelineCtx>,
    video_preferences: Vec<WebrtcVideoDecoderOptions>,
) -> Result<(Vec<VideoDecoderOptions>, Vec<RTCRtpCodecParameters>), InputInitError> {
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

    // When setting codec preferences, payload types should be compatible with those in the offer. Simplest way to achieve that is by setting defaults
    let mut video_codecs_params: Vec<RTCRtpCodecParameters> = Vec::new();
    for pref in &video_preferences {
        match pref {
            VideoDecoderOptions::FfmpegH264 | VideoDecoderOptions::VulkanH264 => {
                video_codecs_params.extend(h264_codec_params_default_payload_type())
            }
            VideoDecoderOptions::FfmpegVp8 => {
                video_codecs_params.extend(vp8_codec_params_default_payload_type())
            }
            VideoDecoderOptions::FfmpegVp9 => {
                video_codecs_params.extend(vp9_codec_params_default_payload_type())
            }
        }
    }

    let video_codecs_params = video_codecs_params
        .into_iter()
        .unique_by(|c| {
            (
                c.capability.mime_type.clone(),
                c.capability.sdp_fmtp_line.clone(),
            )
        })
        .collect();
    Ok((video_preferences, video_codecs_params))
}
