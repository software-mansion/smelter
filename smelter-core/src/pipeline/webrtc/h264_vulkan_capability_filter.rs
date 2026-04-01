use std::sync::Arc;

use webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecParameters;

use crate::pipeline::PipelineCtx;

pub(crate) fn filter_h264_codecs_for_vulkan_encoder(
    ctx: &Arc<PipelineCtx>,
    codecs: Vec<RTCRtpCodecParameters>,
) -> Vec<RTCRtpCodecParameters> {
    let Some(support) = ctx
        .graphics_context
        .vulkan_h264_encode_profile_level_support()
    else {
        return codecs;
    };

    filter_h264_codecs_by_profile_level_support(codecs, support)
}

pub(crate) fn filter_h264_codecs_for_vulkan_decoder(
    ctx: &Arc<PipelineCtx>,
    codecs: Vec<RTCRtpCodecParameters>,
) -> Vec<RTCRtpCodecParameters> {
    let Some(support) = ctx
        .graphics_context
        .vulkan_h264_decode_profile_level_support()
    else {
        return codecs;
    };

    filter_h264_codecs_by_profile_level_support(codecs, support)
}

fn filter_h264_codecs_by_profile_level_support(
    codecs: Vec<RTCRtpCodecParameters>,
    support: crate::graphics_context::H264ProfileLevelSupport,
) -> Vec<RTCRtpCodecParameters> {
    codecs
        .into_iter()
        .filter(|codec| {
            h264_profile_level_idc_from_fmtp(&codec.capability.sdp_fmtp_line).is_none_or(
                |(profile_idc, level_idc)| {
                    support
                        .max_level_for_profile(profile_idc)
                        .is_some_and(|max_level_idc| level_idc <= max_level_idc)
                },
            )
        })
        .collect()
}

fn h264_profile_level_idc_from_fmtp(fmtp: &str) -> Option<(u8, u8)> {
    for param in fmtp.split(';') {
        let (key, val) = param.trim().split_once('=')?;
        if !key.trim().eq_ignore_ascii_case("profile-level-id") {
            continue;
        }

        let plid = val.trim();
        if plid.len() != 6 {
            return None;
        }

        let profile_idc = u8::from_str_radix(&plid[0..2], 16).ok()?;
        let level_idc = u8::from_str_radix(&plid[4..6], 16).ok()?;
        return Some((profile_idc, level_idc));
    }

    None
}
