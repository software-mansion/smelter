use std::collections::{HashMap, HashSet};

use tracing::warn;
use webrtc::{
    api::media_engine::MIME_TYPE_H264,
    peer_connection::sdp::session_description::RTCSessionDescription,
    rtp_transceiver::rtp_codec::{RTCRtpCodecCapability, RTCRtpCodecParameters},
};

use super::supported_codec_parameters::get_video_rtcp_feedback;

/// RFC 6184 Section 8.1: "If no profile-level-id is present, the Baseline profile
/// without additional constraints at Level 1 MUST be inferred."
const DEFAULT_PROFILE_LEVEL_ID: &str = "42000a";

/// RFC 6184 Section 8.1: default packetization-mode is 0.
const DEFAULT_PACKETIZATION_MODE: &str = "0";

/// Builds H264 codec parameters by copying every H264 variant from the SDP offer.
///
/// For each offered H264 payload type, we emit a local codec entry with the same
/// payload type and fmtp, so negotiation produces an exact match. This allows
/// accepting any H264 profile/level the peer supports.
///
/// Returns an empty list if no H264 codecs are found in the offer.
pub(crate) fn h264_codecs_from_offer(offer: &RTCSessionDescription) -> Vec<RTCRtpCodecParameters> {
    let Some(session_description) = offer.unmarshal().ok() else {
        warn!("Failed to parse SDP offer for H264 codecs");
        return Vec::new();
    };

    let mut codecs = Vec::new();
    let mut seen = HashSet::new();

    for md in &session_description.media_descriptions {
        if !md.media_name.media.eq_ignore_ascii_case("video") {
            continue;
        }

        let mut h264_payload_types = Vec::new();
        let mut fmtp_by_pt: HashMap<u8, &str> = HashMap::new();

        for attr in &md.attributes {
            let value = attr.value.as_deref().unwrap_or("");
            match attr.key.as_str() {
                "rtpmap" => {
                    let Some((pt_str, codec_desc)) = value.split_once(' ') else {
                        continue;
                    };
                    let Some(pt) = pt_str.trim().parse::<u8>().ok() else {
                        continue;
                    };
                    let codec_name = codec_desc.split('/').next().unwrap_or("");
                    if codec_name.eq_ignore_ascii_case("H264") {
                        h264_payload_types.push(pt);
                    }
                }
                "fmtp" => {
                    if let Some((pt_str, fmtp)) = value.split_once(' ')
                        && let Ok(pt) = pt_str.trim().parse::<u8>()
                    {
                        fmtp_by_pt.insert(pt, fmtp);
                    }
                }
                _ => {}
            }
        }

        for pt in h264_payload_types {
            let (profile_level_id, packetization_mode) = match fmtp_by_pt.get(&pt) {
                Some(fmtp) => parse_h264_fmtp(fmtp),
                None => (DEFAULT_PROFILE_LEVEL_ID, DEFAULT_PACKETIZATION_MODE),
            };

            if !seen.insert((
                pt,
                profile_level_id.to_owned(),
                packetization_mode.to_owned(),
            )) {
                continue;
            }

            codecs.push(RTCRtpCodecParameters {
                capability: RTCRtpCodecCapability {
                    mime_type: MIME_TYPE_H264.to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: format!(
                        "level-asymmetry-allowed=1;packetization-mode={packetization_mode};profile-level-id={profile_level_id}"
                    ),
                    rtcp_feedback: get_video_rtcp_feedback(),
                },
                payload_type: pt,
                ..Default::default()
            });
        }
    }

    codecs
}

/// Extracts profile-level-id and packetization-mode from an H264 fmtp string.
/// Returns RFC 6184 Section 8.1 defaults for any missing parameter.
fn parse_h264_fmtp(fmtp: &str) -> (&str, &str) {
    let mut profile_level_id = DEFAULT_PROFILE_LEVEL_ID;
    let mut packetization_mode = DEFAULT_PACKETIZATION_MODE;

    for param in fmtp.split(';') {
        if let Some((key, val)) = param.trim().split_once('=') {
            match key.trim().to_ascii_lowercase().as_str() {
                "profile-level-id" => profile_level_id = val.trim(),
                "packetization-mode" => packetization_mode = val.trim(),
                _ => {}
            }
        }
    }

    (profile_level_id, packetization_mode)
}
