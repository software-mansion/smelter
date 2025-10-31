use std::collections::HashMap;
use webrtc::api::media_engine::{MIME_TYPE_H264, MIME_TYPE_OPUS, MIME_TYPE_VP8, MIME_TYPE_VP9};
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::rtp_transceiver::{RTCPFeedback, rtp_codec::RTCRtpCodecCapability};

#[derive(Debug, Clone)]
pub struct NegotiatedCodec {
    pub capability: RTCRtpCodecCapability,
    pub payload_type: u8,
}

#[derive(Debug, Clone, Default)]
pub struct NegotiatedMediaCodecs {
    pub video: Vec<NegotiatedCodec>,
    pub audio: Vec<NegotiatedCodec>,
}
pub fn extract_negotiated_codecs(
    answer: &RTCSessionDescription,
) -> Result<NegotiatedMediaCodecs, webrtc::Error> {
    let session_description = answer.unmarshal()?;
    let mut codecs = NegotiatedMediaCodecs::default();

    for md in &session_description.media_descriptions {
        let media_kind = md.media_name.media.to_ascii_lowercase();

        let mut rtpmap: HashMap<u8, (String, u32, u16)> = HashMap::new();
        let mut fmtp: HashMap<u8, String> = HashMap::new();
        let mut rtcp_fb_by_pt: HashMap<u8, Vec<RTCPFeedback>> = HashMap::new();
        let mut rtcp_fb_all: Vec<RTCPFeedback> = Vec::new();

        for attr in &md.attributes {
            let key = attr.key.to_ascii_lowercase();
            let value = attr.value.as_deref().unwrap_or("");

            if key == "rtpmap" {
                // <payload_type> <codec>/<clock_rate>[/<channels>]
                let mut parts = value.splitn(2, ' ');
                let payload_type_str = parts.next().unwrap_or("");
                let spec = parts.next().unwrap_or("");
                if let Ok(payload_type) = payload_type_str.parse::<u8>() {
                    let mut spec_parts = spec.split('/');
                    let codec = spec_parts.next().unwrap_or("").to_string();
                    let clock_rate = spec_parts
                        .next()
                        .and_then(|s| s.parse::<u32>().ok())
                        .unwrap_or_default();
                    let channels = spec_parts
                        .next()
                        .and_then(|s| s.parse::<u16>().ok())
                        .unwrap_or_default();
                    rtpmap.insert(payload_type, (codec, clock_rate, channels));
                }
            } else if key == "fmtp" {
                // <payload_type> <params>
                let mut parts = value.splitn(2, ' ');
                let payload_type_str = parts.next().unwrap_or("");
                let params = parts.next().unwrap_or("").trim().to_string();
                if let Ok(payload_type) = payload_type_str.parse::<u8>() {
                    fmtp.insert(payload_type, params);
                }
            } else if key == "rtcp-fb" {
                // a=rtcp-fb:<payload type|*> <type> [<parameter>]
                let mut parts = value.splitn(2, ' ');
                let payload_type_str = parts.next().unwrap_or("");
                let spec = parts.next().unwrap_or("").trim();

                let mut spec_iter = spec.split_whitespace();
                let typ = spec_iter.next().unwrap_or("").to_string();
                let parameter = spec_iter.next().unwrap_or("").to_string();
                let fb = RTCPFeedback { typ, parameter };

                // if payload_type="*" rtcp-fb applies to all peayload_types in this media session
                if payload_type_str == "*" {
                    rtcp_fb_all.push(fb);
                } else if let Ok(payload_type) = payload_type_str.parse::<u8>() {
                    rtcp_fb_by_pt.entry(payload_type).or_default().push(fb);
                }
            }
        }

        for (payload_type, (codec_name, clock_rate, channels)) in rtpmap {
            let mime = match (
                media_kind.as_str(),
                codec_name.to_ascii_uppercase().as_str(),
            ) {
                ("video", "H264") => Some(MIME_TYPE_H264),
                ("video", "VP8") => Some(MIME_TYPE_VP8),
                ("video", "VP9") => Some(MIME_TYPE_VP9),
                ("audio", "OPUS") => Some(MIME_TYPE_OPUS),
                _ => None,
            };
            if let Some(mime) = mime {
                let mut rtcp_feedback: Vec<RTCPFeedback> = Vec::new();
                rtcp_feedback.extend(rtcp_fb_all.iter().cloned());
                if let Some(mut per_pt_fb) = rtcp_fb_by_pt.remove(&payload_type) {
                    rtcp_feedback.append(&mut per_pt_fb);
                }

                let capability = RTCRtpCodecCapability {
                    mime_type: mime.to_string(),
                    clock_rate,
                    channels,
                    sdp_fmtp_line: fmtp.remove(&payload_type).unwrap_or_default(),
                    rtcp_feedback,
                };
                let negotiated = NegotiatedCodec {
                    capability,
                    payload_type,
                };
                if media_kind == "video" {
                    codecs.video.push(negotiated);
                } else if media_kind == "audio" {
                    codecs.audio.push(negotiated);
                }
            }
        }
    }

    Ok(codecs)
}
