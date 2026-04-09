use webrtc::{
    api::media_engine::{MIME_TYPE_H264, MIME_TYPE_OPUS, MIME_TYPE_VP8, MIME_TYPE_VP9},
    rtp_transceiver::{
        RTCPFeedback,
        rtp_codec::{RTCRtpCodecCapability, RTCRtpCodecParameters},
    },
};

use crate::AudioChannels;

pub fn vp8_codec_params() -> Vec<RTCRtpCodecParameters> {
    vec![RTCRtpCodecParameters {
        capability: RTCRtpCodecCapability {
            mime_type: MIME_TYPE_VP8.to_owned(),
            clock_rate: 90000,
            channels: 0,
            sdp_fmtp_line: "".to_owned(),
            rtcp_feedback: get_video_rtcp_feedback(),
        },
        payload_type: 96,
        ..Default::default()
    }]
}

pub fn vp9_codec_params() -> Vec<RTCRtpCodecParameters> {
    vec![RTCRtpCodecParameters {
        capability: RTCRtpCodecCapability {
            mime_type: MIME_TYPE_VP9.to_owned(),
            clock_rate: 90000,
            channels: 0,
            sdp_fmtp_line: "".to_owned(),
            rtcp_feedback: get_video_rtcp_feedback(),
        },
        payload_type: 98,
        ..Default::default()
    }]
}

pub fn h264_codec_params() -> Vec<RTCRtpCodecParameters> {
    // (payload_type, packetization_mode, profile_level)
    let codec_configs = [
        // constrained baseline, 3.1, included only for Twitch compatibility, SDP offer is rejected if it's missing.
        (102, 1, "42e01f"),
        (103, 0, "42e01f"),
        // constrained baseline, 5.1
        (104, 1, "42e033"),
        (105, 0, "42e033"),
        // main, 5.1
        (106, 1, "4d0033"),
        (107, 0, "4d0033"),
        // high, 5.1
        (108, 1, "640033"),
        (109, 0, "640033"),
    ];

    codec_configs
        .iter()
        .map(|(payload_type, pmode, plid)| RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: MIME_TYPE_H264.to_owned(),
                clock_rate: 90000,
                channels: 0,
                sdp_fmtp_line: format!(
                    "level-asymmetry-allowed=1;packetization-mode={pmode};profile-level-id={plid}"
                ),
                rtcp_feedback: get_video_rtcp_feedback(),
            },
            payload_type: *payload_type,
            ..Default::default()
        })
        .collect()
}

pub(crate) fn get_video_rtcp_feedback() -> Vec<RTCPFeedback> {
    vec![
        RTCPFeedback {
            typ: "goog-remb".to_owned(),
            parameter: "".to_owned(),
        },
        RTCPFeedback {
            typ: "ccm".to_owned(),
            parameter: "fir".to_owned(),
        },
        RTCPFeedback {
            typ: "nack".to_owned(),
            parameter: "".to_owned(),
        },
        RTCPFeedback {
            typ: "nack".to_owned(),
            parameter: "pli".to_owned(),
        },
    ]
}

pub fn opus_codec_params(fec_first: bool, channels: AudioChannels) -> Vec<RTCRtpCodecParameters> {
    let codec_configs = match fec_first {
        true => [
            ("minptime=10;useinbandfec=1", 111),
            ("minptime=10;useinbandfec=0", 110),
        ],
        false => [
            ("minptime=10;useinbandfec=0", 110),
            ("minptime=10;useinbandfec=1", 111),
        ],
    };

    let channels = match channels {
        AudioChannels::Mono => 1,
        AudioChannels::Stereo => 2,
    };

    codec_configs
        .iter()
        .map(|(fmtp, payload_type)| RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: MIME_TYPE_OPUS.to_owned(),
                clock_rate: 48000,
                channels,
                sdp_fmtp_line: fmtp.to_string(),
                rtcp_feedback: vec![],
            },
            payload_type: *payload_type,
            ..Default::default()
        })
        .collect()
}
