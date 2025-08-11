use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast;

use crate::pipeline::{
    rtp::RtpPacket,
    webrtc::whep_output::{
        peer_connection::PeerConnection, track_task_audio::WhepAudioTrackThreadHandle,
        track_task_video::WhepVideoTrackThreadHandle,
    },
};
use crate::prelude::*;

#[derive(Debug, Clone)]
pub(crate) struct WhepOutputConnectionStateOptions {
    pub bearer_token: Option<Arc<str>>,
    pub video_options: Option<WhepVideoConnectionOptions>,
    pub audio_options: Option<WhepAudioConnectionOptions>,
}

#[derive(Debug, Clone)]
pub(crate) struct WhepOutputConnectionState {
    pub bearer_token: Option<Arc<str>>,
    pub sessions: HashMap<Arc<str>, Arc<PeerConnection>>,
    pub video_options: Option<WhepVideoConnectionOptions>,
    pub audio_options: Option<WhepAudioConnectionOptions>,
}

#[derive(Debug, Clone)]
pub(crate) struct WhepVideoConnectionOptions {
    pub encoder: VideoEncoderOptions,
    pub receiver: Arc<broadcast::Receiver<RtpPacket>>,
    pub track_thread_handle: WhepVideoTrackThreadHandle,
}

#[derive(Debug, Clone)]
pub(crate) struct WhepAudioConnectionOptions {
    pub encoder: AudioEncoderOptions,
    pub receiver: Arc<broadcast::Receiver<RtpPacket>>,
    pub track_thread_handle: WhepAudioTrackThreadHandle,
}

impl WhepOutputConnectionState {
    pub fn new(options: WhepOutputConnectionStateOptions) -> Self {
        WhepOutputConnectionState {
            bearer_token: options.bearer_token,
            sessions: HashMap::new(),
            video_options: options.video_options,
            audio_options: options.audio_options,
        }
    }
}
