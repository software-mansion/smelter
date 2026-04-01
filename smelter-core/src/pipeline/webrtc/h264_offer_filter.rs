use std::collections::HashSet;

use tracing::warn;
use webrtc::{
    api::media_engine::MIME_TYPE_H264,
    peer_connection::sdp::session_description::RTCSessionDescription,
    rtp_transceiver::rtp_codec::{RTCRtpCodecCapability, RTCRtpCodecParameters},
};

use super::supported_codec_parameters::get_video_rtcp_feedback;

#[derive(Debug, Clone)]
struct OfferH264Params {
    payload_type: u8,
    profile_level_id: Option<String>,
    packetization_mode: String,
}

#[derive(Debug, Clone)]
struct ParsedH264Fmtp {
    profile_level_id: Option<String>,
    packetization_mode: String,
}

/// Builds H264 codec parameters by copying every H264 variant from the SDP offer.
///
/// For each offered H264 payload type, we emit a local codec entry with the same
/// payload type and fmtp, so negotiation produces an exact match. This allows
/// accepting any H264 profile/level the peer supports.
///
/// Returns an empty list if no H264 codecs are found in the offer.
pub(crate) fn h264_codecs_from_offer(offer: &RTCSessionDescription) -> Vec<RTCRtpCodecParameters> {
    let offer_h264_params = match extract_h264_params_from_offer(offer) {
        Some(params) if !params.is_empty() => params,
        Some(_) => return Vec::new(),
        None => {
            warn!("Failed to parse SDP offer for H264 codecs");
            return Vec::new();
        }
    };

    let mut codecs = Vec::new();
    let mut seen = HashSet::new();
    for offer in offer_h264_params {
        if !seen.insert((
            offer.payload_type,
            offer.packetization_mode.clone(),
            offer.profile_level_id.clone(),
        )) {
            continue;
        }

        codecs.push(RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: MIME_TYPE_H264.to_owned(),
                clock_rate: 90000,
                channels: 0,
                sdp_fmtp_line: build_h264_fmtp(
                    offer.profile_level_id.as_deref(),
                    offer.packetization_mode.as_str(),
                ),
                rtcp_feedback: get_video_rtcp_feedback(),
            },
            payload_type: offer.payload_type,
            ..Default::default()
        });
    }

    codecs
}

/// Extracts H264 payload parameters from offer fmtp/rtpmap lines.
fn extract_h264_params_from_offer(offer: &RTCSessionDescription) -> Option<Vec<OfferH264Params>> {
    let session_description = offer.unmarshal().ok()?;
    let mut h264_payload_types = HashSet::new();
    let mut params = Vec::new();

    for md in &session_description.media_descriptions {
        if !md.media_name.media.eq_ignore_ascii_case("video") {
            continue;
        }

        for attr in &md.attributes {
            if !attr.key.eq_ignore_ascii_case("rtpmap") {
                continue;
            }
            let value = attr.value.as_deref().unwrap_or("");
            let Some((pt, codec_desc)) = value.split_once(' ') else {
                continue;
            };
            let Some(payload_type) = pt.trim().parse::<u8>().ok() else {
                continue;
            };

            let codec_name = codec_desc.split('/').next().unwrap_or("");
            if codec_name.eq_ignore_ascii_case("H264") {
                h264_payload_types.insert(payload_type);
            }
        }

        for attr in &md.attributes {
            if !attr.key.eq_ignore_ascii_case("fmtp") {
                continue;
            }
            let value = attr.value.as_deref().unwrap_or("");
            // fmtp value format: "<pt> <params>"
            let (pt, fmtp) = match value.split_once(' ') {
                Some((pt, fmtp)) => (pt, fmtp),
                None => continue,
            };
            let Some(payload_type) = pt.trim().parse::<u8>().ok() else {
                continue;
            };
            if !h264_payload_types.contains(&payload_type) {
                continue;
            }

            if let Some(parsed) = parse_h264_fmtp(fmtp) {
                params.push(OfferH264Params {
                    payload_type,
                    profile_level_id: parsed.profile_level_id,
                    packetization_mode: parsed.packetization_mode,
                });
            }
        }
    }

    Some(params)
}

/// Parses H264-related fmtp attributes.
fn parse_h264_fmtp(fmtp: &str) -> Option<ParsedH264Fmtp> {
    let mut profile_level_id = None;
    // RFC 6184 default packetization mode is 0.
    let mut packetization_mode = "0".to_string();

    for param in fmtp.split(';') {
        let param = param.trim();
        if let Some((key, val)) = param.split_once('=') {
            match key.trim().to_ascii_lowercase().as_str() {
                "profile-level-id" => {
                    profile_level_id = Some(val.trim().to_ascii_lowercase());
                }
                "packetization-mode" => {
                    packetization_mode = val.trim().to_owned();
                }
                _ => {}
            }
        }
    }

    Some(ParsedH264Fmtp {
        profile_level_id,
        packetization_mode,
    })
}

fn build_h264_fmtp(profile_level_id: Option<&str>, packetization_mode: &str) -> String {
    match profile_level_id {
        Some(plid) => format!(
            "level-asymmetry-allowed=1;packetization-mode={packetization_mode};profile-level-id={plid}"
        ),
        None => format!("level-asymmetry-allowed=1;packetization-mode={packetization_mode}"),
    }
}
