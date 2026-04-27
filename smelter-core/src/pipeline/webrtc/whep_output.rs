use std::sync::Arc;
use tokio::sync::broadcast;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;

use crate::{
    pipeline::{
        encoder::{
            ffmpeg_h264::FfmpegH264Encoder, ffmpeg_vp8::FfmpegVp8Encoder,
            ffmpeg_vp9::FfmpegVp9Encoder, libopus::OpusEncoder, vulkan_h264::VulkanH264Encoder,
        },
        output::{Output, OutputAudio, OutputVideo},
        webrtc::whep_output::{
            state::{
                WhepAudioConnectionOptions, WhepOutputConnectionStateOptions, WhepOutputsState,
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
    utils::{InitializableThread, ThreadJoiner},
};

use crate::prelude::*;

pub(super) mod init_payloaders;
pub(crate) mod pc_state_change;
pub(super) mod peer_connection;
pub(super) mod state;
pub(super) mod stream_media_to_peer;
pub(super) mod track_task_audio;
pub(super) mod track_task_video;

/// WHEP output - serves media to a remote WHEP client.
///
/// ## Codec negotiation
///
/// Remote client sends SDP offer. We echo all offered codec variants in our
/// answer to maximize negotiation success. The actual encoding is driven by our
/// encoder config, not the negotiated profile/level. Tracks where negotiation
/// fails are cleaned up (allowing audio-only or video-only streams).
#[derive(Debug)]
pub struct WhepOutput {
    video: Option<WhepVideoTrackThreadHandle>,
    audio: Option<WhepAudioTrackThreadHandle>,
    output_ref: Ref<OutputId>,
    outputs_state: WhepOutputsState,
    // Drop order matters: `outputs_state.remove_output` runs first in Drop;
    // then `video`/`audio` Handle fields drop (closing the encoder senders);
    // then these joiners wait. This guarantees encoder threads holding
    // `Arc<vk_video::*>` exit before pipeline shutdown.
    _video_thread: Option<ThreadJoiner>,
    _audio_thread: Option<ThreadJoiner>,
}

impl WhepOutput {
    pub fn new(
        ctx: Arc<PipelineCtx>,
        output_ref: Ref<OutputId>,
        options: WhepOutputOptions,
    ) -> Result<Self, OutputInitError> {
        let state_clone = ctx.whip_whep_state.clone();
        let Some(state) = state_clone else {
            return Err(OutputInitError::WhipWhepServerNotRunning);
        };
        let bearer_token = options.bearer_token.clone();

        ctx.stats_sender.send(StatsEvent::NewOutput {
            output_ref: output_ref.clone(),
            kind: OutputProtocolKind::Whep,
        });

        let video_pair = options
            .video
            .as_ref()
            .map(|video| Self::init_video_thread(&ctx, &output_ref, video.clone()))
            .transpose()?;
        let (video_options, video_thread) = match video_pair {
            Some((opts, thread)) => (Some(opts), Some(ThreadJoiner::new(thread))),
            None => (None, None),
        };

        let audio_pair = options
            .audio
            .as_ref()
            .map(|audio| Self::init_audio_thread(&ctx, &output_ref, audio.clone()))
            .transpose()?;
        let (audio_options, audio_thread) = match audio_pair {
            Some((opts, thread)) => (Some(opts), Some(ThreadJoiner::new(thread))),
            None => (None, None),
        };

        state.outputs.add_output(
            &output_ref,
            WhepOutputConnectionStateOptions {
                bearer_token: bearer_token.clone(),
                video_options: video_options.clone(),
                audio_options: audio_options.clone(),
            },
        );

        Ok(Self {
            audio: audio_options.map(|a| a.track_thread_handle),
            video: video_options.map(|v| v.track_thread_handle),
            output_ref,
            outputs_state: state.outputs.clone(),
            _video_thread: video_thread,
            _audio_thread: audio_thread,
        })
    }

    fn init_video_thread(
        ctx: &Arc<PipelineCtx>,
        output_ref: &Ref<OutputId>,
        options: VideoEncoderOptions,
    ) -> Result<(WhepVideoConnectionOptions, std::thread::JoinHandle<()>), OutputInitError> {
        let (sender, receiver) = broadcast::channel(1000);
        let stats_sender = WhepOutputStatsSender::new(ctx.stats_sender.clone(), output_ref.clone());

        let (thread_handle, thread) = match &options {
            VideoEncoderOptions::FfmpegH264(options) => {
                WhepVideoTrackThread::<FfmpegH264Encoder>::spawn(
                    output_ref.clone(),
                    WhepVideoTrackThreadOptions {
                        ctx: ctx.clone(),
                        encoder_options: options.clone(),
                        chunks_sender: sender,
                        stats_sender,
                    },
                )?
            }
            VideoEncoderOptions::VulkanH264(options) => {
                if !ctx.graphics_context.has_vulkan_encoder_support() {
                    return Err(OutputInitError::EncoderError(
                        EncoderInitError::VulkanContextRequiredForVulkanEncoder,
                    ));
                }
                WhepVideoTrackThread::<VulkanH264Encoder>::spawn(
                    output_ref.clone(),
                    WhepVideoTrackThreadOptions {
                        ctx: ctx.clone(),
                        encoder_options: options.clone(),
                        chunks_sender: sender,
                        stats_sender,
                    },
                )?
            }
            VideoEncoderOptions::FfmpegVp8(options) => {
                WhepVideoTrackThread::<FfmpegVp8Encoder>::spawn(
                    output_ref.clone(),
                    WhepVideoTrackThreadOptions {
                        ctx: ctx.clone(),
                        encoder_options: options.clone(),
                        chunks_sender: sender,
                        stats_sender,
                    },
                )?
            }
            VideoEncoderOptions::FfmpegVp9(options) => {
                WhepVideoTrackThread::<FfmpegVp9Encoder>::spawn(
                    output_ref.clone(),
                    WhepVideoTrackThreadOptions {
                        ctx: ctx.clone(),
                        encoder_options: options.clone(),
                        chunks_sender: sender,
                        stats_sender,
                    },
                )?
            }
        };

        Ok((
            WhepVideoConnectionOptions {
                encoder: options,
                receiver: receiver.into(),
                track_thread_handle: thread_handle,
            },
            thread,
        ))
    }

    fn init_audio_thread(
        ctx: &Arc<PipelineCtx>,
        output_ref: &Ref<OutputId>,
        options: AudioEncoderOptions,
    ) -> Result<(WhepAudioConnectionOptions, std::thread::JoinHandle<()>), OutputInitError> {
        let (sender, receiver) = broadcast::channel(1000);
        let stats_sender = WhepOutputStatsSender::new(ctx.stats_sender.clone(), output_ref.clone());

        let (thread_handle, thread) = match options.clone() {
            AudioEncoderOptions::Opus(options) => WhepAudioTrackThread::<OpusEncoder>::spawn(
                output_ref.clone(),
                WhepAudioTrackThreadOptions {
                    ctx: ctx.clone(),
                    encoder_options: options.clone(),
                    chunks_sender: sender,
                    stats_sender,
                },
            )?,
            AudioEncoderOptions::FdkAac(_options) => {
                return Err(OutputInitError::UnsupportedAudioCodec(AudioCodec::Aac));
            }
        };

        Ok((
            WhepAudioConnectionOptions {
                encoder: options,
                receiver: receiver.into(),
                track_thread_handle: thread_handle,
            },
            thread,
        ))
    }
}

impl Drop for WhepOutput {
    fn drop(&mut self) {
        self.outputs_state.remove_output(&self.output_ref);
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

#[derive(Debug, Clone)]
pub(crate) struct WhepOutputStatsSender {
    stats_sender: StatsSender,
    output_ref: Ref<OutputId>,
}

impl WhepOutputStatsSender {
    pub fn new(stats_sender: StatsSender, output_ref: Ref<OutputId>) -> Self {
        Self {
            stats_sender,
            output_ref,
        }
    }

    fn bytes_sent_event(&self, size: usize, track_kind: StatsTrackKind) {
        self.stats_sender.send(
            WhepOutputTrackStatsEvent::BytesSent(size).into_event(&self.output_ref, track_kind),
        );
    }

    pub(super) fn peer_state_changed(&self, session_id: &Arc<str>, state: RTCPeerConnectionState) {
        self.stats_sender.send(
            WhepOutputStatsEvent::PeerStateChanged {
                session_id: session_id.clone(),
                state,
            }
            .into_event(&self.output_ref),
        );
    }
}
