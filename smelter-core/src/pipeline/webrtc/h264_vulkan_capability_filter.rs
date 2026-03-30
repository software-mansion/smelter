use std::sync::Arc;

use webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecParameters;

use crate::pipeline::PipelineCtx;

#[derive(Debug, Clone, Copy)]
struct H264ProfileLevelSupport {
    baseline_max_level_idc: Option<u8>,
    main_max_level_idc: Option<u8>,
    high_max_level_idc: Option<u8>,
}

impl H264ProfileLevelSupport {
    fn max_level_for_profile(self, profile_idc: u8) -> Option<u8> {
        match profile_idc {
            0x42 => self.baseline_max_level_idc,
            0x4d => self.main_max_level_idc,
            0x64 => self.high_max_level_idc,
            _ => None,
        }
    }
}

pub(crate) fn filter_h264_codecs_for_vulkan_encoder(
    ctx: &Arc<PipelineCtx>,
    codecs: Vec<RTCRtpCodecParameters>,
) -> Vec<RTCRtpCodecParameters> {
    let Some(support) = vulkan_h264_encode_profile_level_support(ctx) else {
        return codecs;
    };

    filter_h264_codecs_by_profile_level_support(codecs, support)
}

pub(crate) fn filter_h264_codecs_for_vulkan_decoder(
    ctx: &Arc<PipelineCtx>,
    codecs: Vec<RTCRtpCodecParameters>,
) -> Vec<RTCRtpCodecParameters> {
    let Some(support) = vulkan_h264_decode_profile_level_support(ctx) else {
        return codecs;
    };

    filter_h264_codecs_by_profile_level_support(codecs, support)
}

fn filter_h264_codecs_by_profile_level_support(
    codecs: Vec<RTCRtpCodecParameters>,
    support: H264ProfileLevelSupport,
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

#[cfg(feature = "vk-video")]
fn vulkan_h264_encode_profile_level_support(
    ctx: &Arc<PipelineCtx>,
) -> Option<H264ProfileLevelSupport> {
    let vulkan_ctx = ctx.graphics_context.vulkan_ctx.as_ref()?;
    let caps = vulkan_ctx.device.encode_capabilities().h264?;

    Some(H264ProfileLevelSupport {
        baseline_max_level_idc: caps.baseline_profile.map(|p| p.max_level_idc),
        main_max_level_idc: caps.main_profile.map(|p| p.max_level_idc),
        high_max_level_idc: caps.high_profile.map(|p| p.max_level_idc),
    })
}

#[cfg(not(feature = "vk-video"))]
fn vulkan_h264_encode_profile_level_support(
    _ctx: &Arc<PipelineCtx>,
) -> Option<H264ProfileLevelSupport> {
    None
}

#[cfg(feature = "vk-video")]
fn vulkan_h264_decode_profile_level_support(
    ctx: &Arc<PipelineCtx>,
) -> Option<H264ProfileLevelSupport> {
    let vulkan_ctx = ctx.graphics_context.vulkan_ctx.as_ref()?;
    let caps = vulkan_ctx.device.decode_capabilities().h264?;

    Some(H264ProfileLevelSupport {
        baseline_max_level_idc: caps.baseline_profile.map(|p| p.max_level_idc),
        main_max_level_idc: caps.main_profile.map(|p| p.max_level_idc),
        high_max_level_idc: caps.high_profile.map(|p| p.max_level_idc),
    })
}

#[cfg(not(feature = "vk-video"))]
fn vulkan_h264_decode_profile_level_support(
    _ctx: &Arc<PipelineCtx>,
) -> Option<H264ProfileLevelSupport> {
    None
}
