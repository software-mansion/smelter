use std::sync::Arc;
use tracing::error;
use webrtc::{
    rtp_transceiver::{rtp_codec::RTCRtpCodecCapability, PayloadType, RTCRtpTransceiver},
    track::track_local::track_local_static_rtp::TrackLocalStaticRTP,
};

use crate::pipeline::encoder::{AudioEncoderOptions, VideoEncoderOptions};

pub trait MatchCodecCapability {
    fn matches(&self, capability: &RTCRtpCodecCapability) -> bool;
}

impl MatchCodecCapability for VideoEncoderOptions {
    fn matches(&self, capability: &RTCRtpCodecCapability) -> bool {
        match self {
            VideoEncoderOptions::H264(_) => capability.mime_type == "video/H264",
            VideoEncoderOptions::VP8(_) => capability.mime_type == "video/VP8",
            VideoEncoderOptions::VP9(_) => capability.mime_type == "video/VP9",
        }
    }
}

impl MatchCodecCapability for AudioEncoderOptions {
    fn matches(&self, capability: &RTCRtpCodecCapability) -> bool {
        match self {
            AudioEncoderOptions::Opus(_) => capability.mime_type == "audio/opus",
            AudioEncoderOptions::Aac(_) => false,
        }
    }
}

pub async fn setup_track<T: MatchCodecCapability + Clone>(
    transceiver: Option<Arc<RTCRtpTransceiver>>,
    encoder_preferences: Option<Vec<T>>,
    track_kind: String,
) -> (
    Option<Arc<TrackLocalStaticRTP>>,
    Option<PayloadType>,
    Option<T>,
) {
    let (Some(transceiver), Some(encoder_preferences)) = (transceiver, encoder_preferences) else {
        return (None, None, None);
    };

    let sender = transceiver.sender().await;
    let params = sender.get_parameters().await;
    let supported_codecs = &params.rtp_parameters.codecs;

    for encoder_options in &encoder_preferences {
        if let Some(codec_parameters) = supported_codecs
            .iter()
            .find(|codec_params| encoder_options.matches(&codec_params.capability))
        {
            let track = Arc::new(TrackLocalStaticRTP::new(
                codec_parameters.capability.clone(),
                track_kind.clone(),
                "webrtc-rs".to_string(),
            ));

            if sender.replace_track(Some(track.clone())).await.is_err() {
                error!("Failed to replace {} track", track_kind);
                return (None, None, None);
            }

            return (
                Some(track),
                Some(codec_parameters.payload_type),
                Some(encoder_options.clone()),
            );
        }
    }

    (None, None, None)
}
