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
    let profile_level_ids = [
        "42001f", // baseline, 3.1
        "42e01f", // constrained baseline, 3.1
        "42002a", // baseline, 4.2
        "4d001f", // main, 3.1
        "4d0028", // main, 4.0
        "640028", // high, 4.0
        "640029", // high, 4.1
        "64002a", // high, 4.2
        "640032", // high, 5.0
        "640033", // high, 5.1
    ];

    let opus_payload_types: [u8; 2] = [110, 111];
    let payload_types = (100u8..).filter(|pt| !opus_payload_types.contains(pt));

    profile_level_ids
        .iter()
        .flat_map(|plid| [1, 0].map(|pmode| (plid, pmode)))
        .zip(payload_types)
        .map(|((plid, pmode), payload_type)| RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: MIME_TYPE_H264.to_owned(),
                clock_rate: 90000,
                channels: 0,
                sdp_fmtp_line: format!(
                    "level-asymmetry-allowed=1;packetization-mode={pmode};profile-level-id={plid}"
                ),
                rtcp_feedback: get_video_rtcp_feedback(),
            },
            payload_type,
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
