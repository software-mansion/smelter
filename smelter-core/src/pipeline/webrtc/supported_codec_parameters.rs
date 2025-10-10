use webrtc::{
    api::media_engine::{MIME_TYPE_H264, MIME_TYPE_OPUS, MIME_TYPE_VP8, MIME_TYPE_VP9},
    rtp_transceiver::{
        RTCPFeedback,
        rtp_codec::{RTCRtpCodecCapability, RTCRtpCodecParameters},
    },
};

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

pub fn vp8_codec_params_default_payload_type() -> Vec<RTCRtpCodecParameters> {
    vec![RTCRtpCodecParameters {
        capability: RTCRtpCodecCapability {
            mime_type: MIME_TYPE_VP8.to_owned(),
            clock_rate: 90000,
            channels: 0,
            sdp_fmtp_line: "".to_owned(),
            rtcp_feedback: get_video_rtcp_feedback(),
        },
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

pub fn vp9_codec_params_default_payload_type() -> Vec<RTCRtpCodecParameters> {
    vec![RTCRtpCodecParameters {
        capability: RTCRtpCodecCapability {
            mime_type: MIME_TYPE_VP9.to_owned(),
            clock_rate: 90000,
            channels: 0,
            sdp_fmtp_line: "".to_owned(),
            rtcp_feedback: get_video_rtcp_feedback(),
        },
        ..Default::default()
    }]
}

pub fn h264_codec_params() -> Vec<RTCRtpCodecParameters> {
    let codec_configs = [
        (
            "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42001f",
            102,
        ),
        (
            "level-asymmetry-allowed=1;packetization-mode=0;profile-level-id=42001f",
            127,
        ),
        (
            "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42e01f",
            125,
        ),
        (
            "level-asymmetry-allowed=1;packetization-mode=0;profile-level-id=42e01f",
            108,
        ),
        (
            "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=640032",
            123,
        ),
    ];

    codec_configs
        .iter()
        .map(|(fmtp, payload_type)| RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: MIME_TYPE_H264.to_owned(),
                clock_rate: 90000,
                channels: 0,
                sdp_fmtp_line: fmtp.to_string(),
                rtcp_feedback: get_video_rtcp_feedback(),
            },
            payload_type: *payload_type,
            ..Default::default()
        })
        .collect()
}

pub fn h264_codec_params_default_payload_type() -> Vec<RTCRtpCodecParameters> {
    let codec_configs = [
        "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42001f",
        "level-asymmetry-allowed=1;packetization-mode=0;profile-level-id=42001f",
        "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42e01f",
        "level-asymmetry-allowed=1;packetization-mode=0;profile-level-id=42e01f",
        "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=640032",
    ];

    codec_configs
        .iter()
        .map(|fmtp| RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: MIME_TYPE_H264.to_owned(),
                clock_rate: 90000,
                channels: 0,
                sdp_fmtp_line: fmtp.to_string(),
                rtcp_feedback: get_video_rtcp_feedback(),
            },
            ..Default::default()
        })
        .collect()
}

fn get_video_rtcp_feedback() -> Vec<RTCPFeedback> {
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

pub fn opus_codec_params() -> Vec<RTCRtpCodecParameters> {
    let codec_configs = [
        ("minptime=10;useinbandfec=1", 111, 2),
        ("minptime=10;useinbandfec=1", 112, 1),
        ("minptime=10;useinbandfec=0", 109, 2),
        ("minptime=10;useinbandfec=0", 110, 1),
    ];

    codec_configs
        .iter()
        .map(|(fmtp, payload_type, channels)| RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: MIME_TYPE_OPUS.to_owned(),
                clock_rate: 48000,
                channels: *channels,
                sdp_fmtp_line: fmtp.to_string(),
                rtcp_feedback: vec![],
            },
            payload_type: *payload_type,
            ..Default::default()
        })
        .collect()
}
