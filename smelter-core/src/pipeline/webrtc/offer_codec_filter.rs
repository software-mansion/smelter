use std::collections::{HashMap, HashSet};

use tracing::warn;
use webrtc::{
    api::media_engine::{MIME_TYPE_H264, MIME_TYPE_VP8, MIME_TYPE_VP9},
    peer_connection::sdp::session_description::RTCSessionDescription,
    rtp_transceiver::rtp_codec::{RTCRtpCodecCapability, RTCRtpCodecParameters},
};

use super::supported_codec_parameters::get_video_rtcp_feedback;

/// RFC 6184 Section 8.1: "If no profile-level-id is present, the Baseline profile
/// without additional constraints at Level 1 MUST be inferred."
const DEFAULT_PROFILE_LEVEL_ID: &str = "42000a";

/// RFC 6184 Section 8.1: default packetization-mode is 0.
const DEFAULT_PACKETIZATION_MODE: &str = "0";

/// Video codec parameters extracted from an SDP offer, grouped by codec type.
/// Payload types come directly from the offer, so they are guaranteed not to collide.
#[derive(Debug, Clone)]
pub(crate) struct OfferVideoCodecs {
    pub h264: Vec<RTCRtpCodecParameters>,
    pub vp8: Vec<RTCRtpCodecParameters>,
    pub vp9: Vec<RTCRtpCodecParameters>,
}

/// Parses the SDP offer once and extracts all video codec parameters (H264, VP8, VP9).
///
/// For H264, each offered variant is emitted with its original payload type and fmtp,
/// so negotiation produces an exact match regardless of profile/level.
///
/// For VP8/VP9, the payload type is copied from the offer.
pub(crate) fn video_codecs_from_offer(offer: &RTCSessionDescription) -> OfferVideoCodecs {
    let Some(session_description) = offer.unmarshal().ok() else {
        warn!("Failed to parse SDP offer for video codecs");
        return OfferVideoCodecs {
            h264: Vec::new(),
            vp8: Vec::new(),
            vp9: Vec::new(),
        };
    };

    let mut h264_codecs = Vec::new();
    let mut vp8_codecs = Vec::new();
    let mut vp9_codecs = Vec::new();

    let mut h264_seen = HashSet::new();
    let mut vp8_seen = HashSet::new();
    let mut vp9_seen = HashSet::new();

    for md in &session_description.media_descriptions {
        if !md.media_name.media.eq_ignore_ascii_case("video") {
            continue;
        }

        let mut codec_pts: Vec<(u8, &str)> = Vec::new(); // (pt, codec_name)
        let mut fmtp_by_pt: HashMap<u8, &str> = HashMap::new();

        for attr in &md.attributes {
            let value = attr.value.as_deref().unwrap_or("");
            match attr.key.to_ascii_lowercase().as_str() {
                "rtpmap" => {
                    let Some((pt_str, codec_desc)) = value.split_once(' ') else {
                        continue;
                    };
                    let Some(pt) = pt_str.trim().parse::<u8>().ok() else {
                        continue;
                    };
                    let codec_name = codec_desc.split('/').next().unwrap_or("");
                    if codec_name.eq_ignore_ascii_case("H264")
                        || codec_name.eq_ignore_ascii_case("VP8")
                        || codec_name.eq_ignore_ascii_case("VP9")
                    {
                        codec_pts.push((pt, codec_name));
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

        for (pt, codec_name) in codec_pts {
            if codec_name.eq_ignore_ascii_case("H264") {
                let (profile_level_id, packetization_mode) = match fmtp_by_pt.get(&pt) {
                    Some(fmtp) => parse_h264_fmtp(fmtp),
                    None => (DEFAULT_PROFILE_LEVEL_ID, DEFAULT_PACKETIZATION_MODE),
                };

                if !h264_seen.insert((
                    pt,
                    profile_level_id.to_owned(),
                    packetization_mode.to_owned(),
                )) {
                    continue;
                }

                h264_codecs.push(RTCRtpCodecParameters {
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
            } else if codec_name.eq_ignore_ascii_case("VP8") && vp8_seen.insert(pt) {
                vp8_codecs.push(RTCRtpCodecParameters {
                    capability: RTCRtpCodecCapability {
                        mime_type: MIME_TYPE_VP8.to_owned(),
                        clock_rate: 90000,
                        channels: 0,
                        sdp_fmtp_line: "".to_owned(),
                        rtcp_feedback: get_video_rtcp_feedback(),
                    },
                    payload_type: pt,
                    ..Default::default()
                });
            } else if codec_name.eq_ignore_ascii_case("VP9") && vp9_seen.insert(pt) {
                vp9_codecs.push(RTCRtpCodecParameters {
                    capability: RTCRtpCodecCapability {
                        mime_type: MIME_TYPE_VP9.to_owned(),
                        clock_rate: 90000,
                        channels: 0,
                        sdp_fmtp_line: "".to_owned(),
                        rtcp_feedback: get_video_rtcp_feedback(),
                    },
                    payload_type: pt,
                    ..Default::default()
                });
            }
        }
    }

    OfferVideoCodecs {
        h264: h264_codecs,
        vp8: vp8_codecs,
        vp9: vp9_codecs,
    }
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
