use std::sync::Arc;

use itertools::Itertools;
use tracing::warn;
use webrtc::{
    peer_connection::sdp::session_description::RTCSessionDescription,
    rtp_transceiver::rtp_codec::RTCRtpCodecParameters,
};

use crate::{
    codecs::VideoDecoderOptions,
    pipeline::webrtc::{
        h264_offer_filter::h264_codecs_from_offer,
        h264_vulkan_capability_filter::filter_h264_codecs_for_vulkan_decoder,
        supported_codec_parameters::{vp8_codec_params, vp9_codec_params},
    },
    prelude::WebrtcVideoDecoderOptions,
};

use crate::prelude::*;

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

/// Builds codec parameters from video preferences, with H264 variants copied directly
/// from the SDP offer. This ensures exact matches during negotiation regardless of
/// which H264 profile/level the peer uses, since ffmpeg can decode any profile.
///
/// For Vulkan H264, the offer-derived codecs are further filtered by hardware capabilities.
pub(super) fn video_params_compliant_with_offer(
    ctx: &Arc<PipelineCtx>,
    video_preferences: &[VideoDecoderOptions],
    offer: &RTCSessionDescription,
) -> Vec<RTCRtpCodecParameters> {
    let codecs: Vec<RTCRtpCodecParameters> = video_preferences
        .iter()
        .flat_map(|pref| match pref {
            VideoDecoderOptions::FfmpegH264 => h264_codecs_from_offer(offer),
            VideoDecoderOptions::VulkanH264 => {
                filter_h264_codecs_for_vulkan_decoder(ctx, h264_codecs_from_offer(offer))
            }
            VideoDecoderOptions::FfmpegVp8 => vp8_codec_params(),
            VideoDecoderOptions::FfmpegVp9 => vp9_codec_params(),
        })
        .collect();

    codecs
}
