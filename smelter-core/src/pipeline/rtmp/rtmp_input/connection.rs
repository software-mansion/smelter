use std::{sync::Arc, thread::JoinHandle, time::Duration};

use crossbeam_channel::Sender;
use rtmp::{
    AacAudioConfig, AacAudioData, H264VideoConfig, H264VideoData, RtmpEvent, RtmpEventReceiver,
};
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
    Ready(DecoderThreadHandle),
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

    fn handle_rtmp_event(&mut self, rtmp_event: RtmpEvent) {
        match rtmp_event {
            RtmpEvent::H264Config(config) => self.process_video_config(config),
            RtmpEvent::AacConfig(config) => self.process_audio_config(config),
            RtmpEvent::H264Data(data) => self.process_video(data),
            RtmpEvent::AacData(data) => self.process_audio(data),
            RtmpEvent::Metadata(metadata) => info!(?metadata, "Received metadata"), // TODO
            _ => warn!(?rtmp_event, "Unsupported message"),
        }
    }

    fn process_video_config(&mut self, config: H264VideoConfig) {
        let parsed_config = match H264AvcDecoderConfig::parse(config.data) {
            Ok(config) => config,
            Err(err) => {
                warn!(
                    "Failed to parse H264 config: {}",
                    ErrorStack::new(&err).into_string()
                );
                return;
            }
        };

        info!("H264 config received");
        match self.init_h264_decoder(parsed_config) {
            Ok(handle) => self.video_track_state = TrackState::Ready(handle),
            Err(err) => error!(
                "Failed to initialize H264 decoder: {}",
                ErrorStack::new(&*err).into_string()
            ),
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

    fn process_video(&mut self, video: H264VideoData) {
        let sender = match &self.video_track_state {
            TrackState::Ready(handle) => handle.chunk_sender.clone(),
            TrackState::BeforeFirstEvent => {
                warn!("H264 decoder not yet initialized, skipping video until config arrives");
                self.video_track_state = TrackState::ConfigMissing;
                return;
            }
            TrackState::ConfigMissing => return,
        };

        let pts = self.shift_pts_to_queue_offset(video.pts);
        let chunk = EncodedInputChunk {
            data: video.data,
            pts,
            dts: Some(video.dts),
            kind: MediaKind::Video(VideoCodec::H264),
        };

        if sender.send(PipelineEvent::Data(chunk)).is_err() {
            warn!("Video decoder channel closed");
        }
    }

    fn process_audio_config(&mut self, config: AacAudioConfig) {
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
            }
            Err(err) => error!(
                "Failed to init AAC decoder: {}",
                ErrorStack::new(&err).into_string()
            ),
        }
    }

    fn process_audio(&mut self, audio: AacAudioData) {
        let sender = match &self.audio_track_state {
            TrackState::Ready(handle) => handle.chunk_sender.clone(),
            TrackState::BeforeFirstEvent => {
                warn!("AAC decoder not yet initialized, skipping audio until config arrives");
                self.audio_track_state = TrackState::ConfigMissing;
                return;
            }
            TrackState::ConfigMissing => return,
        };

        let pts = self.shift_pts_to_queue_offset(audio.pts);
        let chunk = EncodedInputChunk {
            data: audio.data.clone(),
            pts,
            dts: None,
            kind: MediaKind::Audio(AudioCodec::Aac),
        };

        if sender.send(PipelineEvent::Data(chunk)).is_err() {
            warn!("Audio decoder channel closed");
        }
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

pub(crate) fn start_connection_thread(
    ctx: Arc<PipelineCtx>,
    input_ref: Ref<InputId>,
    receiver: RtmpEventReceiver,
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

            for rtmp_event in receiver {
                state.handle_rtmp_event(rtmp_event);
            }

            info!("RTMP stream connection closed");
        })
        .unwrap()
}
