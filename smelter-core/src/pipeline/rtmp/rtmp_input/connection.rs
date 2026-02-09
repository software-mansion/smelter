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

struct RtmpConnectionState {
    ctx: Arc<PipelineCtx>,
    input_ref: Ref<InputId>,
    frame_sender: Sender<PipelineEvent<Frame>>,
    samples_sender: Sender<PipelineEvent<InputAudioSamples>>,
    video_decoders: RtmpServerInputVideoDecoders,
    buffer: InputBuffer,

    video_handle: Option<DecoderThreadHandle>,
    audio_handle: Option<DecoderThreadHandle>,

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
            video_handle: None,
            audio_handle: None,
            first_packet_offset: None,
        }
    }

    fn handle_rtmp_event(&mut self, rtmp_event: RtmpEvent) {
        match rtmp_event {
            RtmpEvent::VideoConfig(config) => self.process_video_config(config),
            RtmpEvent::AudioConfig(config) => self.process_audio_config(config),
            RtmpEvent::Video(data) => self.process_video(data),
            RtmpEvent::Audio(data) => self.process_audio(data),
            RtmpEvent::Metadata(metadata) => info!(?metadata, "Received metadata"), // TODO
        }
    }

    fn process_video_config(&mut self, config: VideoConfig) {
        if config.codec != rtmp::VideoCodec::H264 {
            error!(?config.codec, "Unsupported video codec");
            return;
        }

        match H264AvcDecoderConfig::parse(config.data) {
            Ok(parsed_config) => {
                info!("H264 config received");
                self.init_h264_decoder(parsed_config);
            }
            Err(err) => {
                warn!(
                    "Failed to parse H264 config: {}",
                    ErrorStack::new(&err).into_string()
                );
            }
        }
    }

    fn init_h264_decoder(&mut self, h264_config: H264AvcDecoderConfig) {
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
            }
            VideoDecoderOptions::VulkanH264 => {
                VideoDecoderThread::<vulkan_h264::VulkanH264Decoder, _>::spawn(
                    self.input_ref.clone(),
                    decoder_thread_options,
                )
            }
            _ => {
                error!("Invalid video decoder provided, expected H264");
                return;
            }
        };

        match handle {
            Ok(handle) => {
                self.video_handle = Some(handle);
            }
            Err(err) => {
                error!(
                    "Failed to initialize H264 decoder: {}",
                    ErrorStack::new(&err).into_string()
                );
            }
        }
    }

    fn process_video(&mut self, video: VideoData) {
        if video.codec != rtmp::VideoCodec::H264 {
            error!(?video.codec, "Unsupported video codec");
            return;
        }

        let Some(sender) = self.video_handle.as_ref().map(|v| v.chunk_sender.clone()) else {
            warn!("H264 decoder not yet initialized, skipping video until config arrives");
            return;
        };
        let (pts, dts) = self.pts_dts_from_timestamps(video.pts, video.dts);
        let chunk = EncodedInputChunk {
            data: video.data,
            pts,
            dts,
            kind: MediaKind::Video(VideoCodec::H264),
        };

        if sender.send(PipelineEvent::Data(chunk)).is_err() {
            warn!("Video decoder channel closed");
        }
    }

    fn process_audio_config(&mut self, config: AudioConfig) {
        if config.codec != rtmp::AudioCodec::Aac {
            error!(?config.codec, "Unsupported audio codec");
            return;
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
                self.audio_handle = Some(handle);
            }
            Err(err) => error!(
                "Failed to init AAC decoder: {}",
                ErrorStack::new(&err).into_string()
            ),
        }
    }

    fn process_audio(&mut self, audio: AudioData) {
        if audio.codec != rtmp::AudioCodec::Aac {
            error!(?audio.codec, "Unsupported audio codec");
            return;
        }

        let Some(sender) = self.audio_handle.as_ref().map(|a| a.chunk_sender.clone()) else {
            warn!("AAC decoder not yet initialized, skipping audio until config arrives");
            return;
        };
        let (pts, dts) = self.pts_dts_from_timestamps(audio.pts, audio.dts);
        let chunk = EncodedInputChunk {
            data: audio.data.clone(),
            pts,
            dts,
            kind: MediaKind::Audio(AudioCodec::Aac),
        };

        if sender.send(PipelineEvent::Data(chunk)).is_err() {
            warn!("Audio decoder channel closed");
        }
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
                state.handle_rtmp_event(rtmp_event);
            }

            info!("RTMP stream connection closed");
        })
        .unwrap()
}
