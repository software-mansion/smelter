use std::{sync::Arc, thread::JoinHandle, time::Duration};

use crossbeam_channel::Sender;
use rtmp::{AacAudioConfig, AacAudioData, H264VideoConfig, H264VideoData, RtmpEvent};
use smelter_render::{Frame, InputId, error::ErrorStack};
use tracing::{Level, error, info, span, warn};

use crate::{
    MediaKind, PipelineCtx, PipelineEvent, Ref,
    codecs::{
        AudioCodec, FdkAacDecoderOptions, H264AvcDecoderConfigError, VideoCodec,
        VideoDecoderOptions,
    },
    error::DecoderInitError,
    pipeline::{
        decoder::{DecoderThreadHandle, fdk_aac::FdkAacDecoder, ffmpeg_h264, vulkan_h264},
        rtmp::rtmp_input::{
            decoder_thread::{
                AudioDecoderThread, AudioDecoderThreadOptions, VideoDecoderThread,
                VideoDecoderThreadOptions,
            },
            state::RtmpInputState,
        },
        utils::{
            H264AvcDecoderConfig, H264AvccToAnnexB, InitializableThread, input_buffer::InputBuffer,
        },
    },
};

use crate::prelude::*;

pub(crate) fn start_connection_thread(
    ctx: Arc<PipelineCtx>,
    input_ref: &Ref<InputId>,
    input: &RtmpInputState,
    conn: rtmp::RtmpConnection,
) -> JoinHandle<()> {
    let app = conn.app().to_string();
    let stream_key = conn.stream_key().to_string();
    let input_id = input_ref.to_string();
    let mut state = RtmpConnectionState {
        ctx,
        input_ref: input_ref.clone(),
        frame_sender: input.frame_sender.clone(),
        samples_sender: input.input_samples_sender.clone(),
        video_decoders: input.video_decoders.clone(),
        buffer: input.buffer.clone(),
        first_packet_offset: None,
        video_track_state: TrackState::BeforeFirstEvent,
        audio_track_state: TrackState::BeforeFirstEvent,
    };
    std::thread::Builder::new()
        .name(format!("RTMP thread for input {input_id}"))
        .spawn(move || {
            let _span = span!(
                Level::INFO,
                "RTMP thread",
                input_id = input_id,
                app = app,
                stream_key = stream_key,
            )
            .entered();
            info!("RTMP stream connection established");

            for event in &conn {
                if let Err(err) = state.handle_rtmp_event(event) {
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

enum TrackState {
    BeforeFirstEvent,
    /// This state can be reached only if the first packet for the track is not a config.
    /// It is a separate state from BeforeFirstEvent to log a warning only once.
    ConfigMissing,
    Ready(DecoderThreadHandle),
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
    fn handle_rtmp_event(&mut self, rtmp_event: RtmpEvent) -> Result<(), RtmpConnectionError> {
        match rtmp_event {
            RtmpEvent::H264Config(config) => self.process_video_config(config)?,
            RtmpEvent::AacConfig(config) => self.process_audio_config(config)?,
            RtmpEvent::H264Data(data) => self.process_video(data)?,
            RtmpEvent::AacData(data) => self.process_audio(data)?,
            RtmpEvent::Metadata(metadata) => info!(?metadata, "Received metadata"), // TODO
            _ => warn!(?rtmp_event, "Unsupported message"),
        }
        Ok(())
    }

    fn process_video_config(&mut self, config: H264VideoConfig) -> Result<(), RtmpConnectionError> {
        let parsed_config = H264AvcDecoderConfig::parse(config.data)?;
        let handle = self.init_h264_decoder(parsed_config)?;
        self.video_track_state = TrackState::Ready(handle);
        Ok(())
    }

    fn init_h264_decoder(
        &mut self,
        h264_config: H264AvcDecoderConfig,
    ) -> Result<DecoderThreadHandle, RtmpConnectionError> {
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
                )
                .map_err(RtmpConnectionError::InitH264Decoder)?
            }
            VideoDecoderOptions::VulkanH264 => {
                VideoDecoderThread::<vulkan_h264::VulkanH264Decoder, _>::spawn(
                    self.input_ref.clone(),
                    decoder_thread_options,
                )
                .map_err(RtmpConnectionError::InitH264Decoder)?
            }
            _ => {
                return Err(RtmpConnectionError::InvalidVideoDecoder);
            }
        };

        Ok(handle)
    }

    fn process_video(&mut self, video: H264VideoData) -> Result<(), RtmpConnectionError> {
        let sender = match &self.video_track_state {
            TrackState::Ready(handle) => handle.chunk_sender.clone(),
            TrackState::BeforeFirstEvent => {
                self.video_track_state = TrackState::ConfigMissing;
                return Err(RtmpConnectionError::VideoDecoderNotInitialized);
            }
            TrackState::ConfigMissing => {
                return Err(RtmpConnectionError::VideoDecoderNotInitialized);
            }
        };

        let pts = self.shift_pts_to_queue_offset(video.pts);
        let chunk = EncodedInputChunk {
            data: video.data,
            pts,
            dts: Some(video.dts),
            kind: MediaKind::Video(VideoCodec::H264),
        };

        sender
            .send(PipelineEvent::Data(chunk))
            .map_err(|_| RtmpConnectionError::DecoderChannelClosed)?;
        Ok(())
    }

    fn process_audio_config(&mut self, config: AacAudioConfig) -> Result<(), RtmpConnectionError> {
        let options = FdkAacDecoderOptions {
            asc: Some(config.data().clone()),
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
        )
        .map_err(RtmpConnectionError::InitAacDecoder)?;
        self.audio_track_state = TrackState::Ready(handle);
        Ok(())
    }

    fn process_audio(&mut self, audio: AacAudioData) -> Result<(), RtmpConnectionError> {
        let sender = match &self.audio_track_state {
            TrackState::Ready(handle) => handle.chunk_sender.clone(),
            TrackState::BeforeFirstEvent => {
                self.audio_track_state = TrackState::ConfigMissing;
                return Err(RtmpConnectionError::AudioDecoderNotInitialized);
            }
            TrackState::ConfigMissing => {
                return Err(RtmpConnectionError::AudioDecoderNotInitialized);
            }
        };

        let pts = self.shift_pts_to_queue_offset(audio.pts);
        let chunk = EncodedInputChunk {
            data: audio.data.clone(),
            pts,
            dts: None,
            kind: MediaKind::Audio(AudioCodec::Aac),
        };

        sender
            .send(PipelineEvent::Data(chunk))
            .map_err(|_| RtmpConnectionError::DecoderChannelClosed)?;
        Ok(())
    }

    fn shift_pts_to_queue_offset(&mut self, pts: Duration) -> Duration {
        let offset = *self
            .first_packet_offset
            .get_or_insert_with(|| self.ctx.queue_sync_point.elapsed().saturating_sub(pts));

        let pts = pts + offset;
        self.buffer.recalculate_buffer(pts);
        pts
    }
}
