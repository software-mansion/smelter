use std::{sync::Arc, time::Duration};

use crossbeam_channel::Sender;
use rtmp::{AudioConfig, AudioData, RtmpEvent, VideoConfig, VideoData};
use smelter_render::{Frame, InputId};
use tracing::{error, info, warn};

use crate::{
    MediaKind, PipelineCtx, PipelineEvent, Ref,
    codecs::{AudioCodec, FdkAacDecoderOptions, VideoCodec, VideoDecoderOptions},
    pipeline::{
        decoder::{fdk_aac::FdkAacDecoder, ffmpeg_h264, vulkan_h264},
        rtmp::rtmp_input::decoder_thread::{
            AudioDecoderThread, AudioDecoderThreadOptions, VideoDecoderThread,
            VideoDecoderThreadOptions,
        },
        utils::{H264AvcDecoderConfig, H264AvccToAnnexB, input_buffer::InputBuffer},
    },
    thread_utils::InitializableThread,
};

use crate::prelude::*;

pub(super) struct RtmpConnectionState {
    ctx: Arc<PipelineCtx>,
    input_ref: Ref<InputId>,
    frame_sender: Sender<PipelineEvent<Frame>>,
    samples_sender: Sender<PipelineEvent<InputAudioSamples>>,
    video_decoders: RtmpServerInputVideoDecoders,
    buffer: InputBuffer,

    video_chunk_sender: Option<Sender<PipelineEvent<EncodedInputChunk>>>,
    audio_chunk_sender: Option<Sender<PipelineEvent<EncodedInputChunk>>>,

    first_packet_offset: Option<Duration>,
}

impl RtmpConnectionState {
    pub(super) fn new(
        ctx: Arc<PipelineCtx>,
        input_ref: Ref<InputId>,
        video_sender: Sender<PipelineEvent<Frame>>,
        audio_sender: Sender<PipelineEvent<InputAudioSamples>>,
        video_decoders: RtmpServerInputVideoDecoders,
        buffer: InputBuffer,
    ) -> Self {
        Self {
            ctx,
            input_ref,
            frame_sender: video_sender,
            samples_sender: audio_sender,
            video_decoders,

            video_chunk_sender: None,
            audio_chunk_sender: None,

            buffer,
            first_packet_offset: None,
        }
    }

    pub(super) fn handle_rtmp_event(&mut self, rtmp_event: RtmpEvent) {
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
            warn!(?config.codec, "Unsupported video codec");
            return;
        }

        match H264AvcDecoderConfig::parse(config.data) {
            Ok(parsed_config) => {
                info!("H264 config received");
                self.init_h264_decoder(parsed_config);
            }
            Err(err) => {
                warn!(?err, "Failed to parse H264 config");
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
                self.video_chunk_sender = Some(handle.chunk_sender);
            }
            Err(err) => {
                error!(?err, "Failed to initialize H264 decoder");
            }
        }
    }

    fn process_video(&mut self, video: VideoData) {
        if video.codec != rtmp::VideoCodec::H264 {
            warn!(?video.codec, "Unsupported video codec");
            return;
        }

        let Some(sender) = self.video_chunk_sender.clone() else {
            warn!("Missing H264 decoder, skipping video until config arrives");
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
            warn!(?config.codec, "Unsupported audio codec");
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
                self.audio_chunk_sender = Some(handle.chunk_sender);
            }
            Err(err) => warn!(?err, "Failed to init AAC decoder"),
        }
    }

    fn process_audio(&mut self, audio: AudioData) {
        if audio.codec != rtmp::AudioCodec::Aac {
            return;
        }

        let Some(sender) = self.audio_chunk_sender.clone() else {
            warn!("Missing AAC decoder, skipping audio until config arrives");
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
