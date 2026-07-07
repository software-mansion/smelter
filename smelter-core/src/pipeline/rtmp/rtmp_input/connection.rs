use std::{
    sync::Arc,
    thread::JoinHandle,
    time::{Duration, Instant},
};

use rtmp::{
    AudioConfig, AudioData, RtmpAudioCodec, RtmpEvent, RtmpVideoCodec, VideoConfig, VideoData,
};
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
            ffmpeg_h264, ffmpeg_vp8, ffmpeg_vp9,
            libopus::OpusDecoder,
            vulkan_h264,
        },
        rtmp::rtmp_input::state::RtmpInputState,
        utils::{
            H264AvcDecoderConfig, H264AvccToAnnexB,
            live_sync::{GopBuffer, LiveSyncController, LiveSyncOptions, MapDecision},
        },
    },
    queue::{QueueSender, QueueTrackOffset, QueueTrackOptions, WeakQueueInput},
    utils::{InitializableThread, channel::Sender},
};

use crate::prelude::*;

const RTMP_BUFFER: Duration = Duration::from_secs(2);
const RTMP_MAX_BUFFER: Duration = Duration::from_secs(20);
/// Memory bound for packets held while probing for the live edge.
const PROBE_HOLD_MAX: Duration = Duration::from_secs(10);

pub(crate) fn start_connection_thread(
    ctx: Arc<PipelineCtx>,
    input_ref: &Ref<InputId>,
    input: &RtmpInputState,
    conn: rtmp::RtmpServerConnection,
) -> Option<JoinHandle<()>> {
    let input_id = input_ref.to_string();
    // bail early if the input is already unregistered
    input.queue_input.upgrade()?;

    let mut state = RtmpConnectionState {
        ctx,
        input_ref: input_ref.clone(),
        decoders: input.decoders.clone(),
        queue_input: input.queue_input.clone(),
        video_track_state: TrackState::BeforeFirstEvent,
        audio_track_state: TrackState::BeforeFirstEvent,
        video_sender: None,
        audio_sender: None,
        sync: LiveSyncController::new(LiveSyncOptions {
            target_buffer: RTMP_BUFFER,
            ..Default::default()
        }),
        phase: Phase::new_probing(),
        discontinuity_reported: false,
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

enum Phase {
    /// Measuring where the live edge is before producing any timestamps.
    /// Packets are held (only the newest GOP matters) and flushed at join.
    Probing {
        video_config: Option<VideoConfig>,
        audio_config: Option<AudioConfig>,
        held_video: GopBuffer<(VideoData, VideoCodec)>,
        held_audio: GopBuffer<(AudioData, AudioCodec)>,
    },
    Live,
}

impl Phase {
    fn new_probing() -> Self {
        Phase::Probing {
            video_config: None,
            audio_config: None,
            held_video: GopBuffer::new(PROBE_HOLD_MAX),
            held_audio: GopBuffer::new(PROBE_HOLD_MAX),
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
    queue_input: WeakQueueInput,

    video_track_state: TrackState,
    audio_track_state: TrackState,
    video_sender: Option<QueueSender<Frame>>,
    audio_sender: Option<QueueSender<InputAudioSamples>>,

    sync: LiveSyncController,
    phase: Phase,
    discontinuity_reported: bool,
}

impl RtmpConnectionState {
    fn handle_rtmp_event(&mut self, rtmp_event: RtmpEvent) -> Result<(), RtmpConnectionError> {
        match rtmp_event {
            RtmpEvent::VideoConfig(config) => match &mut self.phase {
                Phase::Probing { video_config, .. } => {
                    if video_config.is_some() {
                        return Err(RtmpConnectionError::ReceivedSecondVideoTrack);
                    }
                    *video_config = Some(config);
                }
                Phase::Live => self.process_video_config(config)?,
            },
            RtmpEvent::AudioConfig(config) => match &mut self.phase {
                Phase::Probing { audio_config, .. } => {
                    if audio_config.is_some() {
                        return Err(RtmpConnectionError::ReceivedSecondAudioTrack);
                    }
                    *audio_config = Some(config);
                }
                Phase::Live => self.process_audio_config(config)?,
            },
            RtmpEvent::VideoData(data) => {
                let codec = match data.codec {
                    RtmpVideoCodec::H264 => VideoCodec::H264,
                    RtmpVideoCodec::Vp8 => VideoCodec::Vp8,
                    RtmpVideoCodec::Vp9 => VideoCodec::Vp9,
                };
                self.handle_video(data, codec)?;
            }
            RtmpEvent::AudioData(data) => {
                let codec = match data.codec {
                    RtmpAudioCodec::Aac => AudioCodec::Aac,
                    RtmpAudioCodec::Opus => AudioCodec::Opus,
                };
                self.handle_audio(data, codec)?;
            }
            RtmpEvent::Metadata(metadata) => info!(?metadata, "Received metadata"),
        }
        Ok(())
    }

    fn handle_video(
        &mut self,
        video: VideoData,
        codec: VideoCodec,
    ) -> Result<(), RtmpConnectionError> {
        let now = Instant::now();
        self.sync.observe(video.pts, now);
        self.ctx.stats_sender.send(
            RtmpInputTrackStatsEvent::BytesReceived(video.data.len())
                .into_event(&self.input_ref, StatsTrackKind::Video),
        );
        match &mut self.phase {
            Phase::Probing { held_video, .. } => {
                held_video.push(video.pts, video.is_keyframe, (video, codec));
                self.maybe_join(now)
            }
            Phase::Live => self.send_video(video, codec, now),
        }
    }

    fn handle_audio(
        &mut self,
        audio: AudioData,
        codec: AudioCodec,
    ) -> Result<(), RtmpConnectionError> {
        let now = Instant::now();
        self.sync.observe(audio.pts, now);
        self.ctx.stats_sender.send(
            RtmpInputTrackStatsEvent::BytesReceived(audio.data.len())
                .into_event(&self.input_ref, StatsTrackKind::Audio),
        );
        match &mut self.phase {
            Phase::Probing { held_audio, .. } => {
                held_audio.push(audio.pts, false, (audio, codec));
                self.maybe_join(now)
            }
            Phase::Live => self.send_audio(audio, codec, now),
        }
    }

    fn maybe_join(&mut self, now: Instant) -> Result<(), RtmpConnectionError> {
        if !self.sync.ready_to_join(now) {
            return Ok(());
        }
        let Phase::Probing { held_video, .. } = &self.phase else {
            return Ok(());
        };
        // a video stream is only joinable at a keyframe; wait for one even
        // past the probe cap (audio-only streams don't wait)
        if !held_video.is_empty() && held_video.keyframe_pts().is_none() {
            return Ok(());
        }

        let Phase::Probing {
            video_config,
            audio_config,
            mut held_video,
            mut held_audio,
        } = std::mem::replace(&mut self.phase, Phase::Live)
        else {
            unreachable!();
        };

        // newest held keyframe is the join point; for audio-only streams the
        // newest packet is (the target buffer provides the safety margin)
        let baseline = held_video
            .keyframe_pts()
            .or(held_audio.newest_pts())
            .expect("join is only reachable after at least one data packet");

        let queue_input = self
            .queue_input
            .upgrade()
            .ok_or(RtmpConnectionError::ChannelClosed)?;
        let offset = self
            .sync
            .join(baseline, self.ctx.queue_ctx.effective_last_pts());
        let (video_sender, audio_sender) = queue_input.queue_new_track(QueueTrackOptions {
            video: true,
            audio: true,
            offset: QueueTrackOffset::Pts(offset),
        });
        self.video_sender = video_sender;
        self.audio_sender = audio_sender;

        if let Some(config) = video_config {
            if let Err(err) = self.process_video_config(config) {
                warn!("{}", ErrorStack::new(&err).into_string());
            }
        }
        if let Some(config) = audio_config {
            if let Err(err) = self.process_audio_config(config) {
                warn!("{}", ErrorStack::new(&err).into_string());
            }
        }

        // errors on individual packets must not abort the rest of the flush
        for (_, (video, codec)) in held_video.take() {
            match self.send_video(video, codec, now) {
                Err(RtmpConnectionError::ChannelClosed) => {
                    return Err(RtmpConnectionError::ChannelClosed);
                }
                Err(err) => warn!("{}", ErrorStack::new(&err).into_string()),
                Ok(()) => {}
            }
        }
        for (_, (audio, codec)) in held_audio.take() {
            match self.send_audio(audio, codec, now) {
                Err(RtmpConnectionError::ChannelClosed) => {
                    return Err(RtmpConnectionError::ChannelClosed);
                }
                Err(err) => warn!("{}", ErrorStack::new(&err).into_string()),
                Ok(()) => {}
            }
        }
        Ok(())
    }

    fn process_video_config(&mut self, config: VideoConfig) -> Result<(), RtmpConnectionError> {
        let Some(frame_sender) = self.video_sender.take() else {
            return Err(RtmpConnectionError::ReceivedSecondVideoTrack);
        };

        let codec = config.codec;
        let transformer = match codec {
            RtmpVideoCodec::H264 => {
                let h264_config = H264AvcDecoderConfig::parse(config.data)?;
                Some(H264AvccToAnnexB::new(h264_config))
            }
            _ => None,
        };

        let options = VideoDecoderThreadOptions {
            ctx: self.ctx.clone(),
            transformer,
            frame_sender,
            input_buffer_size: RTMP_MAX_BUFFER,
        };

        let decoder_opt = match codec {
            RtmpVideoCodec::H264 => self.decoders.h264.unwrap_or_else(|| {
                match self.ctx.graphics_context.has_vulkan_decoder_support() {
                    true => VideoDecoderOptions::VulkanH264,
                    false => VideoDecoderOptions::FfmpegH264,
                }
            }),
            RtmpVideoCodec::Vp8 => VideoDecoderOptions::FfmpegVp8,
            RtmpVideoCodec::Vp9 => VideoDecoderOptions::FfmpegVp9,
        };

        let input_ref = self.input_ref.clone();
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

        self.video_track_state = TrackState::Ready(handle);
        Ok(())
    }

    fn process_audio_config(&mut self, config: AudioConfig) -> Result<(), RtmpConnectionError> {
        let Some(samples_sender) = self.audio_sender.take() else {
            return Err(RtmpConnectionError::ReceivedSecondAudioTrack);
        };

        let input_ref = self.input_ref.clone();
        let handle = match config.codec {
            RtmpAudioCodec::Aac => {
                let options = AudioDecoderThreadOptions {
                    ctx: self.ctx.clone(),
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
                    ctx: self.ctx.clone(),
                    decoder_options: (),
                    samples_sender,
                    input_buffer_size: RTMP_MAX_BUFFER,
                };
                AudioDecoderThread::<OpusDecoder>::spawn(input_ref, options)
                    .map_err(RtmpConnectionError::InitAudioDecoder)?
            }
        };

        self.audio_track_state = TrackState::Ready(handle);
        Ok(())
    }

    fn send_video(
        &mut self,
        video: VideoData,
        codec: VideoCodec,
        now: Instant,
    ) -> Result<(), RtmpConnectionError> {
        let sender = self
            .video_track_state
            .chunk_sender()
            .ok_or(RtmpConnectionError::VideoDecoderNotInitialized)?;

        let queue_now = self.ctx.queue_ctx.effective_last_pts();
        let decision =
            self.sync
                .map_video(video.pts, Some(video.dts), video.is_keyframe, now, queue_now);
        let (pts, dts) = match decision {
            MapDecision::Send { pts, dts } => (pts, dts),
            MapDecision::Drop => return Ok(()),
            MapDecision::Discontinuity => {
                self.report_discontinuity();
                return Ok(());
            }
        };
        self.drop_unused_tracks(pts);

        let chunk = EncodedInputChunk {
            data: video.data,
            pts,
            dts,
            kind: MediaKind::Video(codec),
            present: true,
        };
        sender
            .send(PipelineEvent::Data(chunk))
            .map_err(|_| RtmpConnectionError::ChannelClosed)?;
        Ok(())
    }

    fn send_audio(
        &mut self,
        audio: AudioData,
        codec: AudioCodec,
        now: Instant,
    ) -> Result<(), RtmpConnectionError> {
        let sender = self
            .audio_track_state
            .chunk_sender()
            .ok_or(RtmpConnectionError::AudioDecoderNotInitialized)?;

        let queue_now = self.ctx.queue_ctx.effective_last_pts();
        let (pts, dts) = match self.sync.map_audio(audio.pts, now, queue_now) {
            MapDecision::Send { pts, dts } => (pts, dts),
            MapDecision::Drop => return Ok(()),
            MapDecision::Discontinuity => {
                self.report_discontinuity();
                return Ok(());
            }
        };
        self.drop_unused_tracks(pts);

        let chunk = EncodedInputChunk {
            data: audio.data,
            pts,
            dts,
            kind: MediaKind::Audio(codec),
            present: true,
        };
        sender
            .send(PipelineEvent::Data(chunk))
            .map_err(|_| RtmpConnectionError::ChannelClosed)?;
        Ok(())
    }

    /// Drop senders of tracks that never received a config. It matters only
    /// if input is required and does not have audio or video track.
    fn drop_unused_tracks(&mut self, track_pts: Duration) {
        if track_pts > Duration::from_secs(5) {
            self.video_sender.take();
            self.audio_sender.take();
        }
    }

    /// Timestamps jumped mid-connection; the stream is dropped until the
    /// publisher reconnects (a reconnect creates fresh connection state).
    fn report_discontinuity(&mut self) {
        if !self.discontinuity_reported {
            self.discontinuity_reported = true;
            warn!("RTMP stream discontinuity, dropping packets until reconnect");
        }
    }
}
