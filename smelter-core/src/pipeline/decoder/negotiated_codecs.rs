use crate::codecs::VideoDecoderOptions;
use std::sync::Arc;
use webrtc::api::media_engine::{MIME_TYPE_H264, MIME_TYPE_OPUS, MIME_TYPE_VP8, MIME_TYPE_VP9};
use webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecParameters;

#[derive(Debug, Clone)]
pub struct NegotiatedVideoCodecsInfo {
    pub h264: Option<VideoCodecInfo>,
    pub vp8: Option<VideoCodecInfo>,
    pub vp9: Option<VideoCodecInfo>,
}

#[derive(Debug, Clone)]
pub struct NegotiatedAudioCodecsInfo {
    #[allow(dead_code)]
    pub opus: Option<AudioCodecInfo>,
}

impl NegotiatedAudioCodecsInfo {
    pub async fn from_webrtc_transceiver(
        audio_transceiver: Arc<webrtc::rtp_transceiver::RTCRtpTransceiver>,
    ) -> Option<Self> {
        let audio_receiver = audio_transceiver.receiver().await;
        let codecs = audio_receiver.get_parameters().await.codecs;

        fn opus_info(track_codecs: &[RTCRtpCodecParameters]) -> Option<AudioCodecInfo> {
            let payload_types: Vec<_> = track_codecs
                .iter()
                .filter(|codec| {
                    codec.capability.mime_type.to_lowercase() == MIME_TYPE_OPUS.to_lowercase()
                })
                .map(|codec| codec.payload_type)
                .collect();

            if !payload_types.is_empty() {
                Some(AudioCodecInfo { payload_types })
            } else {
                None
            }
        }

        let opus = opus_info(&codecs);
        opus.map(|opus| Self { opus: Some(opus) })
    }
}

#[derive(Debug, Clone)]
pub struct VideoCodecInfo {
    pub payload_types: Vec<u8>,
    pub preferred_decoder: VideoDecoderOptions,
}

#[derive(Debug, Clone)]
pub struct AudioCodecInfo {
    #[allow(dead_code)]
    pub payload_types: Vec<u8>,
}

impl NegotiatedVideoCodecsInfo {
    pub async fn from_webrtc_transceiver(
        video_transceiver: Arc<webrtc::rtp_transceiver::RTCRtpTransceiver>,
        video_preferences: &[VideoDecoderOptions],
    ) -> Option<Self> {
        let video_receiver = video_transceiver.receiver().await;
        let codecs = video_receiver.get_parameters().await.codecs;

        fn h264_info(
            track_codecs: &[RTCRtpCodecParameters],
            video_preferences: &[VideoDecoderOptions],
        ) -> Option<VideoCodecInfo> {
            const H264_OPTIONS: [VideoDecoderOptions; 2] = [
                VideoDecoderOptions::VulkanH264,
                VideoDecoderOptions::FfmpegH264,
            ];
            let preferred_decoder = *video_preferences
                .iter()
                .find(|option| H264_OPTIONS.contains(option))?;
            let payload_types: Vec<_> = track_codecs
                .iter()
                .filter(|codec| {
                    codec.capability.mime_type.to_lowercase() == MIME_TYPE_H264.to_lowercase()
                })
                .map(|codec| codec.payload_type)
                .collect();

            if !payload_types.is_empty() {
                Some(VideoCodecInfo {
                    payload_types,
                    preferred_decoder,
                })
            } else {
                None
            }
        }

        fn vp8_info(
            track_codecs: &[RTCRtpCodecParameters],
            video_preferences: &[VideoDecoderOptions],
        ) -> Option<VideoCodecInfo> {
            let preferred_decoder = *video_preferences
                .iter()
                .find(|option| &&VideoDecoderOptions::FfmpegVp8 == option)?;
            let payload_types: Vec<_> = track_codecs
                .iter()
                .filter(|codec| {
                    codec.capability.mime_type.to_lowercase() == MIME_TYPE_VP8.to_lowercase()
                })
                .map(|codec| codec.payload_type)
                .collect();

            if !payload_types.is_empty() {
                Some(VideoCodecInfo {
                    payload_types,
                    preferred_decoder,
                })
            } else {
                None
            }
        }

        fn vp9_info(
            track_codecs: &[RTCRtpCodecParameters],
            video_preferences: &[VideoDecoderOptions],
        ) -> Option<VideoCodecInfo> {
            let preferred_decoder = *video_preferences
                .iter()
                .find(|option| &&VideoDecoderOptions::FfmpegVp9 == option)?;
            let payload_types: Vec<_> = track_codecs
                .iter()
                .filter(|codec| {
                    codec.capability.mime_type.to_lowercase() == MIME_TYPE_VP9.to_lowercase()
                })
                .map(|codec| codec.payload_type)
                .collect();

            if !payload_types.is_empty() {
                Some(VideoCodecInfo {
                    payload_types,
                    preferred_decoder,
                })
            } else {
                None
            }
        }

        let info = Self {
            h264: h264_info(&codecs, video_preferences),
            vp8: vp8_info(&codecs, video_preferences),
            vp9: vp9_info(&codecs, video_preferences),
        };

        if info.has_any_codec() {
            Some(info)
        } else {
            None
        }
    }

    pub fn is_payload_type_h264(&self, pt: u8) -> bool {
        matches!(&self.h264, Some(info) if info.payload_types.contains(&pt))
    }

    pub fn is_payload_type_vp8(&self, pt: u8) -> bool {
        matches!(&self.vp8, Some(info) if info.payload_types.contains(&pt))
    }

    pub fn is_payload_type_vp9(&self, pt: u8) -> bool {
        matches!(&self.vp9, Some(info) if info.payload_types.contains(&pt))
    }

    pub fn has_any_codec(&self) -> bool {
        self.h264.is_some() || self.vp8.is_some() || self.vp9.is_some()
    }
}
