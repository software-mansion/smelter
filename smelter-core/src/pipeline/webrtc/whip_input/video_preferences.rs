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
        h264_offer_filter::filter_h264_codecs_by_offer,
        h264_vulkan_capability_filter::filter_h264_codecs_for_vulkan_decoder,
        supported_codec_parameters::{h264_codec_params, vp8_codec_params, vp9_codec_params},
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

/// Builds codec parameters from video preferences, filtered to only include H264 variants
/// whose exact `profile-level-id` + `packetization-mode` appear in the SDP offer.
///
/// This works around a webrtc-rs bug where the SDP answer can contain H264 fmtp parameters
/// from our codec preferences instead of from the negotiated (offer) codecs.
pub(super) fn video_params_compliant_with_offer(
    ctx: &Arc<PipelineCtx>,
    video_preferences: &[VideoDecoderOptions],
    offer: &RTCSessionDescription,
) -> Vec<RTCRtpCodecParameters> {
    let codecs = params_from_video_preferences(video_preferences);
    let filtered_by_offer_h264_codecs = filter_h264_codecs_by_offer(offer, codecs);
    if uses_vulkan_h264(video_preferences) {
        filter_h264_codecs_for_vulkan_decoder(ctx, filtered_by_offer_h264_codecs)
    } else {
        filtered_by_offer_h264_codecs
    }
}

fn uses_vulkan_h264(video_preferences: &[VideoDecoderOptions]) -> bool {
    video_preferences
        .iter()
        .any(|pref| matches!(pref, VideoDecoderOptions::VulkanH264))
}

fn params_from_video_preferences(
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
