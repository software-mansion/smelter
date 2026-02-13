use std::{
    sync::{Arc, mpsc},
    thread::JoinHandle,
    time::Duration,
};

use crossbeam_channel::Sender;
use rtmp::{AudioConfig, AudioData, RtmpEvent, VideoConfig, VideoData};
use smelter_render::{Frame, InputId, error::ErrorStack};
use tracing::{Level, error, info, span, warn};

use crate::{
    MediaKind, PipelineCtx, PipelineEvent, Ref,
    codecs::{AudioCodec, FdkAacDecoderOptions, VideoCodec, VideoDecoderOptions},
    pipeline::{
        decoder::{DecoderThreadHandle, fdk_aac::FdkAacDecoder, ffmpeg_h264, vulkan_h264},
        rtmp::rtmp_input::decoder_thread::{
            AudioDecoderThread, AudioDecoderThreadOptions, VideoDecoderThread,
            VideoDecoderThreadOptions,
        },
        utils::{H264AvcDecoderConfig, H264AvccToAnnexB, input_buffer::InputBuffer},
    },
    thread_utils::InitializableThread,
};

use crate::prelude::*;

pub(crate) struct RtmpConnectionOptions {
    pub app: Arc<str>,
    pub stream_key: Arc<str>,
    pub frame_sender: Sender<PipelineEvent<Frame>>,
    pub samples_sender: Sender<PipelineEvent<InputAudioSamples>>,
    pub video_decoders: RtmpServerInputVideoDecoders,
    pub buffer: InputBuffer,
}

enum TrackState {
    BeforeFirstEvent,
    ConfigMissing,
    UnsupportedCodec,
    Ready(DecoderThreadHandle),
}

#[derive(thiserror::Error, Debug)]
enum RtmpConnectionError {
    #[error("Unsupported video codec: {0:?}")]
    UnsupportedVideoCodec(rtmp::VideoCodec),

    #[error("Failed to parse H264 config")]
    ParseH264Config(#[source] Box<dyn std::error::Error>),

    #[error("Failed to initialize H264 decoder")]
    InitH264Decoder(#[source] Box<dyn std::error::Error>),

    #[error("Unsupported audio codec: {0:?}")]
    UnsupportedAudioCodec(rtmp::AudioCodec),

    #[error("Failed to initialize AAC decoder")]
    InitAacDecoder(#[source] Box<dyn std::error::Error>),

    #[error("Decoder channel closed")]
    DecoderChannelClosed,

    #[error("Video decoder not initialized yet")]
    VideoDecoderNotInitialized,

    #[error("Audio decoder not initialized yet")]
    AudioDecoderNotInitialized,
}

struct RtmpConnectionState {
    ctx: Arc<PipelineCtx>,
    input_ref: Ref<InputId>,
    frame_sender: Sender<PipelineEvent<Frame>>,
    samples_sender: Sender<PipelineEvent<InputAudioSamples>>,
    video_decoders: RtmpServerInputVideoDecoders,
    buffer: InputBuffer,

    video_track_state: TrackState,
    audio_track_state: TrackState,

    first_packet_offset: Option<Duration>,
}

impl RtmpConnectionState {
    fn new(ctx: Arc<PipelineCtx>, input_ref: Ref<InputId>, options: RtmpConnectionOptions) -> Self {
        Self {
            ctx,
            input_ref,
            frame_sender: options.frame_sender,
            samples_sender: options.samples_sender,
            video_decoders: options.video_decoders,
            buffer: options.buffer,
            first_packet_offset: None,
            video_track_state: TrackState::BeforeFirstEvent,
            audio_track_state: TrackState::BeforeFirstEvent,
        }
    }

    fn handle_rtmp_event(&mut self, rtmp_event: RtmpEvent) -> Result<(), RtmpConnectionError> {
        match rtmp_event {
            RtmpEvent::VideoConfig(config) => self.process_video_config(config)?,
            RtmpEvent::AudioConfig(config) => self.process_audio_config(config)?,
            RtmpEvent::Video(data) => self.process_video(data)?,
            RtmpEvent::Audio(data) => self.process_audio(data)?,
            RtmpEvent::Metadata(metadata) => info!(?metadata, "Received metadata"), // TODO
        }
        Ok(())
    }

    fn process_video_config(&mut self, config: VideoConfig) -> Result<(), RtmpConnectionError> {
        if config.codec != rtmp::VideoCodec::H264 {
            self.video_track_state = TrackState::UnsupportedCodec;
            return Err(RtmpConnectionError::UnsupportedVideoCodec(config.codec));
        }

        let parsed_config = match H264AvcDecoderConfig::parse(config.data) {
            Ok(config) => config,
            Err(err) => {
                return Err(RtmpConnectionError::ParseH264Config(Box::new(err)));
            }
        };

        match self.init_h264_decoder(parsed_config) {
            Ok(handle) => {
                self.video_track_state = TrackState::Ready(handle);
                Ok(())
            }
            Err(err) => Err(RtmpConnectionError::InitH264Decoder(err)),
        }
    }

    fn init_h264_decoder(
        &mut self,
        h264_config: H264AvcDecoderConfig,
    ) -> Result<DecoderThreadHandle, Box<dyn std::error::Error>> {
        let transformer = H264AvccToAnnexB::new(h264_config);
        let decoder_thread_options = VideoDecoderThreadOptions {
            ctx: self.ctx.clone(),
            transformer: Some(transformer),
            frame_sender: self.frame_sender.clone(),
            input_buffer_size: 10,
        };

        let vulkan_supported = self.ctx.graphics_context.has_vulkan_decoder_support();
        let h264_decoder = self.video_decoders.h264.unwrap_or({
            if vulkan_supported {
                VideoDecoderOptions::VulkanH264
            } else {
                VideoDecoderOptions::FfmpegH264
            }
        });

        let handle = match h264_decoder {
            VideoDecoderOptions::FfmpegH264 => {
                VideoDecoderThread::<ffmpeg_h264::FfmpegH264Decoder, _>::spawn(
                    self.input_ref.clone(),
                    decoder_thread_options,
                )?
            }
            VideoDecoderOptions::VulkanH264 => {
                VideoDecoderThread::<vulkan_h264::VulkanH264Decoder, _>::spawn(
                    self.input_ref.clone(),
                    decoder_thread_options,
                )?
            }
            _ => {
                return Err("Invalid video decoder provided, expected H264".into());
            }
        };

        Ok(handle)
    }

    fn process_video(&mut self, video: VideoData) -> Result<(), RtmpConnectionError> {
        let sender = match &self.video_track_state {
            TrackState::Ready(handle) => handle.chunk_sender.clone(),
            TrackState::BeforeFirstEvent => {
                self.video_track_state = TrackState::ConfigMissing;
                return Err(RtmpConnectionError::VideoDecoderNotInitialized);
            }
            TrackState::ConfigMissing | TrackState::UnsupportedCodec => {
                return Err(RtmpConnectionError::VideoDecoderNotInitialized);
            }
        };

        let (pts, dts) = self.pts_dts_from_timestamps(video.pts, video.dts);
        let chunk = EncodedInputChunk {
            data: video.data,
            pts,
            dts,
            kind: MediaKind::Video(VideoCodec::H264),
        };

        sender
            .send(PipelineEvent::Data(chunk))
            .map_err(|_| RtmpConnectionError::DecoderChannelClosed)?;
        Ok(())
    }

    fn process_audio_config(&mut self, config: AudioConfig) -> Result<(), RtmpConnectionError> {
        if config.codec != rtmp::AudioCodec::Aac {
            self.audio_track_state = TrackState::UnsupportedCodec;
            return Err(RtmpConnectionError::UnsupportedAudioCodec(config.codec));
        }

        let options = FdkAacDecoderOptions {
            asc: Some(config.data.clone()),
        };
        let decoder_thread_options = AudioDecoderThreadOptions::<FdkAacDecoder> {
            ctx: self.ctx.clone(),
            decoder_options: options,
            samples_sender: self.samples_sender.clone(),
            input_buffer_size: 10,
        };
        let handle = AudioDecoderThread::<FdkAacDecoder>::spawn(
            self.input_ref.clone(),
            decoder_thread_options,
        );
        match handle {
            Ok(handle) => {
                self.audio_track_state = TrackState::Ready(handle);
                Ok(())
            }
            Err(err) => Err(RtmpConnectionError::InitAacDecoder(Box::new(err))),
        }
    }

    fn process_audio(&mut self, audio: AudioData) -> Result<(), RtmpConnectionError> {
        let sender = match &self.audio_track_state {
            TrackState::Ready(handle) => handle.chunk_sender.clone(),
            TrackState::BeforeFirstEvent => {
                self.audio_track_state = TrackState::ConfigMissing;
                return Err(RtmpConnectionError::AudioDecoderNotInitialized);
            }
            TrackState::ConfigMissing | TrackState::UnsupportedCodec => {
                return Err(RtmpConnectionError::AudioDecoderNotInitialized);
            }
        };

        let (pts, dts) = self.pts_dts_from_timestamps(audio.pts, audio.dts);
        let chunk = EncodedInputChunk {
            data: audio.data.clone(),
            pts,
            dts,
            kind: MediaKind::Audio(AudioCodec::Aac),
        };

        sender
            .send(PipelineEvent::Data(chunk))
            .map_err(|_| RtmpConnectionError::DecoderChannelClosed)?;
        Ok(())
    }

    fn pts_dts_from_timestamps(
        &mut self,
        pts_ms: i64,
        dts_ms: i64,
    ) -> (Duration, Option<Duration>) {
        let pts = Duration::from_millis(pts_ms.max(0) as u64);
        let dts = Duration::from_millis(dts_ms.max(0) as u64);

        let offset = self
            .first_packet_offset
            .get_or_insert_with(|| self.ctx.queue_sync_point.elapsed().saturating_sub(pts));

        let pts = pts + *offset;
        let dts = dts + *offset;

        self.buffer.recalculate_buffer(pts);
        (pts + self.buffer.size(), Some(dts))
    }
}

pub(crate) fn start_connection_thread(
    ctx: Arc<PipelineCtx>,
    input_ref: Ref<InputId>,
    receiver: mpsc::Receiver<RtmpEvent>,
    options: RtmpConnectionOptions,
) -> JoinHandle<()> {
    std::thread::Builder::new()
        .name(format!("RTMP thread for input {input_ref}"))
        .spawn(move || {
            let _span = span!(
                Level::INFO,
                "RTMP thread",
                input_id = input_ref.id().to_string(),
                app = options.app.to_string(),
                stream_key = options.stream_key.to_string(),
            )
            .entered();
            let mut state = RtmpConnectionState::new(ctx, input_ref, options);
            info!("RTMP stream connection opened");

            while let Ok(rtmp_event) = receiver.recv() {
                if let Err(err) = state.handle_rtmp_event(rtmp_event) {
                    match err {
                        RtmpConnectionError::DecoderChannelClosed => {
                            error!("{}", ErrorStack::new(&err).into_string());
                            break;
                        }
                        _ => warn!("{}", ErrorStack::new(&err).into_string()),
                    }
                }
            }

            info!("RTMP stream connection closed");
        })
        .unwrap()
}
