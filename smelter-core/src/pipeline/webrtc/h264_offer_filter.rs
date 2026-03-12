use tracing::warn;
use webrtc::{
    api::media_engine::MIME_TYPE_H264,
    peer_connection::sdp::session_description::RTCSessionDescription,
    rtp_transceiver::rtp_codec::RTCRtpCodecParameters,
};

/// Filters H264 codecs to only include variants whose exact `profile-level-id` +
/// `packetization-mode` appear in the SDP offer. This prevents webrtc-rs from producing
/// an SDP answer with multiple H264 fmtp lines sharing the same payload type.
pub(crate) fn filter_h264_codecs_by_offer(
    offer: &RTCSessionDescription,
    codecs: Vec<RTCRtpCodecParameters>,
) -> Vec<RTCRtpCodecParameters> {
    let offer_h264_params = match extract_h264_params_from_offer(offer) {
        Some(params) if !params.is_empty() => params,
        Some(_) => {
            // Offer has no H264 fmtp lines (or they lack profile-level-id/packetization-mode).
            // Skip filtering to let webrtc-rs negotiate freely.
            return codecs;
        }
        None => {
            warn!("Failed to parse SDP offer for H264 codec filtering, using all codecs");
            return codecs;
        }
    };

    codecs
        .into_iter()
        .filter(|codec| {
            if codec.capability.mime_type != MIME_TYPE_H264 {
                return true;
            }

            let Some((plid, pmode)) = parse_h264_fmtp(&codec.capability.sdp_fmtp_line) else {
                return true;
            };

            offer_h264_params
                .iter()
                .any(|(offer_plid, offer_pmode)| *offer_plid == plid && *offer_pmode == pmode)
        })
        .collect()
}

/// Extracts (profile-level-id, packetization-mode) pairs from H264 fmtp lines in the offer.
fn extract_h264_params_from_offer(offer: &RTCSessionDescription) -> Option<Vec<(String, String)>> {
    let session_description = offer.unmarshal().ok()?;
    let mut params = Vec::new();

    for md in &session_description.media_descriptions {
        if !md.media_name.media.eq_ignore_ascii_case("video") {
            continue;
        }
        for attr in &md.attributes {
            if !attr.key.eq_ignore_ascii_case("fmtp") {
                continue;
            }
            let value = attr.value.as_deref().unwrap_or("");
            // fmtp value format: "<pt> <params>"
            let fmtp = match value.split_once(' ') {
                Some((_, fmtp)) => fmtp,
                None => continue,
            };

            if let Some(pair) = parse_h264_fmtp(fmtp) {
                params.push(pair);
            }
        }
    }

    Some(params)
}

/// Parses `profile-level-id` and `packetization-mode` from an fmtp string.
fn parse_h264_fmtp(fmtp: &str) -> Option<(String, String)> {
    let mut profile_level_id = None;
    let mut packetization_mode = None;

    for param in fmtp.split(';') {
        let param = param.trim();
        if let Some((key, val)) = param.split_once('=') {
            match key.trim().to_ascii_lowercase().as_str() {
                "profile-level-id" => {
                    profile_level_id = Some(val.trim().to_ascii_lowercase());
                }
                "packetization-mode" => {
                    packetization_mode = Some(val.trim().to_owned());
                }
                _ => {}
            }
        }
    }

    Some((profile_level_id?, packetization_mode?))
}
