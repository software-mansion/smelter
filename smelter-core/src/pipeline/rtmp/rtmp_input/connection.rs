use std::{sync::Arc, thread::JoinHandle, time::Duration};

use rtmp::{
    AudioConfig, AudioData, RtmpAudioCodec, RtmpEvent, RtmpVideoCodec, VideoConfig, VideoData,
};
use smelter_render::{InputId, error::ErrorStack};
use tracing::{Level, info, span, trace, warn};

use crate::{
    MediaKind, PipelineCtx, Ref,
    codecs::{FdkAacDecoderOptions, H264AvcDecoderConfigError, VideoDecoderOptions},
    error::DecoderInitError,
    pipeline::{
        decoder::{
            DecoderThreadHandle,
            decoder_thread_audio::{AudioDecoderThread, AudioDecoderThreadOptions},
            decoder_thread_video::{VideoDecoderThread, VideoDecoderThreadOptions},
            fdk_aac::FdkAacDecoder,
            ffmpeg_h264, ffmpeg_vp8, ffmpeg_vp9,
            libopus::OpusDecoder,
            vulkan_h264,
        },
        rtmp::rtmp_input::state::RtmpInputState,
        utils::{H264AvcDecoderConfig, H264AvccToAnnexB},
    },
    queue::{QueueSender, QueueTrackOffset, QueueTrackOptions},
    utils::{
        InitializableThread,
        input_sync::{DecoderHandleInputSyncExt, InputSync, InputSyncTrack, SimpleSync, TrackKind},
        live_sync::{BufferingStrategy, ChunkBuffer, LiveSync, LiveSyncOptions},
    },
};

use crate::prelude::*;

const RTMP_BUFFER: Duration = Duration::from_secs(2);
const RTMP_MAX_BUFFER: Duration = Duration::from_secs(60);

pub(crate) fn start_connection_thread(
    ctx: Arc<PipelineCtx>,
    input_ref: &Ref<InputId>,
    input: &RtmpInputState,
    conn: rtmp::RtmpServerConnection,
    is_live: bool,
) -> Option<JoinHandle<()>> {
    let input_id = input_ref.to_string();
    let queue_input = input.queue_input.upgrade()?;
    let (video_sender, audio_sender) = queue_input.queue_new_track(QueueTrackOptions {
        video: true,
        audio: true,
        offset: match is_live {
            true => QueueTrackOffset::Pts(Duration::ZERO),
            false => QueueTrackOffset::None,
        },
    });
    let sync = match is_live {
        true => InputSync::Live(LiveSync::new(
            LiveSyncOptions::with_desired_buffer(BufferingStrategy::Range {
                min: Duration::from_secs(1),
                max: Duration::from_secs(5),
                desired: RTMP_BUFFER,
            }),
            ctx.queue_ctx.sync_point,
        )),
        false => InputSync::Simple(SimpleSync::new()),
    };

    let mut state = RtmpConnectionState {
        ctx: ctx.clone(),
        input_ref: input_ref.clone(),
        decoders: input.decoders.clone(),
        video_track_state: TrackState::BeforeFirstEvent,
        audio_track_state: TrackState::BeforeFirstEvent,
        video_sender,
        audio_sender,
        sync,
    };

    let handle = std::thread::Builder::new()
        .name(format!("RTMP thread for input {input_id}"))
        .spawn(move || {
            let _span = span!(Level::INFO, "RTMP thread", input_id = input_id).entered();

            let app: &str = conn.app();
            let stream_key: &str = conn.stream_key();
            info!(
                app,
                stream_key, is_live, "RTMP stream connection established"
            );

            for event in &conn {
                if let Err(err) = state.handle_rtmp_event(event) {
                    match err {
                        // decoders are gone, so there is nothing to read the
                        // connection for anymore
                        RtmpConnectionError::TrackClosed => break,
                        _ => warn!("{}", ErrorStack::new(&err).into_string()),
                    }
                }
            }
            state.sync.flush();

            info!("RTMP stream connection closed");
        })
        .unwrap();
    Some(handle)
}

enum TrackState {
    BeforeFirstEvent,
    /// This state can be reached only if the first packet for the track is not a config.
    /// It is a separate state from BeforeFirstEvent to log a warning only once.
    ConfigMissing,
    Ready(InputSyncTrack<ChunkBuffer>),
}

impl TrackState {
    fn try_ready(&mut self) -> Option<&mut InputSyncTrack<ChunkBuffer>> {
        match self {
            TrackState::Ready(sync) => Some(sync),
            TrackState::BeforeFirstEvent => {
                *self = TrackState::ConfigMissing;
                None
            }
            TrackState::ConfigMissing => None,
        }
    }
}

#[derive(thiserror::Error, Debug)]
enum RtmpConnectionError {
    #[error("Failed to parse H264 config")]
    ParseH264Config(#[from] H264AvcDecoderConfigError),

    #[error("Failed to initialize video decoder")]
    InitVideoDecoder(#[source] DecoderInitError),

    #[error("Failed to initialize audio decoder")]
    InitAudioDecoder(#[source] DecoderInitError),

    #[error("Track closed")]
    TrackClosed,

    #[error("Video decoder not initialized yet")]
    VideoDecoderNotInitialized,

    #[error("Audio decoder not initialized yet")]
    AudioDecoderNotInitialized,

    #[error("Video track already configured")]
    ReceivedSecondVideoTrack,

    #[error("Audio track already configured")]
    ReceivedSecondAudioTrack,
}

struct RtmpConnectionState {
    ctx: Arc<PipelineCtx>,
    input_ref: Ref<InputId>,
    decoders: RtmpServerInputDecoders,
    sync: InputSync<ChunkBuffer>,

    video_track_state: TrackState,
    audio_track_state: TrackState,
    video_sender: Option<QueueSender<Frame>>,
    audio_sender: Option<QueueSender<InputAudioSamples>>,
}

impl RtmpConnectionState {
    fn handle_rtmp_event(&mut self, rtmp_event: RtmpEvent) -> Result<(), RtmpConnectionError> {
        match rtmp_event {
            RtmpEvent::VideoConfig(config) => self.process_video_config(config)?,
            RtmpEvent::AudioConfig(config) => self.process_audio_config(config)?,
            RtmpEvent::VideoData(data) => self.handle_video_chunk(data)?,
            RtmpEvent::AudioData(data) => self.handle_audio_chunk(data)?,
            RtmpEvent::Metadata(metadata) => info!(?metadata, "Received metadata"),
        }
        Ok(())
    }

    fn handle_video_chunk(&mut self, video: VideoData) -> Result<(), RtmpConnectionError> {
        let Some(sync) = self.video_track_state.try_ready() else {
            return Err(RtmpConnectionError::VideoDecoderNotInitialized);
        };

        trace!(pts=?video.pts, "Received video chunk");
        let chunk = EncodedInputChunk {
            data: video.data,
            pts: video.pts,
            dts: Some(video.dts),
            kind: MediaKind::Video(video.codec.into()),
            present: true,
        };

        self.ctx.stats_sender.send(
            RtmpInputTrackStatsEvent::BytesReceived(chunk.data.len())
                .into_event(&self.input_ref, StatsTrackKind::Video),
        );
        sync.write_chunk(chunk)
            .map_err(|_| RtmpConnectionError::TrackClosed)?;
        Ok(())
    }

    fn handle_audio_chunk(&mut self, audio: AudioData) -> Result<(), RtmpConnectionError> {
        let Some(sync) = self.audio_track_state.try_ready() else {
            return Err(RtmpConnectionError::AudioDecoderNotInitialized);
        };

        trace!(pts=?audio.pts, "Received audio chunk");
        let chunk = EncodedInputChunk {
            data: audio.data.clone(),
            pts: audio.pts,
            dts: None,
            kind: MediaKind::Audio(audio.codec.into()),
            present: true,
        };

        self.ctx.stats_sender.send(
            RtmpInputTrackStatsEvent::BytesReceived(chunk.data.len())
                .into_event(&self.input_ref, StatsTrackKind::Audio),
        );
        sync.write_chunk(chunk)
            .map_err(|_| RtmpConnectionError::TrackClosed)?;
        Ok(())
    }

    fn process_video_config(&mut self, config: VideoConfig) -> Result<(), RtmpConnectionError> {
        let Some(frame_sender) = self.video_sender.take() else {
            return Err(RtmpConnectionError::ReceivedSecondVideoTrack);
        };
        trace!(?config, "Received video config");

        let handle = spawn_video_decoder(
            &self.ctx,
            &self.input_ref,
            &self.decoders,
            config,
            frame_sender,
        )?;

        let sync = self.sync.add_track_with_handle(TrackKind::Video, handle);
        self.video_track_state = TrackState::Ready(sync);
        Ok(())
    }

    fn process_audio_config(&mut self, config: AudioConfig) -> Result<(), RtmpConnectionError> {
        let Some(samples_sender) = self.audio_sender.take() else {
            return Err(RtmpConnectionError::ReceivedSecondAudioTrack);
        };

        trace!(?config, "Received audio config");
        let handle = spawn_audio_decoder(&self.ctx, &self.input_ref, config, samples_sender)?;

        let sync = self.sync.add_track_with_handle(TrackKind::Audio, handle);
        self.audio_track_state = TrackState::Ready(sync);
        Ok(())
    }
}

fn spawn_video_decoder(
    ctx: &Arc<PipelineCtx>,
    input_ref: &Ref<InputId>,
    decoders: &RtmpServerInputDecoders,
    config: VideoConfig,
    frame_sender: QueueSender<Frame>,
) -> Result<DecoderThreadHandle, RtmpConnectionError> {
    let codec = config.codec;
    let transformer = match codec {
        RtmpVideoCodec::H264 => {
            let h264_config = H264AvcDecoderConfig::parse(config.data)?;
            Some(H264AvccToAnnexB::new(h264_config))
        }
        _ => None,
    };

    let options = VideoDecoderThreadOptions {
        ctx: ctx.clone(),
        transformer,
        frame_sender,
        input_buffer_size: RTMP_MAX_BUFFER,
    };

    let decoder_opt = match codec {
        RtmpVideoCodec::H264 => decoders.h264.unwrap_or_else(|| {
            match ctx.graphics_context.has_vulkan_decoder_support() {
                true => VideoDecoderOptions::VulkanH264,
                false => VideoDecoderOptions::FfmpegH264,
            }
        }),
        RtmpVideoCodec::Vp8 => VideoDecoderOptions::FfmpegVp8,
        RtmpVideoCodec::Vp9 => VideoDecoderOptions::FfmpegVp9,
    };

    let input_ref = input_ref.clone();
    let handle = match decoder_opt {
        VideoDecoderOptions::FfmpegH264 => {
            VideoDecoderThread::<ffmpeg_h264::FfmpegH264Decoder, _>::spawn(input_ref, options)
                .map_err(RtmpConnectionError::InitVideoDecoder)?
        }
        VideoDecoderOptions::VulkanH264 => {
            VideoDecoderThread::<vulkan_h264::VulkanH264Decoder, _>::spawn(input_ref, options)
                .map_err(RtmpConnectionError::InitVideoDecoder)?
        }
        VideoDecoderOptions::FfmpegVp8 => {
            VideoDecoderThread::<ffmpeg_vp8::FfmpegVp8Decoder, _>::spawn(input_ref, options)
                .map_err(RtmpConnectionError::InitVideoDecoder)?
        }
        VideoDecoderOptions::FfmpegVp9 => {
            VideoDecoderThread::<ffmpeg_vp9::FfmpegVp9Decoder, _>::spawn(input_ref, options)
                .map_err(RtmpConnectionError::InitVideoDecoder)?
        }
    };
    Ok(handle)
}

fn spawn_audio_decoder(
    ctx: &Arc<PipelineCtx>,
    input_ref: &Ref<InputId>,
    config: AudioConfig,
    samples_sender: QueueSender<InputAudioSamples>,
) -> Result<DecoderThreadHandle, RtmpConnectionError> {
    let input_ref = input_ref.clone();
    let handle = match config.codec {
        RtmpAudioCodec::Aac => {
            let options = AudioDecoderThreadOptions {
                ctx: ctx.clone(),
                decoder_options: FdkAacDecoderOptions {
                    asc: Some(config.data.clone()),
                },
                samples_sender,
                input_buffer_size: RTMP_MAX_BUFFER,
            };
            AudioDecoderThread::<FdkAacDecoder>::spawn(input_ref, options)
                .map_err(RtmpConnectionError::InitAudioDecoder)?
        }
        RtmpAudioCodec::Opus => {
            let options = AudioDecoderThreadOptions {
                ctx: ctx.clone(),
                decoder_options: (),
                samples_sender,
                input_buffer_size: RTMP_MAX_BUFFER,
            };
            AudioDecoderThread::<OpusDecoder>::spawn(input_ref, options)
                .map_err(RtmpConnectionError::InitAudioDecoder)?
        }
    };
    Ok(handle)
}
