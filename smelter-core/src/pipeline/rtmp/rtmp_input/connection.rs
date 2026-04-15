use std::{sync::Arc, thread::JoinHandle, time::Duration};

use rtmp::{AacAudioConfig, AacAudioData, H264VideoConfig, H264VideoData, RtmpEvent};
use smelter_render::{InputId, error::ErrorStack};
use tracing::{Level, info, span, warn};

use crate::{
    MediaKind, PipelineCtx, PipelineEvent, Ref,
    codecs::{
        AudioCodec, FdkAacDecoderOptions, H264AvcDecoderConfigError, VideoCodec,
        VideoDecoderOptions,
    },
    error::DecoderInitError,
    pipeline::{
        decoder::{
            DecoderThreadHandle,
            decoder_thread_audio::{AudioDecoderThread, AudioDecoderThreadOptions},
            decoder_thread_video::{VideoDecoderThread, VideoDecoderThreadOptions},
            fdk_aac::FdkAacDecoder,
            ffmpeg_h264, vulkan_h264,
        },
        rtmp::rtmp_input::state::RtmpInputState,
        utils::{H264AvcDecoderConfig, H264AvccToAnnexB},
    },
    queue::{QueueSender, QueueTrackOffset, QueueTrackOptions},
    utils::{InitializableThread, channel::Sender},
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

    let mut state = RtmpConnectionState {
        ctx,
        input_ref: input_ref.clone(),
        decoders: input.decoders.clone(),
        video_track_state: TrackState::BeforeFirstEvent,
        audio_track_state: TrackState::BeforeFirstEvent,
        video_sender,
        audio_sender,
        first_pts: None,
    };

    let handle = std::thread::Builder::new()
        .name(format!("RTMP thread for input {input_id}"))
        .spawn(move || {
            let _span = span!(Level::INFO, "RTMP thread", input_id = input_id).entered();

            let app: &str = conn.app();
            let stream_key: &str = conn.stream_key();
            info!(app, stream_key, "RTMP stream connection established");

            for event in &conn {
                if let Err(err) = state.handle_rtmp_event(event) {
                    match err {
                        RtmpConnectionError::ChannelClosed => {
                            break;
                        }
                        _ => warn!("{}", ErrorStack::new(&err).into_string()),
                    }
                }
            }

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
    Ready(DecoderThreadHandle),
}

impl TrackState {
    fn chunk_sender(&mut self) -> Option<Sender<PipelineEvent<EncodedInputChunk>>> {
        match self {
            TrackState::Ready(handle) => Some(handle.chunk_sender.clone()),
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

    #[error("Failed to initialize H264 decoder")]
    InitH264Decoder(#[source] DecoderInitError),

    #[error("Invalid video decoder provided, expected H264 decoder")]
    InvalidVideoDecoder,

    #[error("Failed to initialize AAC decoder")]
    InitAacDecoder(#[source] DecoderInitError),

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

    video_track_state: TrackState,
    audio_track_state: TrackState,
    video_sender: Option<QueueSender<Frame>>,
    audio_sender: Option<QueueSender<InputAudioSamples>>,

    first_pts: Option<Duration>,
}

impl RtmpConnectionState {
    fn handle_rtmp_event(&mut self, rtmp_event: RtmpEvent) -> Result<(), RtmpConnectionError> {
        match rtmp_event {
            RtmpEvent::H264Config(config) => self.process_video_config(config)?,
            RtmpEvent::AacConfig(config) => self.process_audio_config(config)?,
            RtmpEvent::H264Data(data) => self.process_video(data)?,
            RtmpEvent::AacData(data) => self.process_audio(data)?,
            RtmpEvent::Metadata(metadata) => info!(?metadata, "Received metadata"),
            _ => warn!(?rtmp_event, "Unsupported message"),
        }
        Ok(())
    }

    fn process_video_config(&mut self, config: H264VideoConfig) -> Result<(), RtmpConnectionError> {
        let Some(frame_sender) = self.video_sender.take() else {
            return Err(RtmpConnectionError::ReceivedSecondVideoTrack);
        };
        let h264_config = H264AvcDecoderConfig::parse(config.data)?;

        let options = VideoDecoderThreadOptions {
            ctx: self.ctx.clone(),
            transformer: Some(H264AvccToAnnexB::new(h264_config)),
            frame_sender,
            input_buffer_size: RTMP_MAX_BUFFER,
        };

        let h264_decoder = self.decoders.h264.unwrap_or_else(|| {
            match self.ctx.graphics_context.has_vulkan_decoder_support() {
                true => VideoDecoderOptions::VulkanH264,
                false => VideoDecoderOptions::FfmpegH264,
            }
        });

        let input_ref = self.input_ref.clone();
        let handle = match h264_decoder {
            VideoDecoderOptions::FfmpegH264 => {
                VideoDecoderThread::<ffmpeg_h264::FfmpegH264Decoder, _>::spawn(input_ref, options)
                    .map_err(RtmpConnectionError::InitH264Decoder)?
            }
            VideoDecoderOptions::VulkanH264 => {
                VideoDecoderThread::<vulkan_h264::VulkanH264Decoder, _>::spawn(input_ref, options)
                    .map_err(RtmpConnectionError::InitH264Decoder)?
            }
            _ => {
                return Err(RtmpConnectionError::InvalidVideoDecoder);
            }
        };

        self.video_track_state = TrackState::Ready(handle);
        Ok(())
    }

    fn process_audio_config(&mut self, config: AacAudioConfig) -> Result<(), RtmpConnectionError> {
        let Some(samples_sender) = self.audio_sender.take() else {
            return Err(RtmpConnectionError::ReceivedSecondAudioTrack);
        };
        let options = AudioDecoderThreadOptions {
            ctx: self.ctx.clone(),
            decoder_options: FdkAacDecoderOptions {
                asc: Some(config.data().clone()),
            },
            samples_sender,
            input_buffer_size: RTMP_MAX_BUFFER,
        };
        let input_ref = self.input_ref.clone();
        let handle = AudioDecoderThread::<FdkAacDecoder>::spawn(input_ref, options)
            .map_err(RtmpConnectionError::InitAacDecoder)?;
        self.audio_track_state = TrackState::Ready(handle);
        Ok(())
    }

    fn process_video(&mut self, video: H264VideoData) -> Result<(), RtmpConnectionError> {
        let sender = self
            .video_track_state
            .chunk_sender()
            .ok_or(RtmpConnectionError::VideoDecoderNotInitialized)?;

        let pts = self.normalize_pts(video.pts);
        let chunk = EncodedInputChunk {
            data: video.data,
            pts,
            dts: Some(video.dts),
            kind: MediaKind::Video(VideoCodec::H264),
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

    fn process_audio(&mut self, audio: AacAudioData) -> Result<(), RtmpConnectionError> {
        let sender = self
            .audio_track_state
            .chunk_sender()
            .ok_or(RtmpConnectionError::AudioDecoderNotInitialized)?;

        let pts = self.normalize_pts(audio.pts);
        let chunk = EncodedInputChunk {
            data: audio.data.clone(),
            pts,
            dts: None,
            kind: MediaKind::Audio(AudioCodec::Aac),
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
