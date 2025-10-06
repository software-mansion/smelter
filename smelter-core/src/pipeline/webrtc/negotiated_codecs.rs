use std::sync::Arc;

use webrtc::{
    api::media_engine::{MIME_TYPE_H264, MIME_TYPE_OPUS, MIME_TYPE_VP8, MIME_TYPE_VP9},
    rtp_transceiver::{rtp_codec::RTCRtpCodecParameters, RTCRtpTransceiver},
};

use crate::{
    codecs::VideoDecoderOptions,
    pipeline::decoder::negotiated_codecs::{
        AudioCodecInfo, NegotiatedAudioCodecsInfo, NegotiatedVideoCodecsInfo, VideoCodecInfo,
    },
};

pub trait WebrtcNegotiatedVideoCodecsInfo: Sized {
    async fn from_webrtc_transceiver(
        transceiver: Arc<RTCRtpTransceiver>,
        preferences: &[VideoDecoderOptions],
    ) -> Option<Self>;
}

pub trait WebrtcNegotiatedAudioCodecsInfo: Sized {
    async fn from_webrtc_transceiver(transceiver: Arc<RTCRtpTransceiver>) -> Option<Self>;
}

impl WebrtcNegotiatedVideoCodecsInfo for NegotiatedVideoCodecsInfo {
    async fn from_webrtc_transceiver(
        video_transceiver: Arc<RTCRtpTransceiver>,
        video_preferences: &[VideoDecoderOptions],
    ) -> Option<Self> {
        let video_receiver = video_transceiver.receiver().await;
        let codecs = video_receiver.get_parameters().await.codecs;

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
}

impl WebrtcNegotiatedAudioCodecsInfo for NegotiatedAudioCodecsInfo {
    async fn from_webrtc_transceiver(audio_transceiver: Arc<RTCRtpTransceiver>) -> Option<Self> {
        let audio_receiver = audio_transceiver.receiver().await;
        let codecs = audio_receiver.get_parameters().await.codecs;

        let opus = opus_info(&codecs);
        opus.map(|opus| Self { opus: Some(opus) })
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
        .filter(|codec| codec.capability.mime_type.to_lowercase() == MIME_TYPE_H264.to_lowercase())
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
        .filter(|codec| codec.capability.mime_type.to_lowercase() == MIME_TYPE_VP8.to_lowercase())
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
        .filter(|codec| codec.capability.mime_type.to_lowercase() == MIME_TYPE_VP9.to_lowercase())
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

fn opus_info(track_codecs: &[RTCRtpCodecParameters]) -> Option<AudioCodecInfo> {
    let payload_types: Vec<_> = track_codecs
        .iter()
        .filter(|codec| codec.capability.mime_type.to_lowercase() == MIME_TYPE_OPUS.to_lowercase())
        .map(|codec| codec.payload_type)
        .collect();

    if !payload_types.is_empty() {
        Some(AudioCodecInfo { payload_types })
    } else {
        None
    }
}
