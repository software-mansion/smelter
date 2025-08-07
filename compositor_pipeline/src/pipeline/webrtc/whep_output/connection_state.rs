use std::sync::Arc;
use tokio::sync::broadcast;

use crate::pipeline::rtp::RtpPacket;
use crate::pipeline::webrtc::whep_output::track_task_audio::WhepAudioTrackThreadHandle;
use crate::pipeline::webrtc::whep_output::track_task_video::WhepVideoTrackThreadHandle;
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
            video_options: options.video_options,
            audio_options: options.audio_options,
        }
    }
}
