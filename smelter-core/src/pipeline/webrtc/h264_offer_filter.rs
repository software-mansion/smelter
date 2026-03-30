use std::collections::HashSet;

use tracing::warn;
use webrtc::{
    api::media_engine::MIME_TYPE_H264,
    peer_connection::sdp::session_description::RTCSessionDescription,
    rtp_transceiver::rtp_codec::RTCRtpCodecParameters,
};

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

/// Mirrors H264 variants from the SDP offer.
///
/// For each offered H264 payload type, we emit a local codec entry with the same
/// payload type and negotiated fmtp constraints, so WHIP answers can include every
/// offered H264 profile/level and WHEP can expose as many H264 subtypes as possible.
pub(crate) fn filter_h264_codecs_by_offer(
    offer: &RTCSessionDescription,
    codecs: Vec<RTCRtpCodecParameters>,
) -> Vec<RTCRtpCodecParameters> {
    let offer_h264_params = match extract_h264_params_from_offer(offer) {
        Some(params) if !params.is_empty() => params,
        Some(_) => {
            // Offer has no explicit H264 fmtp lines.
            return codecs;
        }
        None => {
            warn!("Failed to parse SDP offer for H264 codec filtering, using all codecs");
            return codecs;
        }
    };

    let (h264_templates, mut passthrough): (Vec<_>, Vec<_>) = codecs
        .into_iter()
        .partition(|codec| codec.capability.mime_type == MIME_TYPE_H264);

    if h264_templates.is_empty() {
        return passthrough;
    }

    // Any H264 template works: payload type and fmtp are overwritten from the offer.
    let Some(template) = h264_templates.into_iter().next() else {
        return passthrough;
    };

    let mut seen = HashSet::new();
    for offer in offer_h264_params {
        if !seen.insert((
            offer.payload_type,
            offer.packetization_mode.clone(),
            offer.profile_level_id.clone(),
        )) {
            continue;
        }

        let mut codec = template.clone();

        codec.payload_type = offer.payload_type;
        codec.capability.sdp_fmtp_line = build_h264_fmtp(
            offer.profile_level_id.as_deref(),
            offer.packetization_mode.as_str(),
        );
        passthrough.push(codec);
    }

    passthrough
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
