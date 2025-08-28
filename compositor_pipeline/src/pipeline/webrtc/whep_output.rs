use std::sync::Arc;
use tokio::sync::broadcast;

use crate::{
    pipeline::{
        encoder::{
            ffmpeg_h264::FfmpegH264Encoder, ffmpeg_vp8::FfmpegVp8Encoder,
            ffmpeg_vp9::FfmpegVp9Encoder, libopus::OpusEncoder,
        },
        output::{Output, OutputAudio, OutputVideo},
        webrtc::whep_output::{
            connection_state::{
                WhepAudioConnectionOptions, WhepOutputConnectionStateOptions,
                WhepVideoConnectionOptions,
            },
            track_task_audio::{
                WhepAudioTrackThread, WhepAudioTrackThreadHandle, WhepAudioTrackThreadOptions,
            },
            track_task_video::{
                WhepVideoTrackThread, WhepVideoTrackThreadHandle, WhepVideoTrackThreadOptions,
            },
        },
    },
    thread_utils::InitializableThread,
};

use crate::prelude::*;

pub(crate) mod cleanup_session_handler;
pub(super) mod connection_state;
pub(super) mod init_payloaders;
pub(super) mod peer_connection;
pub(super) mod state;
pub(super) mod stream_media_to_peer;
pub(super) mod track_task_audio;
pub(super) mod track_task_video;

#[derive(Debug)]
pub struct WhepOutput {
    video: Option<WhepVideoTrackThreadHandle>,
    audio: Option<WhepAudioTrackThreadHandle>,
}

impl WhepOutput {
    pub fn new(
        ctx: Arc<PipelineCtx>,
        output_id: OutputId,
        options: WhepSenderOptions,
    ) -> Result<Self, OutputInitError> {
        let state_clone = ctx.whip_whep_state.clone();
        let Some(state) = state_clone else {
            return Err(OutputInitError::WhipWhepServerNotRunning);
        };
        let bearer_token = options.bearer_token.clone();

        let video_options = options
            .video
            .as_ref()
            .map(|video| Self::init_video_thread(&ctx, &output_id, video.clone()))
            .transpose()?;

        let audio_options = options
            .audio
            .as_ref()
            .map(|audio| Self::init_audio_thread(&ctx, &output_id, audio.clone()))
            .transpose()?;

        state.outputs.add_output(
            &output_id,
            WhepOutputConnectionStateOptions {
                bearer_token: bearer_token.clone(),
                video_options: video_options.clone(),
                audio_options: audio_options.clone(),
            },
        );

        Ok(Self {
            audio: audio_options.map(|a| a.track_thread_handle),
            video: video_options.map(|v| v.track_thread_handle),
        })
    }

    fn init_video_thread(
        ctx: &Arc<PipelineCtx>,
        output_id: &OutputId,
        options: VideoEncoderOptions,
    ) -> Result<WhepVideoConnectionOptions, OutputInitError> {
        let (sender, receiver) = broadcast::channel(1000);
        let thread_handle = match &options {
            VideoEncoderOptions::FfmpegH264(options) => {
                WhepVideoTrackThread::<FfmpegH264Encoder>::spawn(
                    output_id.clone(),
                    WhepVideoTrackThreadOptions {
                        ctx: ctx.clone(),
                        encoder_options: options.clone(),
                        chunks_sender: sender,
                    },
                )?
            }
            VideoEncoderOptions::FfmpegVp8(options) => {
                WhepVideoTrackThread::<FfmpegVp8Encoder>::spawn(
                    output_id.clone(),
                    WhepVideoTrackThreadOptions {
                        ctx: ctx.clone(),
                        encoder_options: options.clone(),
                        chunks_sender: sender,
                    },
                )?
            }
            VideoEncoderOptions::FfmpegVp9(options) => {
                WhepVideoTrackThread::<FfmpegVp9Encoder>::spawn(
                    output_id.clone(),
                    WhepVideoTrackThreadOptions {
                        ctx: ctx.clone(),
                        encoder_options: options.clone(),
                        chunks_sender: sender,
                    },
                )?
            }
        };

        Ok(WhepVideoConnectionOptions {
            encoder: options,
            receiver: receiver.into(),
            track_thread_handle: thread_handle,
        })
    }

    fn init_audio_thread(
        ctx: &Arc<PipelineCtx>,
        output_id: &OutputId,
        options: AudioEncoderOptions,
    ) -> Result<WhepAudioConnectionOptions, OutputInitError> {
        let (sender, receiver) = broadcast::channel(1000);
        let thread_handle = match options.clone() {
            AudioEncoderOptions::Opus(options) => WhepAudioTrackThread::<OpusEncoder>::spawn(
                output_id.clone(),
                WhepAudioTrackThreadOptions {
                    ctx: ctx.clone(),
                    encoder_options: options.clone(),
                    chunks_sender: sender,
                },
            )?,
            AudioEncoderOptions::FdkAac(_options) => {
                return Err(OutputInitError::UnsupportedAudioCodec(AudioCodec::Aac))
            }
        };

        Ok(WhepAudioConnectionOptions {
            encoder: options,
            receiver: receiver.into(),
            track_thread_handle: thread_handle,
        })
    }
}

impl Output for WhepOutput {
    fn audio(&self) -> Option<OutputAudio<'_>> {
        self.audio.as_ref().map(|audio| OutputAudio {
            samples_batch_sender: &audio.sample_batch_sender,
        })
    }

    fn video(&self) -> Option<OutputVideo<'_>> {
        self.video.as_ref().map(|video| OutputVideo {
            resolution: video.config.resolution,
            frame_format: video.config.output_format,
            frame_sender: &video.frame_sender,
            keyframe_request_sender: &video.keyframe_request_sender,
        })
    }

    fn kind(&self) -> OutputProtocolKind {
        OutputProtocolKind::Whep
    }
}
