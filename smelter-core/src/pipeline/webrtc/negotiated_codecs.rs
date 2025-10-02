use std::sync::Arc;

use webrtc::{
    api::media_engine::{MIME_TYPE_H264, MIME_TYPE_OPUS, MIME_TYPE_VP8, MIME_TYPE_VP9},
    rtp_transceiver::{
        PayloadType, RTCRtpTransceiver, rtp_codec::RTCRtpCodecParameters,
        rtp_receiver::RTCRtpReceiver,
    },
};

use crate::{
    codecs::VideoDecoderOptions,
    pipeline::{decoder::VideoDecoderMapping, rtp::depayloader::VideoPayloadTypeMapping},
};

pub(super) struct VideoCodecMappings {
    pub(super) decoder_mapping: VideoDecoderMapping,
    pub(super) payload_type_mapping: VideoPayloadTypeMapping,
}

pub trait WebrtcVideoDecoderMapping: Sized {
    async fn from_webrtc_transceiver(
        transceiver: Arc<RTCRtpTransceiver>,
        preferences: &[VideoDecoderOptions],
    ) -> Option<Self>;
}

impl WebrtcVideoDecoderMapping for VideoDecoderMapping {
    async fn from_webrtc_transceiver(
        video_transceiver: Arc<RTCRtpTransceiver>,
        video_preferences: &[VideoDecoderOptions],
    ) -> Option<Self> {
        let video_receiver = video_transceiver.receiver().await;
        let codecs = video_receiver.get_parameters().await.codecs;

        let info = Self {
            h264: h264_decoder_info(&codecs, video_preferences),
            vp8: vp8_decoder_info(&codecs, video_preferences),
            vp9: vp9_decoder_info(&codecs, video_preferences),
        };

        info.has_any_codec().then_some(info)
    }
}

fn h264_decoder_info(
    track_codecs: &[RTCRtpCodecParameters],
    video_preferences: &[VideoDecoderOptions],
) -> Option<VideoDecoderOptions> {
    const H264_OPTIONS: [VideoDecoderOptions; 2] = [
        VideoDecoderOptions::VulkanH264,
        VideoDecoderOptions::FfmpegH264,
    ];
    let preferred_decoder = *video_preferences
        .iter()
        .find(|option| H264_OPTIONS.contains(option))?;
    let h264_negotiated = track_codecs
        .iter()
        .any(|codec| codec.capability.mime_type.to_lowercase() == MIME_TYPE_H264.to_lowercase());

    h264_negotiated.then_some(preferred_decoder)
}

fn vp8_decoder_info(
    track_codecs: &[RTCRtpCodecParameters],
    video_preferences: &[VideoDecoderOptions],
) -> Option<VideoDecoderOptions> {
    let preferred_decoder = *video_preferences
        .iter()
        .find(|option| &&VideoDecoderOptions::FfmpegVp8 == option)?;
    let vp8_negotiated = track_codecs
        .iter()
        .any(|codec| codec.capability.mime_type.to_lowercase() == MIME_TYPE_VP8.to_lowercase());

    vp8_negotiated.then_some(preferred_decoder)
}

fn vp9_decoder_info(
    track_codecs: &[RTCRtpCodecParameters],
    video_preferences: &[VideoDecoderOptions],
) -> Option<VideoDecoderOptions> {
    let preferred_decoder = *video_preferences
        .iter()
        .find(|option| &&VideoDecoderOptions::FfmpegVp9 == option)?;
    let vp9_negotiated = track_codecs
        .iter()
        .any(|codec| codec.capability.mime_type.to_lowercase() == MIME_TYPE_VP9.to_lowercase());

    vp9_negotiated.then_some(preferred_decoder)
}

pub trait WebrtcVideoPayloadTypeMapping: Sized {
    async fn from_webrtc_transceiver(transceiver: Arc<RTCRtpTransceiver>) -> Option<Self>;
}

impl WebrtcVideoPayloadTypeMapping for VideoPayloadTypeMapping {
    async fn from_webrtc_transceiver(video_transceiver: Arc<RTCRtpTransceiver>) -> Option<Self> {
        let video_receiver = video_transceiver.receiver().await;
        let codecs = video_receiver.get_parameters().await.codecs;

        let info = Self {
            h264: h264_payload_type_info(&codecs),
            vp8: vp8_payload_type_info(&codecs),
            vp9: vp9_payload_type_info(&codecs),
        };

        info.has_any_codec().then_some(info)
    }
}

fn h264_payload_type_info(track_codecs: &[RTCRtpCodecParameters]) -> Option<Vec<PayloadType>> {
    let payload_types: Vec<PayloadType> = track_codecs
        .iter()
        .filter(|codec| codec.capability.mime_type.to_lowercase() == MIME_TYPE_H264.to_lowercase())
        .map(|codec| codec.payload_type)
        .collect();

    (!payload_types.is_empty()).then_some(payload_types)
}

fn vp8_payload_type_info(track_codecs: &[RTCRtpCodecParameters]) -> Option<Vec<PayloadType>> {
    let payload_types: Vec<PayloadType> = track_codecs
        .iter()
        .filter(|codec| codec.capability.mime_type.to_lowercase() == MIME_TYPE_VP8.to_lowercase())
        .map(|codec| codec.payload_type)
        .collect();

    (!payload_types.is_empty()).then_some(payload_types)
}

fn vp9_payload_type_info(track_codecs: &[RTCRtpCodecParameters]) -> Option<Vec<PayloadType>> {
    let payload_types: Vec<PayloadType> = track_codecs
        .iter()
        .filter(|codec| codec.capability.mime_type.to_lowercase() == MIME_TYPE_VP9.to_lowercase())
        .map(|codec| codec.payload_type)
        .collect();

    (!payload_types.is_empty()).then_some(payload_types)
}

pub async fn audio_codec_negotiated(receiver: Arc<RTCRtpReceiver>) -> bool {
    let track_codecs = receiver.get_parameters().await.codecs;
    track_codecs
        .iter()
        .any(|codec| codec.capability.mime_type.to_lowercase() == MIME_TYPE_OPUS.to_lowercase())
}
