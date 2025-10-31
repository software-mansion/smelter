use std::sync::Arc;
use webrtc::api::media_engine::{MIME_TYPE_H264, MIME_TYPE_OPUS, MIME_TYPE_VP8, MIME_TYPE_VP9};
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::rtp_transceiver::{rtp_codec::RTCRtpCodecCapability, rtp_sender::RTCRtpSender};
use webrtc::track::track_local::track_local_static_rtp::TrackLocalStaticRTP;

pub async fn replace_tracks_with_negotiated_codec(
    answer: &RTCSessionDescription,
    video_sender: &Arc<RTCRtpSender>,
    audio_sender: &Arc<RTCRtpSender>,
) -> Result<(), webrtc::Error> {
    let (video_mime_type, audio_mime_type) = extract_negotiated_codec(answer)?;

    if let Some(mime_type) = video_mime_type {
        let track = Arc::new(TrackLocalStaticRTP::new(
            RTCRtpCodecCapability {
                mime_type,
                ..Default::default()
            },
            "video".to_string(),
            "webrtc-rs".to_string(),
        ));
        video_sender.replace_track(Some(track)).await?;
    }

    if let Some(mime_type) = audio_mime_type {
        let track = Arc::new(TrackLocalStaticRTP::new(
            RTCRtpCodecCapability {
                mime_type,
                ..Default::default()
            },
            "audio".to_string(),
            "webrtc-rs".to_string(),
        ));
        audio_sender.replace_track(Some(track)).await?;
    }

    Ok(())
}

fn extract_negotiated_codec(
    answer: &RTCSessionDescription,
) -> Result<(Option<String>, Option<String>), webrtc::Error> {
    let session_description = answer.unmarshal()?;
    let mut video_mime_type: Option<String> = None;
    let mut audio_mime_type: Option<String> = None;

    for md in &session_description.media_descriptions {
        let media_kind = md.media_name.media.to_ascii_lowercase();

        for attr in &md.attributes {
            if attr.key.eq_ignore_ascii_case("rtpmap") {
                // a=rtpmap:<pt> <codec>/<clockrate>[/<channels>]
                let value = attr.value.as_deref().unwrap_or("");
                let mut parts = value.split_whitespace();
                let _payload_type = parts.next();
                let spec = parts.next().unwrap_or("");
                let codec_name = spec.split('/').next().unwrap_or("").to_ascii_uppercase();

                let mime_type = match (media_kind.as_str(), codec_name.as_str()) {
                    ("video", "H264") => Some(MIME_TYPE_H264),
                    ("video", "VP8") => Some(MIME_TYPE_VP8),
                    ("video", "VP9") => Some(MIME_TYPE_VP9),
                    ("audio", "OPUS") => Some(MIME_TYPE_OPUS),
                    _ => None,
                };

                if let Some(mime_type) = mime_type {
                    if media_kind == "video" && video_mime_type.is_none() {
                        video_mime_type = Some(mime_type.to_string());
                    } else if media_kind == "audio" && audio_mime_type.is_none() {
                        audio_mime_type = Some(mime_type.to_string());
                    }

                    if video_mime_type.is_some() && audio_mime_type.is_some() {
                        return Ok((video_mime_type, audio_mime_type));
                    }
                }
            }
        }
    }

    Ok((video_mime_type, audio_mime_type))
}
