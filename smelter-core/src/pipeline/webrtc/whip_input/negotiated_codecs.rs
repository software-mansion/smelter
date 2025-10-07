use std::sync::Arc;
use webrtc::{
    api::media_engine::{MIME_TYPE_H264, MIME_TYPE_OPUS, MIME_TYPE_VP8, MIME_TYPE_VP9},
    rtp_transceiver::{PayloadType, RTCRtpTransceiver, rtp_codec::RTCRtpCodecParameters},
};

use crate::codecs::VideoDecoderOptions;

#[derive(Debug, Clone)]
pub(super) struct NegotiatedVideoCodecsInfo {
    pub h264: Option<VideoCodecInfo>,
    pub vp8: Option<VideoCodecInfo>,
    pub vp9: Option<VideoCodecInfo>,
}

#[derive(Debug, Clone)]
pub(super) struct NegotiatedAudioCodecsInfo {
    #[allow(dead_code)]
    pub opus: Option<AudioCodecInfo>,
}

#[derive(Debug, Clone)]
pub(super) struct VideoCodecInfo {
    pub payload_types: Vec<PayloadType>,
    pub preferred_decoder: VideoDecoderOptions,
}

#[derive(Debug, Clone)]
pub(super) struct AudioCodecInfo {
    #[allow(dead_code)]
    pub payload_types: Vec<PayloadType>,
}

impl NegotiatedAudioCodecsInfo {
    pub async fn new(audio_transceiver: Arc<RTCRtpTransceiver>) -> Option<Self> {
        let audio_receiver = audio_transceiver.receiver().await;
        let codecs = audio_receiver.get_parameters().await.codecs;

        let opus = Self::opus_info(&codecs);
        opus.map(|opus| Self { opus: Some(opus) })
    }

    fn opus_info(track_codecs: &[RTCRtpCodecParameters]) -> Option<AudioCodecInfo> {
        let payload_types: Vec<_> = track_codecs
            .iter()
            .filter(|codec| {
                codec.capability.mime_type.to_lowercase() == MIME_TYPE_OPUS.to_lowercase()
            })
            .map(|codec| codec.payload_type)
            .collect();

        match !payload_types.is_empty() {
            true => Some(AudioCodecInfo { payload_types }),
            false => None,
        }
    }
}

impl NegotiatedVideoCodecsInfo {
    pub async fn new(
        video_transceiver: Arc<RTCRtpTransceiver>,
        video_preferences: &[VideoDecoderOptions],
    ) -> Option<Self> {
        let video_receiver = video_transceiver.receiver().await;
        let codecs = video_receiver.get_parameters().await.codecs;

        let info = Self {
            h264: Self::h264_info(&codecs, video_preferences),
            vp8: Self::vp8_info(&codecs, video_preferences),
            vp9: Self::vp9_info(&codecs, video_preferences),
        };
        match info.h264.is_none() && info.vp8.is_none() && info.vp9.is_none() {
            false => Some(info),
            true => None,
        }
    }

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

        match !payload_types.is_empty() {
            true => Some(VideoCodecInfo {
                payload_types,
                preferred_decoder,
            }),
            false => None,
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

        match !payload_types.is_empty() {
            true => Some(VideoCodecInfo {
                payload_types,
                preferred_decoder,
            }),
            false => None,
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

        match !payload_types.is_empty() {
            true => Some(VideoCodecInfo {
                payload_types,
                preferred_decoder,
            }),
            false => None,
        }
    }

    pub fn is_payload_type_h264(&self, pt: PayloadType) -> bool {
        matches!(&self.h264, Some(info) if info.payload_types.contains(&pt))
    }

    pub fn is_payload_type_vp8(&self, pt: PayloadType) -> bool {
        matches!(&self.vp8, Some(info) if info.payload_types.contains(&pt))
    }

    pub fn is_payload_type_vp9(&self, pt: PayloadType) -> bool {
        matches!(&self.vp9, Some(info) if info.payload_types.contains(&pt))
    }
}
