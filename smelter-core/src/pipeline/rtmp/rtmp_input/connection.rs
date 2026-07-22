use std::{sync::Arc, thread::JoinHandle, time::Duration};

use rtmp::{
    AudioConfig, AudioData, RtmpAudioCodec, RtmpEvent, RtmpRecvTimeoutError, RtmpVideoCodec,
    VideoConfig, VideoData,
};
use smelter_render::{InputId, error::ErrorStack};
use tracing::{Level, info, span, warn};

use crate::{
    MediaKind, PipelineCtx, PipelineEvent, Ref,
    codecs::{
        FdkAacDecoderOptions, H264AvcDecoderConfigError, VideoDecoderOptions,
    },
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
        channel::Sender,
        live_sync::{LiveSync, LiveSyncOptions, LiveSyncTrack},
    },
};

use crate::prelude::*;

const RTMP_BUFFER: Duration = Duration::from_secs(2);
const RTMP_MAX_BUFFER: Duration = Duration::from_secs(20);

pub(crate) fn start_connection_thread(
    ctx: Arc<PipelineCtx>,
    input_ref: &Ref<InputId>,
    input: &RtmpInputState,
    conn: rtmp::RtmpServerConnection,
) -> Option<JoinHandle<()>> {
    let input_id = input_ref.to_string();
    let queue_input = input.queue_input.upgrade()?;
    let (video_sender, audio_sender) = queue_input.queue_new_track(QueueTrackOptions {
        video: true,
        audio: true,
        offset: QueueTrackOffset::Pts(ctx.queue_ctx.effective_last_pts() + RTMP_BUFFER),
    });

    let state = RtmpConnectionState {
        ctx: ctx.clone(),
        input_ref: input_ref.clone(),
        decoders: input.decoders.clone(),
        video_track_state: TrackState::BeforeFirstEvent,
        audio_track_state: TrackState::BeforeFirstEvent,
        video_sender,
        audio_sender,
        first_pts: None,
        conn,
        live_sync: Some(LiveSync::new(
            LiveSyncOptions::with_desired_buffer(Duration::from_secs(2)),
            ctx.queue_ctx.sync_point,
        )),
    };

    let handle = std::thread::Builder::new()
        .name(format!("RTMP thread for input {input_id}"))
        .spawn(move || {
            let _span = span!(Level::INFO, "RTMP thread", input_id = input_id).entered();

            let app: &str = state.conn.app();
            let stream_key: &str = state.conn.stream_key();
            info!(app, stream_key, "RTMP stream connection established");

            state.run();

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
    Ready(
        DecoderThreadHandle,
        Option<LiveSyncTrack<EncodedInputChunk>>,
    ),
}

impl TrackState {
    fn chunk_sender(&mut self) -> Option<Sender<PipelineEvent<EncodedInputChunk>>> {
        match self {
            TrackState::Ready(handle, sync) => Some(handle.chunk_sender.clone()),
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

    #[error("Decoder channel closed")]
    ChannelClosed,

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
    live_sync: Option<LiveSync>,

    video_track_state: TrackState,
    audio_track_state: TrackState,
    video_sender: Option<QueueSender<Frame>>,
    audio_sender: Option<QueueSender<InputAudioSamples>>,

    first_pts: Option<Duration>,

    conn: rtmp::RtmpServerConnection,
}

impl RtmpConnectionState {
    fn run(mut self) {
        loop {
            let event = match self.conn.next_event_timeout(Duration::from_millis(100)) {
                Ok(event) => Some(event),
                Err(RtmpRecvTimeoutError::Timeout) => None,
                Err(RtmpRecvTimeoutError::ConnectionClosed) => break,
            };

            if let Some(event) = event {
                if let Err(err) = self.handle_rtmp_event(event) {
                    match err {
                        RtmpConnectionError::ChannelClosed => break,
                        _ => warn!("{}", ErrorStack::new(&err).into_string()),
                    }
                }
            }

            if let Err(err) = self.process_live_sync_buffer() {
                match err {
                    RtmpConnectionError::ChannelClosed => break,
                    _ => warn!("{}", ErrorStack::new(&err).into_string()),
                }
            }
        }
    }

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

    fn process_live_sync_buffer(&mut self) -> Result<(), RtmpConnectionError> {}

    fn handle_video_chunk(&mut self, video: VideoData) -> Result<(), RtmpConnectionError> {
        let sender = self
            .video_track_state
            .chunk_sender()
            .ok_or(RtmpConnectionError::VideoDecoderNotInitialized)?;

        let pts = self.normalize_pts(video.pts);
        let chunk = EncodedInputChunk {
            data: video.data,
            pts,
            dts: Some(video.dts),
            kind: MediaKind::Video(video.codec.into()),
            present: true,
        };

        self.ctx.stats_sender.send(
            RtmpInputTrackStatsEvent::BytesReceived(chunk.data.len())
                .into_event(&self.input_ref, StatsTrackKind::Video),
        );
        sender
            .send(PipelineEvent::Data(chunk))
            .map_err(|_| RtmpConnectionError::ChannelClosed)?;
        Ok(())
    }

    fn handle_audio_chunk(&mut self, audio: AudioData) -> Result<(), RtmpConnectionError> {
        let sender = self
            .audio_track_state
            .chunk_sender()
            .ok_or(RtmpConnectionError::AudioDecoderNotInitialized)?;

        let pts = self.normalize_pts(audio.pts);
        let chunk = EncodedInputChunk {
            data: audio.data.clone(),
            pts,
            dts: None,
            kind: MediaKind::Audio(audio.codec.into()),
            present: true,
        };

        self.ctx.stats_sender.send(
            RtmpInputTrackStatsEvent::BytesReceived(chunk.data.len())
                .into_event(&self.input_ref, StatsTrackKind::Audio),
        );
        sender
            .send(PipelineEvent::Data(chunk))
            .map_err(|_| RtmpConnectionError::ChannelClosed)?;
        Ok(())
    }

    fn process_video_config(&mut self, config: VideoConfig) -> Result<(), RtmpConnectionError> {
        let Some(frame_sender) = self.video_sender.take() else {
            return Err(RtmpConnectionError::ReceivedSecondVideoTrack);
        };

        let handle = spawn_video_decoder(
            &self.ctx,
            &self.input_ref,
            &self.decoders,
            config,
            frame_sender,
        )?;

        let sync = self.live_sync.as_ref().map(|sync| sync.add_track());
        self.video_track_state = TrackState::Ready(handle, sync);
        Ok(())
    }

    fn process_audio_config(&mut self, config: AudioConfig) -> Result<(), RtmpConnectionError> {
        let Some(samples_sender) = self.audio_sender.take() else {
            return Err(RtmpConnectionError::ReceivedSecondAudioTrack);
        };

        let handle = spawn_audio_decoder(&self.ctx, &self.input_ref, config, samples_sender)?;

        let sync = self.live_sync.as_ref().map(|sync| sync.add_track());
        self.audio_track_state = TrackState::Ready(handle, sync);
        Ok(())
    }

    fn normalize_pts(&mut self, pts: Duration) -> Duration {
        let first_pts = *self.first_pts.get_or_insert(pts);

        // drop unused track, it matters only if input is required
        // and does not have audio or video track. Channels need to be large
        // enough to fit 5 second
        if pts.saturating_sub(first_pts) > Duration::from_secs(5) {
            self.video_sender.take();
            self.audio_sender.take();
        }

        pts.saturating_sub(first_pts)
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
