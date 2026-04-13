use std::sync::Arc;

use itertools::Itertools;
use tracing::warn;
use webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecParameters;

use crate::{
    codecs::VideoDecoderOptions,
    pipeline::webrtc::{
        h264_vulkan_capability_filter::filter_h264_codecs_for_vulkan_decoder,
        offer_codec_filter::OfferCodecs,
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

/// Builds codec parameters from video preferences, with codec variants copied directly
/// from the SDP offer.
/// For Vulkan H264, the offer-derived codecs are further filtered by hardware capabilities.
pub(super) fn video_params_compliant_with_offer(
    ctx: &Arc<PipelineCtx>,
    video_preferences: &[VideoDecoderOptions],
    offer_codecs: &OfferCodecs,
) -> Vec<RTCRtpCodecParameters> {
    let codecs: Vec<RTCRtpCodecParameters> = video_preferences
        .iter()
        .flat_map(|pref| match pref {
            VideoDecoderOptions::FfmpegH264 => offer_codecs.h264.clone(),
            VideoDecoderOptions::VulkanH264 => {
                filter_h264_codecs_for_vulkan_decoder(ctx, offer_codecs.h264.clone())
            }
            VideoDecoderOptions::FfmpegVp8 => offer_codecs.vp8.clone(),
            VideoDecoderOptions::FfmpegVp9 => offer_codecs.vp9.clone(),
        })
        .unique_by(|codec| {
            (
                codec.payload_type,
                codec.capability.mime_type.clone(),
                codec.capability.sdp_fmtp_line.clone(),
            )
        })
        .collect();

    codecs
}
