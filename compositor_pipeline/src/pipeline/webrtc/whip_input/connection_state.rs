use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use webrtc::{peer_connection::RTCPeerConnection, rtp_transceiver::rtp_codec::RTPCodecType};

use crate::pipeline::{decoder::VideoDecoderOptions, webrtc::whip_input::DecodedDataSender};

#[derive(Debug, Clone)]
pub(super) struct WhipInputConnectionStateOptions {
    pub bearer_token: String,
    pub video_preferences: Vec<VideoDecoderOptions>,
    pub decoded_data_sender: DecodedDataSender,
}

#[derive(Debug, Clone)]
pub(super) struct WhipInputConnectionState {
    pub bearer_token: String,
    pub peer_connection: Option<Arc<RTCPeerConnection>>,
    pub start_time_video: Option<Instant>,
    pub start_time_audio: Option<Instant>,
    pub video_preferences: Vec<VideoDecoderOptions>,
    pub decoded_data_sender: DecodedDataSender,
}

impl WhipInputConnectionState {
    pub fn new(options: WhipInputConnectionStateOptions) -> Self {
        WhipInputConnectionState {
            bearer_token: options.bearer_token,
            peer_connection: None,
            start_time_video: None,
            start_time_audio: None,
            video_preferences: options.video_preferences,
            decoded_data_sender: options.decoded_data_sender,
        }
    }

    pub fn elapsed_from_start_time(&mut self, track_kind: RTPCodecType) -> Option<Duration> {
        match track_kind {
            RTPCodecType::Video => {
                let start_time = self.start_time_video.get_or_insert_with(Instant::now);
                Some(start_time.elapsed())
            }
            RTPCodecType::Audio => {
                let start_time = self.start_time_audio.get_or_insert_with(Instant::now);
                Some(start_time.elapsed())
            }
            _ => None,
        }
    }
}
